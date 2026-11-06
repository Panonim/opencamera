import { Button } from "@/components/ui/button";
import { t } from "@opencamera/i18n/desktop";
import { rich } from "@opencamera/i18n/react";
import { Loader2, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/api";
import { useApp } from "../store";
import { Badge, Card } from "./common";

const TONE = { starting: "neutral", active: "success", needsSetup: "warning", unsupported: "danger", error: "danger" } as const;

export function useVirtualCamera() {
  const vc = useApp((s) => s.state?.virtualCamera);
  const platform = useApp((s) => s.platform);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return {
    vc,
    platform,
    busy,
    error,
    canInstall: platform === "windows" || platform === "linux",
    installLabel: t(platform === "windows" ? "virtualCamera.install" : "virtualCamera.loadModule"),
    install: () => void run(api.setupVirtualCamera),
    retry: () => void run(api.retryVirtualCamera),
    remove: () => void run(api.removeVirtualCamera),
  };
}

// Warns on the home screen until the virtual camera is set up.
export function VirtualCameraBanner() {
  const { vc, busy, error, canInstall, installLabel, install, retry } = useVirtualCamera();
  if (!vc || (vc.status !== "needsSetup" && vc.status !== "error")) return null;

  return (
    <div role="alert" className="mb-5 flex items-start gap-3 rounded-lg border border-warning/40 bg-warning/10 p-4">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" />
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium">{t(vc.status === "error" ? "virtualCamera.bannerError" : "virtualCamera.bannerSetup")}</div>
        <p className="mt-0.5 text-sm text-muted-foreground">{vc.message ?? t("virtualCamera.bannerBody")}</p>
        {error && <p className="mt-2 text-xs break-words whitespace-pre-wrap text-danger select-text">{error}</p>}
      </div>
      <div className="flex shrink-0 gap-2">
        <Button size="sm" variant="outline" disabled={busy} onClick={retry}>
          {t("common.checkAgain")}
        </Button>
        {vc.status === "needsSetup" && canInstall && (
          <Button size="sm" disabled={busy} onClick={install}>
            {busy && <Loader2 className="animate-spin" />}
            {installLabel}
          </Button>
        )}
      </div>
    </div>
  );
}

export function VirtualCameraCard() {
  const { vc, platform, busy, error, canInstall, installLabel, install, retry, remove } = useVirtualCamera();
  if (!vc) return null;

  return (
    <Card title={t("virtualCamera.title")} badge={<Badge tone={TONE[vc.status]}>{t(`virtualCamera.status.${vc.status}`)}</Badge>}>
      {vc.status === "active" ? (
        <p className="text-sm text-muted-foreground">{rich(t("virtualCamera.activeHint"), { b: (s) => <span className="font-medium text-foreground">{s}</span> })}</p>
      ) : (
        <p className="text-sm text-muted-foreground">{vc.message ?? t("virtualCamera.preparing")}</p>
      )}
      {error && <p className="mt-2 text-xs break-words whitespace-pre-wrap text-danger select-text">{error}</p>}
      {vc.status === "needsSetup" && (
        <div className="mt-3 flex flex-wrap gap-2">
          {canInstall && (
            <Button size="sm" disabled={busy} onClick={install}>
              {busy && <Loader2 className="animate-spin" />}
              {installLabel}
            </Button>
          )}
          <Button size="sm" variant="outline" disabled={busy} onClick={retry}>
            {t("common.checkAgain")}
          </Button>
        </div>
      )}
      {vc.status === "error" && (
        <Button className="mt-3" size="sm" variant="outline" onClick={retry}>
          {t("common.retry")}
        </Button>
      )}
      {vc.status === "active" && platform === "windows" && (
        <div className="mt-3 flex flex-wrap gap-2">
          <Button size="sm" variant="outline" disabled={busy} onClick={install}>
            {busy && <Loader2 className="animate-spin" />}
            {t("virtualCamera.reinstall")}
          </Button>
          <Button size="sm" variant="outline" disabled={busy} onClick={remove}>
            {t("virtualCamera.remove")}
          </Button>
        </div>
      )}
    </Card>
  );
}
