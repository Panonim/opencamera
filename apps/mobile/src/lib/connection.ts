import {
  ADAPT_START,
  PROTOCOL_VERSION,
  SCALE_STEPS,
  adaptScale,
  bitrateCeilingKbps,
  degradationPreference,
  orderCodecs,
  parseDesktopMessage,
  profileOf,
  WARNING_START,
  reconnectDelayMs,
  streamProblem,
  sustainedWarning,
  throttledFps,
  type DesktopMessage,
  type FacingMode,
  type IceServer,
  type PhoneMessage,
  type PhonePower,
  type PhoneSettings,
} from "@opencamera/protocol";
import { setMobile, useMobile, type MessageKind } from "../store";
import { CameraError, applyInPlace, capabilitiesOf, listCameras, openCamera } from "./camera";
import { clearCredentials, deviceName, loadCredentials, saveCredentials } from "./credentials";
import { watchPower } from "./power";

/** Shows an error after this many failed tries if the phone never connected. */
const MAX_INITIAL_ATTEMPTS = 4;
const STATS_INTERVAL_MS = 2000;
// iPadOS reports itself as a Mac, so touch support tells them apart.
const IS_IOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);

/** Runs the camera and the link to the computer, while the screens only display its state. */
class Controller {
  private token: string | null = null;
  private ws: WebSocket | null = null;
  private pc: RTCPeerConnection | null = null;
  private sender: RTCRtpSender | null = null;
  private stream: MediaStream | null = null;
  private attempt = 0;
  private everConnected = false;
  private stopped = true;
  private reconnectTimer = 0;
  private disconnectTimer = 0;
  private statsTimer = 0;
  private lastBytes = 0;
  private lastStatsAt = 0;
  private wakeLock: WakeLockSentinel | null = null;
  private iceServers: IceServer[] = [];
  private adapt = ADAPT_START;
  private warn = WARNING_START;

  constructor() {
    document.addEventListener("visibilitychange", () => void this.onVisibility());
    watchPower((patch) => void this.onPower(patch));
  }

  private async onPower(patch: Partial<PhonePower>) {
    const { power, settings } = useMobile.getState();
    const next = { ...power, ...patch };
    const throttleChanged = throttledFps(settings.fps, power, settings.batterySaver) !== throttledFps(settings.fps, next, settings.batterySaver);
    setMobile({ power: next });
    if (this.stopped) return;
    if (throttleChanged) await this.applyEncoding();
    this.reportState();
  }

  setToken(token: string | null) {
    this.token = token;
  }

  /** Turns on the camera and then connects to the computer. */
  async start() {
    if (!this.stopped) return;
    this.stopped = false;
    this.attempt = 0;
    // Starting always turns video back on, even if it was paused last time.
    const settings = { ...useMobile.getState().settings, videoEnabled: true };
    setMobile({ screen: "starting", message: null, settings });
    try {
      await this.openStream(settings);
    } catch (e) {
      this.stop(e instanceof CameraError ? e.code : "unknown");
      return;
    }
    setMobile({ screen: "live", status: "connecting" });
    void this.requestWakeLock();
    this.connect();
  }

  disconnect() {
    this.send({ type: "bye" });
    this.stop("user_disconnect");
  }

  private stop(kind: MessageKind) {
    this.stopped = true;
    clearTimeout(this.reconnectTimer);
    clearTimeout(this.disconnectTimer);
    clearInterval(this.statsTimer);
    this.closePeer();
    this.ws?.close();
    this.ws = null;
    this.releaseStream();
    void this.wakeLock?.release().catch(() => {});
    this.wakeLock = null;
    setMobile({ screen: "message", message: kind, stream: null });
  }

  private releaseStream() {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
  }

  private videoTrack(): MediaStreamTrack | undefined {
    return this.stream?.getVideoTracks()[0];
  }

  private async openStream(settings: PhoneSettings) {
    this.stream?.getTracks().forEach((t) => t.stop());
    const stream = await openCamera(settings);
    this.stream = stream;
    const track = stream.getVideoTracks()[0];
    track.addEventListener("ended", () => void this.onTrackEnded());
    const cameras = await listCameras().catch(() => []);
    const actual = track.getSettings();
    setMobile({
      stream,
      cameras,
      capabilities: capabilitiesOf(track, cameras),
      settings: {
        ...settings,
        deviceId: actual.deviceId ?? settings.deviceId,
        facingMode: (actual.facingMode as FacingMode | undefined) ?? settings.facingMode,
      },
      actual: captureSize(actual, settings.fps),
    });
    if (this.sender) await this.sender.replaceTrack(track);
    await this.applyEncoding();
    this.reportState();
  }

  private pairingProof(): { token?: string; deviceId?: string; deviceSecret?: string } {
    if (this.token) return { token: this.token };
    const creds = loadCredentials();
    if (creds) return { deviceId: creds.deviceId, deviceSecret: creds.deviceSecret };
    return {};
  }

  private connect() {
    if (this.stopped) return;
    const ws = new WebSocket(`wss://${location.host}/ws`);
    this.ws = ws;
    ws.onopen = () => {
      this.send({
        type: "hello",
        protocolVersion: PROTOCOL_VERSION,
        ...this.pairingProof(),
        deviceName: deviceName(),
        userAgent: navigator.userAgent,
      });
    };
    ws.onmessage = (e) => {
      const msg = parseDesktopMessage(String(e.data));
      if (msg) void this.onMessage(msg);
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.closePeer();
      this.scheduleReconnect();
    };
  }

  private scheduleReconnect() {
    if (this.stopped) return;
    if (!this.everConnected && this.attempt >= MAX_INITIAL_ATTEMPTS) {
      this.stop("unreachable");
      return;
    }
    setMobile({ status: this.everConnected ? "reconnecting" : "connecting" });
    clearTimeout(this.reconnectTimer);
    this.reconnectTimer = window.setTimeout(() => this.connect(), reconnectDelayMs(this.attempt++));
  }

  private send(msg: PhoneMessage) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(msg));
  }

  private async onMessage(msg: DesktopMessage) {
    switch (msg.type) {
      case "welcome":
        if (msg.deviceSecret) saveCredentials({ deviceId: msg.deviceId, deviceSecret: msg.deviceSecret, desktopName: msg.desktopName });
        this.token = null;
        setMobile({ desktopName: msg.desktopName });
        this.iceServers = msg.iceServers;
        await this.negotiate();
        break;
      case "reject":
        if (msg.reason === "unknown_device") clearCredentials();
        this.stop(msg.reason);
        break;
      case "answer":
        await this.pc?.setRemoteDescription({ type: "answer", sdp: msg.sdp });
        break;
      case "ice":
        if (msg.candidate) await this.pc?.addIceCandidate(msg.candidate).catch(() => {});
        break;
      case "control":
        await this.updateSettings(msg.settings);
        break;
      case "bye":
        if (msg.reason === "desktop_closed") {
          // The computer app may be restarting, so keep trying to reconnect quietly.
          this.everConnected = true;
          this.ws?.close();
        } else {
          this.stop(msg.reason === "replaced" ? "replaced" : "desktop_disconnect");
        }
        break;
    }
  }

  private async negotiate() {
    this.closePeer();
    this.adapt = ADAPT_START;
    const pc = new RTCPeerConnection({ iceServers: this.iceServers });
    this.pc = pc;
    const track = this.videoTrack() ?? null;
    const transceiver = pc.addTransceiver(track ?? "video", { direction: "sendonly", streams: this.stream ? [this.stream] : [] });
    this.sender = transceiver.sender;
    preferCodec(transceiver);
    pc.onicecandidate = (e) => this.send({ type: "ice", candidate: e.candidate ? e.candidate.toJSON() as { candidate: string } : null });
    pc.onconnectionstatechange = () => {
      if (this.pc !== pc) return;
      clearTimeout(this.disconnectTimer);
      if (pc.connectionState === "connected") {
        this.attempt = 0;
        this.everConnected = true;
        setMobile({ status: "connected" });
        void this.applyEncoding();
        this.startStats();
      } else if (pc.connectionState === "failed") {
        this.ws?.close();
      } else if (pc.connectionState === "disconnected") {
        setMobile({ status: "reconnecting" });
        // Gives the link five seconds to recover on its own before starting over.
        this.disconnectTimer = window.setTimeout(() => this.ws?.close(), 5000);
      }
    };
    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    this.send({ type: "offer", sdp: pc.localDescription!.sdp });
    this.reportState();
  }

  private closePeer() {
    clearInterval(this.statsTimer);
    this.pc?.close();
    this.pc = null;
    this.sender = null;
    this.warn = WARNING_START;
    setMobile({ warning: null });
  }

  private async applyEncoding() {
    if (!this.sender) return;
    const { settings, power } = useMobile.getState();
    const fps = throttledFps(settings.fps, power, settings.batterySaver) ?? settings.fps;
    const params = this.sender.getParameters();
    if (!params.encodings?.length) return;
    params.encodings[0].maxBitrate = bitrateCeilingKbps(settings, fps) * 1000;
    params.encodings[0].maxFramerate = fps;
    params.encodings[0].scaleResolutionDownBy = settings.bitrate === "auto" ? SCALE_STEPS[this.adapt.step] : 1;
    (params as RTCRtpSendParameters & { degradationPreference?: string }).degradationPreference = degradationPreference(settings.quality, IS_IOS);
    await this.sender.setParameters(params).catch(() => {});
  }

  /** Applies setting changes made on the phone or on the computer. */
  async updateSettings(patch: Partial<PhoneSettings>) {
    const prev = useMobile.getState().settings;
    const next: PhoneSettings = { ...prev, ...patch };
    if (patch.facingMode && !patch.deviceId) delete next.deviceId;
    if (next.bitrate !== prev.bitrate) this.adapt = ADAPT_START;
    setMobile({ settings: next });

    if (patch.videoEnabled === false) {
      this.releaseStream();
      await this.sender?.replaceTrack(null);
      setMobile({ stream: null });
      this.reportState();
      return;
    }
    const cameraChanged = !this.stream || next.deviceId !== prev.deviceId || next.facingMode !== prev.facingMode;
    try {
      const track = this.videoTrack();
      if (!cameraChanged && track && (await applyInPlace(track, next))) {
        setMobile({ actual: captureSize(track.getSettings(), next.fps) });
        await this.applyEncoding();
        this.reportState();
      } else {
        await this.openStream(next);
      }
    } catch (e) {
      setMobile({ settings: prev });
      if (e instanceof CameraError && e.code === "permission_denied") this.stop("permission_denied");
      else await this.openStream(prev).catch(() => this.stop("camera_busy"));
    }
  }

  private reportState() {
    const s = useMobile.getState();
    this.send({
      type: "state",
      state: {
        cameras: s.cameras,
        settings: s.settings,
        capabilities: s.capabilities,
        width: s.actual.width,
        height: s.actual.height,
        suspended: document.visibilityState === "hidden",
        power: s.power,
      },
    });
  }

  private startStats() {
    clearInterval(this.statsTimer);
    this.lastBytes = 0;
    this.lastStatsAt = 0;
    this.statsTimer = window.setInterval(() => void this.collectStats(), STATS_INTERVAL_MS);
  }

  private async collectStats() {
    if (!this.pc) return;
    const report = await this.pc.getStats();
    let bytes = 0;
    let fps = 0;
    let limitation = "none";
    let rtt: number | null = null;
    let availableKbps: number | null = null;
    let codecId: string | undefined;
    let encoder: string | null = null;
    let hardware: boolean | null = null;
    report.forEach((r) => {
      if (r.type === "outbound-rtp" && r.kind === "video") {
        bytes = r.bytesSent ?? 0;
        fps = r.framesPerSecond ?? 0;
        limitation = r.qualityLimitationReason ?? "none";
        codecId = r.codecId;
        encoder = r.encoderImplementation ?? null;
        hardware = r.powerEfficientEncoder ?? null;
      } else if (r.type === "remote-inbound-rtp" && r.kind === "video" && typeof r.roundTripTime === "number") {
        rtt = Math.round(r.roundTripTime * 1000);
      } else if (r.type === "candidate-pair" && r.nominated && r.state === "succeeded" && typeof r.availableOutgoingBitrate === "number") {
        availableKbps = r.availableOutgoingBitrate / 1000;
      }
    });
    const now = performance.now();
    const kbps = this.lastStatsAt ? ((bytes - this.lastBytes) * 8) / (now - this.lastStatsAt) : 0;
    this.lastBytes = bytes;
    this.lastStatsAt = now;
    setMobile((s) => ({ actual: { ...s.actual, fps: Math.round(fps) || s.actual.fps } }));
    const { settings, power } = useMobile.getState();
    this.warn = sustainedWarning(this.warn, streamProblem(power.thermal, settings.videoEnabled ? limitation : null), now);
    setMobile({ warning: this.warn.shown });
    if (settings.bitrate === "auto") {
      const step = this.adapt.step;
      const ceilingKbps = bitrateCeilingKbps(settings, throttledFps(settings.fps, power, settings.batterySaver) ?? settings.fps);
      this.adapt = adaptScale(this.adapt, { availableKbps, ceilingKbps });
      if (this.adapt.step !== step) await this.applyEncoding();
    }
    this.send({
      type: "stats",
      stats: {
        bitrateKbps: Math.round(kbps),
        encodeFps: fps,
        rttMs: rtt,
        qualityLimitation: limitation,
        profile: profileOf(codecId ? report.get(codecId)?.sdpFmtpLine : undefined),
        encoder,
        hardware,
        scale: SCALE_STEPS[this.adapt.step],
      },
    });
  }

  private async onVisibility() {
    if (this.stopped) return;
    this.reportState();
    if (document.visibilityState === "visible") {
      void this.requestWakeLock();
      const track = this.videoTrack();
      if (useMobile.getState().settings.videoEnabled && (!track || track.readyState === "ended")) {
        setMobile({ wasSuspended: true });
        await this.openStream(useMobile.getState().settings).catch(() => this.stop("camera_busy"));
      }
    }
  }

  private async onTrackEnded() {
    // Restarts the camera the phone turned off in the background once the page is visible again.
    if (!this.stopped && document.visibilityState === "visible") await this.onVisibility();
  }

  private async requestWakeLock() {
    try {
      this.wakeLock = (await navigator.wakeLock?.request("screen")) ?? null;
    } catch {
      this.wakeLock = null;
    }
  }
}

function captureSize(actual: MediaTrackSettings, fallbackFps: number): { width: number; height: number; fps: number } {
  return { width: actual.width ?? 0, height: actual.height ?? 0, fps: Math.round(actual.frameRate ?? fallbackFps) };
}

function preferCodec(transceiver: RTCRtpTransceiver) {
  const caps = RTCRtpSender.getCapabilities?.("video");
  if (!caps || !transceiver.setCodecPreferences) return;
  transceiver.setCodecPreferences(orderCodecs(caps.codecs));
}

export const controller = new Controller();
