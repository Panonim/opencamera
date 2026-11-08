import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { t } from "@opencamera/i18n/desktop";
import { Logo } from "@opencamera/ui/Logo";
import { Check, Copy, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { ActionRow, Card, Crossfade, Page, PageHeader } from "../components/common";
import { LogView } from "../components/LogView";
import { UpdateRow } from "../components/UpdateRow";
import { api, onEngineLog, type Diagnostics } from "../lib/api";
import { toast } from "../lib/toast";
import { useApp } from "../store";

const MAX_LIVE_LINES = 3000;

// Stays in English on purpose so shared reports read the same for maintainers.
function diagnosticsText(d: Diagnostics) {
  return [
    `OpenCamera ${d.version} (${d.os}/${d.arch})`,
    `Virtual camera: ${d.virtualCamera.status}${d.virtualCamera.message ? ` - ${d.virtualCamera.message}` : ""}`,
    d.virtualCameraDetails,
    `Server: ${d.server.running ? `listening on ${d.server.address}:${d.server.port}` : "not running"}`,
    `Local addresses: ${d.addresses.join(", ") || "none"}`,
    `Paired devices: ${d.pairedDevices}`,
    "",
    ...d.logs,
  ].join("\n");
}

export function About() {
  const version = useApp((s) => s.version);
  const resetAll = useApp((s) => s.resetAll);
  const [confirmReset, setConfirmReset] = useState(false);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [copied, setCopied] = useState(false);
  const developerMode = useApp((s) => s.settings?.developerMode ?? false);
  const load = () => void api.diagnostics().then(setDiag);
  useEffect(load, [developerMode]);
  useEffect(() => {
    if (!developerMode) return;
    const unlisten = onEngineLog((line) => setDiag((d) => d && { ...d, logs: [...d.logs.slice(-(MAX_LIVE_LINES - 1)), line] }));
    return () => void unlisten.then((u) => u());
  }, [developerMode]);

  return (
    <Page className="max-w-3xl">
      <PageHeader title={t("nav.about")} />
      <div className="flex flex-col gap-5">
        <Card>
          <div className="flex items-start gap-4">
            <div className="flex size-12 shrink-0 items-center justify-center rounded-lg bg-elevated">
              <Logo className="size-7" />
            </div>
            <div>
              <div className="font-semibold">OpenCamera</div>
              <div className="text-xs text-muted-foreground">{t("common.version", { version })}</div>
              <p className="mt-3 text-sm leading-relaxed text-muted-foreground">{t("about.description")}</p>
            </div>
          </div>
          <div className="mt-5 flex flex-col gap-3">
            <UpdateRow />
            <ActionRow label={t("about.resetLabel")}>
              <Button
                size="sm"
                variant={confirmReset ? "destructive" : "outline"}
                className="min-w-52 duration-200"
                onBlur={() => setConfirmReset(false)}
                onClick={() => {
                  if (!confirmReset) return setConfirmReset(true);
                  setConfirmReset(false);
                  resetAll().catch((e) => toast("error", t("about.resetError"), String(e)));
                }}
              >
                <Crossfade active={+confirmReset} labels={[t("about.reset"), t("about.resetConfirm")]} />
              </Button>
            </ActionRow>
          </div>
        </Card>

        <Card
          title={t("about.diagnostics")}
          badge={
            <div className="flex gap-1">
              <Button size="xs" variant="ghost" onClick={load}>
                <RefreshCw /> {t("about.refresh")}
              </Button>
              <Button
                size="xs"
                variant="ghost"
                disabled={!diag}
                onClick={() => {
                  if (!diag) return;
                  void navigator.clipboard.writeText(diagnosticsText(diag));
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                }}
              >
                {copied ? <Check /> : <Copy />} {t(copied ? "about.copied" : "about.copy")}
              </Button>
            </div>
          }
        >
          <p className="mb-3 text-xs text-muted-foreground">
            {t("about.diagnosticsBody")}
            {developerMode && ` ${t("about.diagnosticsLive")}`}
          </p>
          {diag ? (
            <LogView text={diagnosticsText(diag)} className={developerMode ? "max-h-[60vh]" : undefined} />
          ) : (
            <Skeleton className="h-40 w-full" />
          )}
        </Card>
      </div>
    </Page>
  );
}
