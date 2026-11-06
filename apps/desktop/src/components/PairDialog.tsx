import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { cn, formatCountdown } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import { Copy, Loader2, ShieldAlert, Wifi } from "lucide-react";
import QRCode from "qrcode";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { errorInfo } from "../lib/errors";
import { useApp } from "../store";
import { CertificateHelp } from "./CertificateHelp";

const HINT_AFTER_MS = 45_000;

// Starts pairing while active, draws the QR code and swaps in a fresh code when the old one expires.
export function usePairing(active: boolean) {
  const pairing = useApp((s) => s.state?.pairing);
  const [qr, setQr] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    if (!active) return;
    // Clears the old code on open, so the dialog keeps its content while closing.
    setQr(null);
    setError(null);
    api.startPairing().catch(() => setError(useApp.getState().state?.error?.code ?? "server_failed"));
  }, [active]);

  useEffect(() => {
    if (!pairing?.url) return;
    // Draws the code dark on white so every phone can scan it.
    void QRCode.toString(pairing.url, { type: "svg", margin: 0, errorCorrectionLevel: "M", color: { dark: "#18181b", light: "#ffffff" } }).then(setQr);
  }, [pairing?.url]);

  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [active]);

  useEffect(() => {
    if (active && pairing && now >= pairing.expiresAt) void api.startPairing().catch(() => {});
  }, [active, pairing, now]);

  const remaining = pairing ? Math.max(0, Math.round((pairing.expiresAt - now) / 1000)) : 0;
  return { pairing, qr, error: error ? errorInfo(error) : null, remaining, now };
}

export function PairQr({ qr, remaining, ready, className }: { qr: string | null; remaining: number; ready: boolean; className?: string }) {
  return (
    <div className="flex flex-col items-center gap-2">
      <div className={cn("flex size-[220px] items-center justify-center rounded-lg border bg-white p-4", className)}>
        {qr ? <img alt={t("pair.qrAlt")} className="size-full" src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(qr)}`} /> : <Skeleton className="size-full" />}
      </div>
      <span className="text-xs tabular-nums text-muted-foreground">{ready ? t("pair.newCodeIn", { time: formatCountdown(remaining) }) : t("pair.creatingCode")}</span>
    </div>
  );
}

export function PairError({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-md border border-danger/30 bg-danger/10 p-4 text-sm">
      <div className="font-medium">{title}</div>
      <p className="mt-1 text-muted-foreground">{body}</p>
    </div>
  );
}

export function PairDialog() {
  const { pairOpen, setPairOpen, state } = useApp();
  const { pairing, qr, error: err, remaining, now } = usePairing(pairOpen);
  const [openedAt, setOpenedAt] = useState(Date.now());

  useEffect(() => {
    if (pairOpen) setOpenedAt(Date.now());
  }, [pairOpen]);

  const close = (open: boolean) => {
    if (!open) {
      setPairOpen(false);
      // Cancels after the close animation so the code doesn't vanish mid-fade.
      setTimeout(() => {
        const { pairOpen, state } = useApp.getState();
        if (!pairOpen && state?.phase === "pairing") void api.cancelPairing();
      }, 200);
    }
  };

  return (
    <Dialog open={pairOpen} onOpenChange={close}>
      <DialogContent className="max-h-[calc(100%-2rem)] overflow-y-auto sm:max-w-[640px]">
        <DialogHeader>
          <DialogTitle>{t("pair.title")}</DialogTitle>
          <DialogDescription>{t("pair.description")}</DialogDescription>
        </DialogHeader>
        {err ? (
          <PairError {...err} />
        ) : (
          <div className="grid gap-6 sm:grid-cols-[220px_1fr]">
            <PairQr qr={qr} remaining={remaining} ready={!!pairing} />
            <ol className="flex flex-col gap-4 text-sm">
              <li>
                <div className="font-medium">{t("pair.scanTitle")}</div>
                <p className="mt-0.5 text-muted-foreground">{t("pair.scanBody")}</p>
              </li>
              <li>
                <div className="flex items-center gap-1.5 font-medium">
                  {t("pair.warningTitle")} <ShieldAlert className="size-3.5 text-warning" />
                </div>
                <CertificateHelp className="mt-0.5" host={pairing ? new URL(pairing.url).hostname : undefined} />
              </li>
              <li>
                <div className="font-medium">{t("pair.startTitle")}</div>
                <p className="mt-0.5 text-muted-foreground">{t("pair.startBody")}</p>
              </li>
              {pairing && (
                <button
                  className="flex items-center gap-2 truncate rounded-md border bg-elevated px-3 py-2 text-left font-mono text-xs text-muted-foreground transition-colors hover:text-foreground"
                  onClick={() => void navigator.clipboard.writeText(pairing.url)}
                  title={t("pair.copyLink")}
                >
                  <Copy className="size-3.5 shrink-0" />
                  <span className="truncate">{pairing.url.split("#")[0]}</span>
                </button>
              )}
            </ol>
          </div>
        )}
        {!err && now - openedAt > HINT_AFTER_MS && (
          <div className="flex gap-3 rounded-md border bg-elevated p-3 text-xs leading-relaxed text-muted-foreground">
            <Wifi className="mt-0.5 size-4 shrink-0" />
            <span>{t("pair.hint", { address: state?.server.address ?? t("pair.unknownAddress") })}</span>
          </div>
        )}
        {state?.phase === "connecting" && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" /> {t("pair.connecting")}
          </div>
        )}
        <div className="flex justify-end">
          <Button variant="outline" onClick={() => close(false)}>
            {t("common.cancel")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
