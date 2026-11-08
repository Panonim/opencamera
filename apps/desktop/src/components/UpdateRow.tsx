import { Button } from "@/components/ui/button";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import { relaunch } from "@tauri-apps/plugin-process";
import { type Update } from "@tauri-apps/plugin-updater";
import { Check, Download, Loader2, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { checkForUpdate, useUpdate } from "../lib/updates";
import { ActionRow, Swap } from "./common";

type Status = { kind: "idle" } | { kind: "checking" } | { kind: "latest" } | { kind: "available"; update: Update } | { kind: "installing"; percent: number | null };

const LATEST_VISIBLE_MS = 4000;

export function UpdateRow() {
  const pending = useUpdate((s) => s.update);
  const [local, setStatus] = useState<Status>({ kind: "idle" });
  // A background check that found an update takes over the idle state.
  const status: Status = local.kind === "idle" && pending ? { kind: "available", update: pending } : local;

  useEffect(() => {
    if (local.kind !== "latest") return;
    const timer = setTimeout(() => setStatus({ kind: "idle" }), LATEST_VISIBLE_MS);
    return () => clearTimeout(timer);
  }, [local.kind]);

  const fail = (e: unknown) => {
    toast("error", t("update.errorTitle"), String(e));
    setStatus({ kind: "idle" });
  };

  const find = () => {
    setStatus({ kind: "checking" });
    checkForUpdate()
      .then((update) => setStatus(update ? { kind: "available", update } : { kind: "latest" }))
      .catch(fail);
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
      fail(e);
    }
  };

  const busy = status.kind === "checking" || status.kind === "installing";
  const label =
    status.kind === "available" ? (
      <>
        <Download /> {t("update.install", { version: status.update.version })}
      </>
    ) : status.kind === "checking" ? (
      <>
        <Loader2 className="animate-spin" /> {t("update.checking")}
      </>
    ) : status.kind === "installing" ? (
      <>
        <Loader2 className="animate-spin" /> {status.percent != null ? t("update.downloadingPercent", { percent: status.percent }) : t("update.downloading")}
      </>
    ) : status.kind === "latest" ? (
      <>
        <Check /> {t("update.latest")}
      </>
    ) : (
      <>
        <RefreshCw /> {t("update.check")}
      </>
    );

  return (
    <>
      <ActionRow label={t("update.label")}>
        <Button
          size="sm"
          variant={status.kind === "available" ? "default" : "outline"}
          disabled={busy}
          className={cn("min-w-52 transition-colors duration-300", status.kind === "latest" && "text-success")}
          onClick={() => (status.kind === "available" ? void install(status.update) : find())}
        >
          <Swap id={status.kind}>{label}</Swap>
        </Button>
      </ActionRow>
      {status.kind === "available" && status.update.body && (
        <pre className="max-h-40 overflow-auto rounded-md bg-elevated p-3 text-xs whitespace-pre-wrap">{status.update.body}</pre>
      )}
    </>
  );
}
