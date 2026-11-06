import { Button } from "@/components/ui/button";
import { t } from "@opencamera/i18n/desktop";
import { TriangleAlert, X } from "lucide-react";
import { api } from "../lib/api";
import { errorInfo } from "../lib/errors";
import { useApp } from "../store";

export function ErrorBanner() {
  const { state, setPage, setPairOpen } = useApp();
  if (!state?.error) return null;
  const info = errorInfo(state.error.code);

  const act = () => {
    void api.clearError();
    if (info.action === "pair") setPairOpen(true);
    if (info.action === "settings") setPage("settings");
  };

  return (
    <div role="alert" className="mb-5 flex items-start gap-3 rounded-lg border border-danger/30 bg-danger/10 p-4">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-danger" />
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium">{info.title}</div>
        <p className="mt-0.5 text-sm text-muted-foreground">{info.body}</p>
      </div>
      {info.action !== "dismiss" && (
        <Button size="sm" variant="outline" onClick={act}>
          {info.actionLabel}
        </Button>
      )}
      <Button size="icon-sm" variant="ghost" aria-label={t("common.dismiss")} onClick={() => void api.clearError()}>
        <X />
      </Button>
    </div>
  );
}
