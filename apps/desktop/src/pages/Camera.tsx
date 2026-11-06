import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { t } from "@opencamera/i18n/desktop";
import { ALL_FRAME_RATES, ALL_RESOLUTIONS, MANUAL_BITRATE, type BitrateSetting, type PhoneSettings, type QualityPreset } from "@opencamera/protocol";
import { Monitor, Smartphone } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge, Card, Page, PageHeader, SaveBar, SettingRow } from "../components/common";
import { PresetsCard } from "../components/PresetsCard";
import { Segmented } from "../components/Segmented";
import { api, type Processing } from "../lib/api";
import type { Preset } from "../lib/presets";
import { useApp } from "../store";

const QUALITY_PRESETS: QualityPreset[] = ["low-latency", "balanced", "high-quality"];
const PLACEHOLDERS: Processing["placeholder"][] = ["card", "black", "freeze"];

function LevelSlider({ value, onCommit }: { value: number; onCommit: (v: number) => void }) {
  const [local, setLocal] = useState(value);
  return (
    <div className="flex w-56 items-center gap-3">
      <Slider min={-100} max={100} step={1} value={[local]} onValueChange={([v]) => setLocal(v)} onValueCommit={([v]) => onCommit(v)} />
      <span className="w-9 text-right text-xs tabular-nums text-muted-foreground">{local > 0 ? `+${local}` : local}</span>
    </div>
  );
}

function BitrateControl({ value, onChange, disabled }: { value: BitrateSetting; onChange: (v: BitrateSetting) => void; disabled?: boolean }) {
  const [local, setLocal] = useState(value === "auto" ? MANUAL_BITRATE.default : value);
  return (
    <div className="flex flex-col items-end gap-3">
      <Segmented
        disabled={disabled}
        value={value === "auto" ? "auto" : "manual"}
        onChange={(m) => onChange(m === "auto" ? "auto" : local)}
        options={[
          { value: "auto", label: t("camera.auto") },
          { value: "manual", label: t("camera.manual") },
        ]}
      />
      {value !== "auto" && (
        <div className="flex w-64 max-w-full items-center gap-3">
          <Slider
            disabled={disabled}
            min={MANUAL_BITRATE.min}
            max={MANUAL_BITRATE.max}
            step={MANUAL_BITRATE.step}
            value={[local]}
            onValueChange={([v]) => setLocal(v)}
            onValueCommit={([v]) => onChange(v)}
          />
          <span className="w-[4.5rem] shrink-0 text-right text-xs font-medium whitespace-nowrap tabular-nums">{t("common.mbps", { value: (local / 1000).toFixed(1) })}</span>
        </div>
      )}
    </div>
  );
}

// Shows requested phone settings until the phone confirms them.
function usePendingPhoneSettings(actual: PhoneSettings | undefined) {
  const [pending, setPending] = useState<Partial<PhoneSettings>>({});
  useEffect(() => {
    setPending((p) => {
      if (!actual) return {};
      const left = Object.fromEntries(Object.entries(p).filter(([k, v]) => actual[k as keyof PhoneSettings] !== v));
      return Object.keys(left).length === Object.keys(p).length ? p : left;
    });
  }, [actual]);
  useEffect(() => {
    if (!Object.keys(pending).length) return;
    const t = setTimeout(() => setPending({}), 6000);
    return () => clearTimeout(t);
  }, [pending]);
  const request = (patch: Partial<PhoneSettings>) => {
    setPending((p) => ({ ...p, ...patch }));
    void api.setPhoneSettings(patch);
  };
  return [actual && { ...actual, ...pending }, request] as const;
}

export function Camera() {
  const state = useApp((s) => s.state);
  const settings = useApp((s) => s.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const phone = state?.phase === "connected" ? state.phone : null;
  const [s, setPhone] = usePendingPhoneSettings(phone?.settings);
  const saved = settings && { processing: settings.processing, grid: settings.grid };
  const [draft, setDraft] = useState<{ processing: Processing; grid: boolean } | null>(saved);
  const [saving, setSaving] = useState(false);
  if (!state || !settings || !draft) return null;
  const caps = phone?.capabilities;
  const p = draft.processing;
  const setP = (patch: Partial<Processing>) => setDraft((d) => ({ ...d!, processing: { ...d!.processing, ...patch } }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const save = () => {
    setSaving(true);
    saveSettings(draft).finally(() => setSaving(false));
  };

  const applyPreset = (preset: Preset) => {
    if (phone) setPhone(preset.phone);
    setDraft((d) => ({ ...d!, processing: preset.processing }));
    void saveSettings({ processing: preset.processing });
  };

  return (
    <Page className="max-w-3xl">
      <PageHeader title={t("nav.camera")} description={t("camera.description")} />
      <div className="flex flex-col gap-5">
        <PresetsCard presets={settings.presets} phone={s} processing={p} onChange={(presets) => void saveSettings({ presets })} onApply={applyPreset} />
        <Card
          title={t("camera.phoneTitle")}
          badge={
            <Badge>
              <Smartphone className="size-3" /> {t("camera.phoneBadge")}
            </Badge>
          }
        >
          {!phone && <p className="mb-3 text-sm text-muted-foreground">{t("camera.needsPhone")}</p>}
          <div className="divide-y">
            <SettingRow label={t("camera.lens")} description={t("camera.lensBody")} disabled={!phone}>
              <Select value={s?.deviceId ?? ""} onValueChange={(deviceId) => setPhone({ deviceId })} disabled={!phone || !phone.cameras.length}>
                <SelectTrigger className="w-56">
                  <SelectValue placeholder={t("camera.defaultLens")} />
                </SelectTrigger>
                <SelectContent>
                  {phone?.cameras.map((c) => (
                    <SelectItem key={c.deviceId} value={c.deviceId}>
                      {c.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </SettingRow>
            <SettingRow label={t("camera.resolution")} description={t("camera.resolutionBody")} disabled={!phone}>
              <Select value={s?.resolution ?? ""} onValueChange={(resolution) => setPhone({ resolution: resolution as never })} disabled={!phone}>
                <SelectTrigger className="w-56">
                  <SelectValue placeholder="-" />
                </SelectTrigger>
                <SelectContent>
                  {ALL_RESOLUTIONS.map((r) => (
                    <SelectItem key={r} value={r} disabled={!caps?.resolutions.includes(r)}>
                      {r}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </SettingRow>
            <SettingRow label={t("camera.frameRate")} disabled={!phone}>
              <Segmented
                disabled={!phone}
                value={s?.fps ?? 30}
                onChange={(fps) => setPhone({ fps })}
                options={ALL_FRAME_RATES.filter((f) => !caps || caps.frameRates.includes(f)).map((f) => ({ value: f, label: t("common.fps", { fps: f }) }))}
              />
            </SettingRow>
            <SettingRow label={t("camera.quality")} description={t("camera.qualityBody")} disabled={!phone}>
              <Segmented<QualityPreset>
                disabled={!phone}
                value={s?.quality ?? "balanced"}
                onChange={(quality) => setPhone({ quality })}
                options={QUALITY_PRESETS.map((q) => ({ value: q, label: t(`camera.qualityPreset.${q}`) }))}
              />
            </SettingRow>
            <SettingRow label={t("camera.bitrate")} description={t("camera.bitrateBody")} disabled={!phone}>
              <BitrateControl disabled={!phone} value={s?.bitrate ?? "auto"} onChange={(bitrate) => setPhone({ bitrate })} />
            </SettingRow>
            <SettingRow label={t("camera.batterySaver")} description={t("camera.batterySaverBody")} disabled={!phone}>
              <Switch checked={s?.batterySaver ?? true} disabled={!phone} onCheckedChange={(batterySaver) => setPhone({ batterySaver })} />
            </SettingRow>
            <SettingRow label={t("camera.cameraOn")} description={t("camera.cameraOnBody")} disabled={!phone}>
              <Switch checked={s?.videoEnabled ?? false} disabled={!phone} onCheckedChange={(videoEnabled) => setPhone({ videoEnabled })} />
            </SettingRow>
          </div>
        </Card>

        <Card
          title={t("camera.computerTitle")}
          badge={
            <Badge>
              <Monitor className="size-3" /> {t("camera.computerBadge")}
            </Badge>
          }
        >
          <div className="divide-y">
            <SettingRow label={t("camera.mirror")} description={t("camera.mirrorBody")}>
              <Switch checked={p.mirror} onCheckedChange={(mirror) => setP({ mirror })} />
            </SettingRow>
            <SettingRow label={t("camera.rotation")}>
              <Segmented value={p.rotation} onChange={(rotation) => setP({ rotation })} options={[0, 90, 180, 270].map((r) => ({ value: r as Processing["rotation"], label: `${r}°` }))} />
            </SettingRow>
            <SettingRow label={t("camera.framing")} description={t("camera.framingBody")}>
              <Segmented value={p.framing} onChange={(framing) => setP({ framing })} options={[{ value: "fit", label: t("camera.fit") }, { value: "fill", label: t("camera.fill") }]} />
            </SettingRow>
            <SettingRow label={t("camera.outputSize")} description={t("camera.outputSizeBody")}>
              <Segmented
                value={p.outputSize}
                onChange={(outputSize) => setP({ outputSize })}
                options={[
                  { value: "720p", label: "720p" },
                  { value: "1080p", label: "1080p" },
                  { value: "2160p", label: "4K" },
                ]}
              />
            </SettingRow>
            <SettingRow label={t("camera.brightness")}>
              <LevelSlider key={`b${p.brightness}`} value={p.brightness} onCommit={(brightness) => setP({ brightness })} />
            </SettingRow>
            <SettingRow label={t("camera.contrast")}>
              <LevelSlider key={`c${p.contrast}`} value={p.contrast} onCommit={(contrast) => setP({ contrast })} />
            </SettingRow>
            <SettingRow label={t("camera.placeholder")} description={t("camera.placeholderBody")}>
              <Segmented
                value={p.placeholder}
                onChange={(placeholder) => setP({ placeholder })}
                options={PLACEHOLDERS.map((v) => ({ value: v, label: t(`camera.placeholderMode.${v}`) }))}
              />
            </SettingRow>
            <SettingRow label={t("camera.grid")} description={t("camera.gridBody")}>
              <Switch checked={draft.grid} onCheckedChange={(grid) => setDraft((d) => ({ ...d!, grid }))} />
            </SettingRow>
          </div>
          <div className="mt-4 flex justify-end">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setP({ mirror: false, rotation: 0, brightness: 0, contrast: 0, framing: "fit" })}
            >
              {t("camera.resetImage")}
            </Button>
          </div>
        </Card>
      </div>
      <SaveBar visible={dirty} canSave saving={saving} onSave={save} onDiscard={() => setDraft(saved)} />
    </Page>
  );
}
