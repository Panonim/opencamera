import { t } from "@opencamera/i18n/mobile";
import { ALL_FRAME_RATES, ALL_RESOLUTIONS, MANUAL_BITRATE, type BitrateSetting, type QualityPreset } from "@opencamera/protocol";
import { X } from "lucide-react";
import { useState, type ReactNode } from "react";
import { controller } from "../lib/connection";
import { useMobile } from "../store";
import { Segmented } from "./ui";

const QUALITY_PRESETS: QualityPreset[] = ["low-latency", "balanced", "high-quality"];

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</h3>
      {children}
    </section>
  );
}

function BitrateControl({ value, onChange }: { value: BitrateSetting; onChange: (v: BitrateSetting) => void }) {
  const [local, setLocal] = useState(value === "auto" ? MANUAL_BITRATE.default : value);
  return (
    <div className="flex flex-col gap-3">
      <Segmented
        value={value === "auto" ? "auto" : "manual"}
        onChange={(m) => onChange(m === "auto" ? "auto" : local)}
        options={[
          { value: "auto", label: t("settings.auto") },
          { value: "manual", label: t("settings.manual") },
        ]}
      />
      {value !== "auto" && (
        <label className="flex items-center gap-3 text-sm">
          <input
            type="range"
            className="flex-1 accent-white"
            min={MANUAL_BITRATE.min}
            max={MANUAL_BITRATE.max}
            step={MANUAL_BITRATE.step}
            value={local}
            onChange={(e) => setLocal(Number(e.target.value))}
            onPointerUp={() => onChange(local)}
            onKeyUp={() => onChange(local)}
          />
          <span className="w-20 text-right tabular-nums text-muted-foreground">{t("settings.mbps", { value: (local / 1000).toFixed(1) })}</span>
        </label>
      )}
    </div>
  );
}

export function SettingsSheet({ onClose }: { onClose: () => void }) {
  const { settings, cameras, capabilities } = useMobile();
  const update = (patch: Parameters<typeof controller.updateSettings>[0]) => void controller.updateSettings(patch);
  // Shows 4K only on cameras that can record it.
  const resolutionOptions = ALL_RESOLUTIONS.filter((r) => r !== "2160p" || capabilities.resolutions.includes(r)).map((r) => ({
    value: r,
    label: r,
    disabled: !capabilities.resolutions.includes(r),
  }));

  return (
    <div className="dark absolute inset-0 z-10 flex flex-col items-center justify-end bg-black/40 duration-200 animate-in fade-in motion-reduce:animate-none" onClick={onClose}>
      <div
        role="dialog"
        aria-label={t("settings.title")}
        onClick={(e) => e.stopPropagation()}
        className="flex max-h-[85%] w-full max-w-lg flex-col gap-5 overflow-y-auto rounded-t-3xl border-x border-t bg-card px-safe pt-2 pb-safe text-foreground shadow-2xl duration-300 ease-out animate-in slide-in-from-bottom motion-reduce:animate-none"
      >
        <div aria-hidden className="mx-auto h-1 w-10 shrink-0 rounded-full bg-muted-foreground/40" />
        <div className="-mt-2 flex items-center justify-between">
          <h2 className="text-base font-semibold">{t("settings.title")}</h2>
          <button aria-label={t("settings.close")} onClick={onClose} className="flex size-10 items-center justify-center rounded-full bg-elevated">
            <X className="size-5" />
          </button>
        </div>

        {cameras.length > 1 && (
          <Row label={t("settings.camera")}>
            <div className="flex flex-col gap-1 rounded-xl bg-elevated p-1">
              {cameras.map((c) => (
                <button
                  key={c.deviceId}
                  onClick={() => update({ deviceId: c.deviceId, facingMode: c.facing ?? settings.facingMode })}
                  className={`h-11 truncate rounded-lg px-3 text-left text-sm ${
                    c.deviceId === settings.deviceId ? "bg-card font-medium shadow-sm" : "text-muted-foreground"
                  }`}
                >
                  {c.label}
                </button>
              ))}
            </div>
          </Row>
        )}

        <Row label={t("settings.resolution")}>
          <Segmented
            value={settings.resolution}
            onChange={(resolution) => update({ resolution })}
            options={resolutionOptions}
          />
        </Row>

        <Row label={t("settings.frameRate")}>
          <Segmented
            value={settings.fps}
            onChange={(fps) => update({ fps })}
            options={ALL_FRAME_RATES.map((f) => ({ value: f, label: t("settings.fps", { fps: f }), disabled: !capabilities.frameRates.includes(f) }))}
          />
        </Row>

        <Row label={t("settings.quality")}>
          <Segmented<QualityPreset>
            value={settings.quality}
            onChange={(quality) => update({ quality })}
            options={QUALITY_PRESETS.map((q) => ({ value: q, label: t(`settings.qualityPreset.${q}`) }))}
          />
        </Row>

        <Row label={t("settings.bitrate")}>
          <BitrateControl value={settings.bitrate} onChange={(bitrate) => update({ bitrate })} />
        </Row>

        <Row label={t("settings.batterySaver")}>
          <Segmented
            value={settings.batterySaver ? "on" : "off"}
            onChange={(v) => update({ batterySaver: v === "on" })}
            options={[
              { value: "on", label: t("settings.on") },
              { value: "off", label: t("settings.off") },
            ]}
          />
        </Row>
        <p className="text-xs leading-relaxed text-muted-foreground">{t("settings.note")}</p>
      </div>
    </div>
  );
}
