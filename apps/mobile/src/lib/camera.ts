import {
  RESOLUTIONS,
  supportedFrameRates,
  supportedResolutions,
  type CameraInfo,
  type FacingMode,
  type PhoneCapabilities,
  type PhoneSettings,
} from "@opencamera/protocol";

export type CameraErrorCode = "permission_denied" | "no_camera" | "camera_busy" | "unsupported" | "unknown";

export class CameraError extends Error {
  constructor(public code: CameraErrorCode) {
    super(code);
  }
}

export function browserSupportError(): CameraErrorCode | null {
  const supported = window.isSecureContext && !!navigator.mediaDevices?.getUserMedia && !!window.RTCPeerConnection;
  return supported ? null : "unsupported";
}

function toCameraError(e: unknown): CameraError {
  const name = e instanceof DOMException ? e.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") return new CameraError("permission_denied");
  if (name === "NotFoundError" || name === "OverconstrainedError") return new CameraError("no_camera");
  if (name === "NotReadableError" || name === "AbortError") return new CameraError("camera_busy");
  return new CameraError("unknown");
}

function videoConstraints(s: PhoneSettings): MediaTrackConstraints {
  const { width, height } = RESOLUTIONS[s.resolution];
  return {
    ...(s.deviceId ? { deviceId: { exact: s.deviceId } } : { facingMode: { ideal: s.facingMode } }),
    width: { ideal: width },
    height: { ideal: height },
    frameRate: { ideal: s.fps },
  };
}

export async function openCamera(s: PhoneSettings): Promise<MediaStream> {
  try {
    return await navigator.mediaDevices.getUserMedia({ video: videoConstraints(s), audio: false });
  } catch (e) {
    throw toCameraError(e);
  }
}

/** Tries to change video size and frame rate without restarting the camera. */
export async function applyInPlace(track: MediaStreamTrack, s: PhoneSettings): Promise<boolean> {
  try {
    const { deviceId: _d, facingMode: _f, ...rest } = videoConstraints(s);
    await track.applyConstraints(rest);
    return true;
  } catch {
    return false;
  }
}

function guessFacing(label: string): FacingMode | undefined {
  if (/front|user|face/i.test(label)) return "user";
  if (/back|rear|environment/i.test(label)) return "environment";
  return undefined;
}

/** Lists the phone's cameras, whose names only show after camera access is allowed. */
export async function listCameras(): Promise<CameraInfo[]> {
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices
    .filter((d) => d.kind === "videoinput")
    .map((d, i) => ({ deviceId: d.deviceId, label: d.label || `Camera ${i + 1}`, facing: guessFacing(d.label) }));
}

export function capabilitiesOf(track: MediaStreamTrack, cameras: CameraInfo[]): PhoneCapabilities {
  const caps = track.getCapabilities?.();
  return {
    resolutions: supportedResolutions(caps?.width?.max, caps?.height?.max),
    frameRates: supportedFrameRates(caps?.frameRate?.max),
    canSwitchCamera: cameras.length > 1,
  };
}
