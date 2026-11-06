import { t } from "@opencamera/i18n/desktop";
import { throttledFps, type PhoneState } from "@opencamera/protocol";
import { Smartphone } from "lucide-react";
import { useApp } from "../store";
import { Card, Metric, phaseLabel } from "./common";

// Android labels like "camera2 0, facing back" mean nothing to people, so those fall back to front/rear.
function cameraName(phone: PhoneState) {
  const label = phone.cameras.find((c) => c.deviceId === phone.settings.deviceId)?.label;
  if (label && !/camera\d|facing/i.test(label)) return label;
  return phone.settings.facingMode === "user" ? t("device.frontCamera") : t("device.rearCamera");
}

function statusText(phone: PhoneState | null | undefined) {
  if (!phone) return t("device.waiting");
  if (phone.suspended) return t("device.suspended");
  if (!phone.settings.videoEnabled) return t("device.videoPaused");
  return cameraName(phone);
}

// Shows which phone is connected, which camera it uses, and its battery and heat.
export function DeviceCard() {
  const state = useApp((s) => s.state);
  if (!state) return null;
  const { device, phone, phase } = state;
  const connected = phase === "connected";
  const power = connected ? phone?.power : undefined;
  const saverFps = connected && phone ? throttledFps(phone.settings.fps, phone.power, phone.settings.batterySaver) : null;

  return (
    <Card title={t("device.title")}>
      {device ? (
        <>
          <div className="flex items-center gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-md border bg-elevated">
              <Smartphone className="size-4" />
            </div>
            <div className="min-w-0">
              <div className="truncate text-sm font-medium">{device.name}</div>
              <div className="truncate text-xs text-muted-foreground">{connected ? statusText(phone) : phaseLabel(phase)}</div>
            </div>
          </div>
          {(power?.battery != null || power?.thermal) && (
            <div className="mt-3 border-t pt-2">
              {power.battery != null && (
                <Metric label={t("device.battery")} value={t(power.charging ? "device.charging" : "common.percent", { value: Math.round(power.battery * 100) })} />
              )}
              {power.thermal && <Metric label={t("device.heat")} value={t(`device.heatLevel.${power.thermal}`)} />}
            </div>
          )}
          {saverFps && <p className="mt-2 text-xs leading-relaxed text-warning">{t("device.saverActive", { fps: saverFps })}</p>}
        </>
      ) : (
        <p className="text-sm text-muted-foreground">{t("device.none")}</p>
      )}
    </Card>
  );
}
