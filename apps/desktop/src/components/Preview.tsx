import { t } from "@opencamera/i18n/desktop";
import { Camera, Loader2, MoonStar, VideoOff } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { onPreviewFrame, useApp } from "../store";

function Overlay({ icon, title, body }: { icon: React.ReactNode; title: string; body?: string }) {
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-card/95 p-6 text-center">
      <div className="flex size-12 items-center justify-center rounded-full border bg-elevated text-muted-foreground">{icon}</div>
      <div>
        <div className="text-sm font-medium">{title}</div>
        {body && <p className="mx-auto mt-1 max-w-sm text-xs leading-relaxed text-muted-foreground">{body}</p>}
      </div>
    </div>
  );
}

// Shows the same picture that video apps get from the virtual camera.
export function Preview() {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [hasFrame, setHasFrame] = useState(false);
  const { state, settings } = useApp();

  useEffect(() => {
    let drawing = false;
    return onPreviewFrame(async (jpeg) => {
      const c = canvas.current;
      if (!c || drawing) return;
      drawing = true;
      try {
        const bitmap = await createImageBitmap(new Blob([jpeg], { type: "image/jpeg" }));
        if (c.width !== bitmap.width) c.width = bitmap.width;
        if (c.height !== bitmap.height) c.height = bitmap.height;
        c.getContext("2d")?.drawImage(bitmap, 0, 0);
        bitmap.close();
        setHasFrame(true);
      } catch {
        // Skips a broken picture and waits for the next one.
      } finally {
        drawing = false;
      }
    });
  }, []);

  const phase = state?.phase ?? "idle";
  useEffect(() => {
    if (phase !== "connected") setHasFrame(false);
  }, [phase]);

  const phone = state?.phone;
  let overlay: React.ReactNode = null;
  if (phase === "connecting" || phase === "reconnecting") {
    overlay = (
      <Overlay
        icon={<Loader2 className="size-5 animate-spin" />}
        title={t(phase === "connecting" ? "preview.connecting" : "preview.reconnecting")}
        body={phase === "reconnecting" ? t("preview.reconnectingBody") : undefined}
      />
    );
  } else if (phase !== "connected") {
    overlay = <Overlay icon={<Camera className="size-5" />} title={t("preview.noPhone")} body={t("preview.noPhoneBody")} />;
  } else if (phone?.suspended) {
    overlay = <Overlay icon={<MoonStar className="size-5" />} title={t("preview.background")} body={t("preview.backgroundBody")} />;
  } else if (phone && !phone.settings.videoEnabled) {
    overlay = <Overlay icon={<VideoOff className="size-5" />} title={t("preview.paused")} body={t("preview.pausedBody")} />;
  } else if (!hasFrame) {
    overlay = <Overlay icon={<Loader2 className="size-5 animate-spin" />} title={t("preview.waiting")} />;
  }

  return (
    <div className="relative aspect-video min-h-64 w-full overflow-hidden rounded-lg border bg-black">
      <canvas ref={canvas} className="size-full object-contain" />
      {settings?.grid && phase === "connected" && (
        <div className="pointer-events-none absolute inset-0 grid grid-cols-3 grid-rows-3">
          {Array.from({ length: 9 }, (_, i) => (
            <div key={i} className="border-[0.5px] border-white/25" />
          ))}
        </div>
      )}
      {overlay}
    </div>
  );
}
