import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { t } from "@opencamera/i18n/desktop";
import { FlipHorizontal2, Grid3x3, RefreshCcw, RotateCw, SlidersHorizontal } from "lucide-react";
import type { ReactNode } from "react";
import { Page, PageHeader } from "../components/common";
import { DeviceCard } from "../components/DeviceCard";
import { ErrorBanner } from "../components/ErrorBanner";
import { PerformancePanel } from "../components/PerformancePanel";
import { Preview } from "../components/Preview";
import { VirtualCameraBanner } from "../components/VirtualCameraCard";
import { api } from "../lib/api";
import { useApp } from "../store";

function ToolButton({ label, onClick, active, disabled, children }: { label: string; onClick: () => void; active?: boolean; disabled?: boolean; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant={active ? "default" : "outline"} size="icon" aria-label={label} aria-pressed={active} disabled={disabled} onClick={onClick}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function Dashboard() {
  const { state, settings, saveSettings, setPairOpen, setPage } = useApp();
  if (!state || !settings) return null;
  const connected = state.phase === "connected";
  const busy = state.phase === "connecting" || state.phase === "reconnecting";
  const phone = state.phone;
  const p = settings.processing;

  return (
    <Page>
      <PageHeader
        title={t("nav.dashboard")}
        description={t("dashboard.description")}
        actions={
          connected || busy ? (
            <Button variant="outline" onClick={() => void api.disconnect()}>
              {t("dashboard.disconnect")}
            </Button>
          ) : (
            <Button onClick={() => setPairOpen(true)}>{t("dashboard.connect")}</Button>
          )
        }
      />
      <ErrorBanner />
      <VirtualCameraBanner />
      <div className="@container">
        <div className="grid grid-cols-1 gap-5 @3xl:grid-cols-[minmax(0,1fr)_300px]">
          <div className="flex flex-col gap-3">
            <Preview />
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-1">
                <ToolButton
                  label={t("dashboard.switchCamera")}
                  disabled={!connected || !phone?.capabilities.canSwitchCamera}
                  onClick={() => void api.setPhoneSettings({ facingMode: phone?.settings.facingMode === "user" ? "environment" : "user" })}
                >
                  <RefreshCcw />
                </ToolButton>
                <ToolButton label={t("dashboard.mirror")} active={p.mirror} onClick={() => void saveSettings({ processing: { ...p, mirror: !p.mirror } })}>
                  <FlipHorizontal2 />
                </ToolButton>
                <ToolButton
                  label={t("dashboard.rotate", { degrees: p.rotation })}
                  active={p.rotation !== 0}
                  onClick={() => void saveSettings({ processing: { ...p, rotation: ((p.rotation + 90) % 360) as typeof p.rotation } })}
                >
                  <RotateCw />
                </ToolButton>
                <ToolButton label={t("dashboard.grid")} active={settings.grid} onClick={() => void saveSettings({ grid: !settings.grid })}>
                  <Grid3x3 />
                </ToolButton>
              </div>
              <Button variant="outline" size="sm" onClick={() => setPage("camera")}>
                <SlidersHorizontal /> {t("common.cameraSettings")}
              </Button>
            </div>
          </div>
          <div className="flex flex-col gap-5">
            <DeviceCard />
            <PerformancePanel />
          </div>
        </div>
      </div>
    </Page>
  );
}
