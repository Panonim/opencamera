import { t } from "@opencamera/i18n/mobile";
import { throttledFps, type StreamWarning } from "@opencamera/protocol";
import { BatteryLow, Cpu, Lock, Maximize2, Minimize2, RefreshCcw, RotateCcw, Settings2, Thermometer, Video, VideoOff, WifiLow, X } from "lucide-react";
import { useEffect, useRef, useState, type ComponentType, type ReactNode } from "react";
import { controller } from "../lib/connection";
import { setMobile, useMobile } from "../store";
import { SettingsSheet } from "./SettingsSheet";
import { RoundButton } from "./ui";

const STATUS_DOT = {
  connecting: "bg-warning animate-pulse",
  connected: "bg-success",
  reconnecting: "bg-warning animate-pulse",
} as const;

const WARNING_ICON: Record<StreamWarning, ComponentType<{ className?: string }>> = { hot: Thermometer, cpu: Cpu, bandwidth: WifiLow };

function useFullscreen() {
  const [on, setOn] = useState(!!document.fullscreenElement);
  useEffect(() => {
    const sync = () => setOn(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", sync);
    return () => document.removeEventListener("fullscreenchange", sync);
  }, []);
  const toggle = () => void (on ? document.exitFullscreen() : document.documentElement.requestFullscreen()).catch(() => {});
  return { supported: !!document.fullscreenEnabled, on, toggle };
}

// Browsers can't read the rotation lock, so a rotation while streaming is the sign that it's off.
function useRotationHint(streaming: boolean) {
  const [state, setState] = useState<"hidden" | "shown" | "dismissed">("hidden");
  useEffect(() => {
    if (!streaming) return;
    const onRotate = () => setState((s) => (s === "hidden" ? "shown" : s));
    const target = screen.orientation ?? window;
    const event = screen.orientation ? "change" : "orientationchange";
    target.addEventListener(event, onRotate);
    return () => target.removeEventListener(event, onRotate);
  }, [streaming]);
  return { show: state === "shown", dismiss: () => setState("dismissed") };
}

function Notice({ icon, title, children, onDismiss }: { icon: ReactNode; title: string; children: ReactNode; onDismiss: () => void }) {
  return (
    <div
      role="status"
      className="flex items-start gap-3 rounded-2xl bg-black/65 p-3.5 pr-2 shadow-lg ring-1 ring-white/10 backdrop-blur-xl duration-300 animate-in fade-in slide-in-from-bottom-2 motion-reduce:animate-none"
    >
      <div className="relative flex size-10 shrink-0 items-center justify-center rounded-full bg-white/15">{icon}</div>
      <div className="min-w-0 flex-1 pt-0.5">
        <p className="text-[15px] font-semibold">{title}</p>
        <p className="mt-0.5 text-[13px] leading-snug text-white/70">{children}</p>
      </div>
      <button
        aria-label={t("live.dismiss")}
        onClick={onDismiss}
        className="flex size-9 shrink-0 items-center justify-center rounded-full text-white/60 active:bg-white/10"
      >
        <X className="size-4" />
      </button>
    </div>
  );
}

export function Live() {
  const { stream, status, desktopName, settings, actual, capabilities, wasSuspended, power, warning } = useMobile();
  const video = useRef<HTMLVideoElement>(null);
  const [sheet, setSheet] = useState(false);
  const fullscreen = useFullscreen();
  const WarningIcon = warning && WARNING_ICON[warning];
  const rotation = useRotationHint(!!stream);
  const saverFps = throttledFps(settings.fps, power, settings.batterySaver);

  useEffect(() => {
    if (video.current) video.current.srcObject = stream;
  }, [stream]);

  const mirrored = settings.facingMode === "user";
  const statusText = status === "connected" && desktopName ? t("live.connectedTo", { name: desktopName }) : t(`live.${status}`);
  const otherFacing = settings.facingMode === "user" ? "environment" : "user";

  return (
    <main className="relative h-full overflow-hidden bg-black text-white">
      {stream ? (
        <video ref={video} autoPlay muted playsInline className={`absolute inset-0 size-full object-cover ${mirrored ? "-scale-x-100" : ""}`} />
      ) : (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-white/70">
          <VideoOff className="size-10" strokeWidth={1.5} />
          <p className="text-sm">{t("live.videoPaused")}</p>
        </div>
      )}

      <header className="absolute inset-x-0 top-0 flex items-start justify-between gap-3 bg-gradient-to-b from-black/60 to-transparent px-safe pt-safe pb-8">
        <div className="flex min-w-0 flex-col gap-1.5">
          <div className="flex items-center gap-2 rounded-full bg-black/45 px-3 py-1.5 text-[13px] font-medium ring-1 ring-white/15 backdrop-blur-md">
            <span className={`size-2 shrink-0 rounded-full ${STATUS_DOT[status]}`} />
            <span className="truncate">{statusText}</span>
          </div>
          {stream && actual.width > 0 && (
            <span className="w-fit rounded-full bg-black/45 px-3 py-1 text-xs tabular-nums text-white/80 backdrop-blur-md">{t("live.actual", actual)}</span>
          )}
          {saverFps && (
            <span className="flex w-fit items-center gap-1.5 rounded-full bg-warning/80 px-3 py-1 text-xs font-medium text-black backdrop-blur-md">
              <BatteryLow className="size-3.5" /> {t("live.batterySaver", { fps: saverFps })}
            </span>
          )}
          {warning && WarningIcon && (
            <span
              role="status"
              className="flex w-fit items-center gap-1.5 rounded-full bg-warning/80 px-3 py-1 text-xs font-medium text-black backdrop-blur-md"
            >
              <WarningIcon className="size-3.5" /> {t(`live.warning.${warning}`)}
            </span>
          )}
        </div>
        <div className="flex shrink-0 gap-2">
          {fullscreen.supported && (
            <RoundButton size="sm" label={t(fullscreen.on ? "live.exitFullscreen" : "live.fullscreen")} onClick={fullscreen.toggle}>
              {fullscreen.on ? <Minimize2 className="size-5" /> : <Maximize2 className="size-5" />}
            </RoundButton>
          )}
          <RoundButton size="sm" label={t("live.disconnect")} onClick={() => controller.disconnect()}>
            <X className="size-5" />
          </RoundButton>
        </div>
      </header>

      {(wasSuspended || rotation.show) && (
        <div className="absolute inset-x-0 bottom-[calc(max(env(safe-area-inset-bottom),16px)+96px)] z-10 px-safe">
          <div className="mx-auto flex max-w-md flex-col gap-2">
            {wasSuspended && (
              <Notice icon={<VideoOff className="size-5" />} title={t("live.pausedTitle")} onDismiss={() => setMobile({ wasSuspended: false })}>
                {t("live.pausedBody")}
              </Notice>
            )}
            {rotation.show && (
              <Notice
                icon={
                  <>
                    <RotateCcw className="size-5" />
                    <Lock className="absolute size-2.5" strokeWidth={3} />
                  </>
                }
                title={t("live.rotationTitle")}
                onDismiss={rotation.dismiss}
              >
                {t("live.rotationBody")}
              </Notice>
            )}
          </div>
        </div>
      )}

      <footer className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/60 to-transparent px-safe pt-10 pb-safe">
        <div className="mx-auto flex max-w-sm items-center justify-around">
          <RoundButton label={t("live.settings")} onClick={() => setSheet(true)}>
            <Settings2 className="size-6" />
          </RoundButton>
          <RoundButton
            size="lg"
            active={settings.videoEnabled}
            label={t(settings.videoEnabled ? "live.pause" : "live.resume")}
            onClick={() =>
              void controller.updateSettings({
                videoEnabled: !settings.videoEnabled,
              })
            }
          >
            {settings.videoEnabled ? <Video className="size-7" /> : <VideoOff className="size-7" />}
          </RoundButton>
          <RoundButton
            label={t("live.switchCamera")}
            disabled={!capabilities.canSwitchCamera || !settings.videoEnabled}
            onClick={() => void controller.updateSettings({ facingMode: otherFacing })}
          >
            <RefreshCcw className="size-6" />
          </RoundButton>
        </div>
      </footer>

      {sheet && <SettingsSheet onClose={() => setSheet(false)} />}
    </main>
  );
}
