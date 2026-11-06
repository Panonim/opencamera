import { Button } from "@/components/ui/button";
import { t } from "@opencamera/i18n/desktop";
import { relaunch } from "@tauri-apps/plugin-process";
import { type Update } from "@tauri-apps/plugin-updater";
import { Download, Loader2, RefreshCw } from "lucide-react";
import { useState } from "react";
import { checkForUpdate, useUpdate } from "../lib/updates";
import { Card, SettingRow } from "./common";

type Status =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: Update }
  | { kind: "installing"; percent: number | null }
  | { kind: "error"; message: string };

export function UpdateCard() {
  const pending = useUpdate((s) => s.update);
  const [local, setStatus] = useState<Status>({ kind: "idle" });
  // A background check that found an update takes over the idle state.
  const status: Status = local.kind === "idle" && pending ? { kind: "available", update: pending } : local;

  const find = () => {
    setStatus({ kind: "checking" });
    checkForUpdate()
      .then((update) => setStatus(update ? { kind: "available", update } : { kind: "latest" }))
      .catch((e) => setStatus({ kind: "error", message: String(e) }));
  };

  const install = async (update: Update) => {
    let total = 0;
    let done = 0;
    setStatus({ kind: "installing", percent: null });
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") done += e.data.chunkLength;
        setStatus({ kind: "installing", percent: total ? Math.round((done / total) * 100) : null });
      });
      await relaunch();
    } catch (e) {
      setStatus({ kind: "error", message: String(e) });
    }
  };

  const description =
    status.kind === "available"
      ? t("update.available", { version: status.update.version })
      : status.kind === "installing"
        ? status.percent != null
          ? t("update.downloadingPercent", { percent: status.percent })
          : t("update.downloading")
        : status.kind === "error"
          ? t("update.error", { message: status.message })
          : t(`update.${status.kind}`);

  return (
    <Card title={t("update.title")}>
      <SettingRow label={t("update.label")} description={description}>
        {status.kind === "available" ? (
          <Button size="sm" onClick={() => void install(status.update)}>
            <Download /> {t("update.install")}
          </Button>
        ) : (
          <Button size="sm" variant="outline" disabled={status.kind === "checking" || status.kind === "installing"} onClick={find}>
            {status.kind === "checking" || status.kind === "installing" ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t("update.check")}
          </Button>
        )}
      </SettingRow>
      {status.kind === "available" && status.update.body && (
        <pre className="mt-3 max-h-40 overflow-auto rounded-md bg-elevated p-3 text-xs whitespace-pre-wrap">{status.update.body}</pre>
      )}
    </Card>
  );
}
