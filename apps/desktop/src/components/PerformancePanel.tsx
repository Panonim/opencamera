import { cn } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import type { PhoneStats } from "@opencamera/protocol";
import { useApp } from "../store";
import { Card, Metric } from "./common";
import { StreamWarning } from "./StreamWarning";

const PROFILE = { baseline: "H.264 Baseline", main: "H.264 Main", high: "H.264 High" } as const;

function encoderText(p: PhoneStats) {
  return p.hardware == null ? p.encoder : `${p.encoder} · ${t(p.hardware ? "performance.hardware" : "performance.software")}`;
}

const QUALITY_CLASS = { good: "text-success", fair: "text-warning", poor: "text-danger" } as const;

function qualityBadge(paused: boolean, quality: keyof typeof QUALITY_CLASS | "none") {
  if (paused) return { label: t("performance.paused"), className: "text-muted-foreground" };
  if (quality === "none") return { label: "-", className: "text-muted-foreground" };
  return { label: t(`performance.quality.${quality}`), className: QUALITY_CLASS[quality] };
}

// Shows how smooth and sharp the phone video is right now.
export function PerformancePanel() {
  const stats = useApp((s) => s.stats);
  const connected = useApp((s) => s.state?.phase === "connected");
  const paused = useApp((s) => !!s.state?.phone && (s.state.phone.suspended || !s.state.phone.settings.videoEnabled));
  const live = !!stats && stats.quality !== "none" && !paused;
  const q = qualityBadge(paused, stats?.quality ?? "none");
  const scale = stats?.phone?.scale ?? 1;
  const show = (value: string) => (live ? value : "-");

  return (
    <Card title={t("performance.title")} badge={connected && <span className={cn("text-xs font-medium", q.className)}>{q.label}</span>} collapsed={!connected}>
      <Metric label={t("performance.resolution")} value={show(stats?.width ? `${stats.width}×${stats.height}` : "-")} />
      <Metric label={t("performance.frameRate")} value={show(t("common.fps", { fps: stats?.fps.toFixed(0) ?? 0 }))} />
      <Metric label={t("performance.bitrate")} value={show(t("common.mbps", { value: ((stats?.bitrateKbps ?? 0) / 1000).toFixed(1) }))} />
      <Metric label={t("performance.packetLoss")} value={show(t("common.percent", { value: stats?.packetLoss.toFixed(1) ?? 0 }))} />
      <Metric label={t("performance.roundTrip")} value={show(stats?.rttMs != null ? t("common.ms", { value: Math.round(stats.rttMs) }) : "-")} />
      <Metric label={t("performance.droppedFrames")} value={show(String(stats?.droppedFrames ?? 0))} />
      <Metric label={t("performance.codec")} value={show(stats?.phone?.profile ? PROFILE[stats.phone.profile] : "-")} />
      {live && stats.phone?.encoder && <Metric label={t("performance.encoder")} value={encoderText(stats.phone)} />}
      <StreamWarning live={live} />
      {live && scale > 1 && <p className="mt-2 text-xs leading-relaxed text-muted-foreground">{t("performance.scaled", { scale })}</p>}
    </Card>
  );
}
