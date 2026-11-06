import { Button } from "@/components/ui/button";
import { t } from "@opencamera/i18n/desktop";
import { streamProblem, sustainedWarning, WARNING_START, type StreamWarning as Warning } from "@opencamera/protocol";
import { Cpu, Thermometer, WifiLow } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useApp } from "../store";

const ICON: Record<Warning, typeof Cpu> = { hot: Thermometer, cpu: Cpu, bandwidth: WifiLow };

// Debounces phone heat and quality limits so the warning only changes once a problem has lasted a while.
function useStreamWarning(live: boolean) {
  const stats = useApp((s) => s.stats);
  const thermal = useApp((s) => s.state?.phone?.power.thermal);
  const [state, setState] = useState(WARNING_START);
  useEffect(() => {
    setState((s) => (live ? sustainedWarning(s, streamProblem(thermal, stats?.phone?.qualityLimitation), Date.now()) : WARNING_START));
  }, [live, stats, thermal]);
  return state.shown;
}

// Warns when the phone is too hot or can't keep up, with what to do about it.
export function StreamWarning({ live }: { live: boolean }) {
  const warning = useStreamWarning(live);
  const setPage = useApp((s) => s.setPage);
  const saverOff = useApp((s) => s.state?.phone?.settings.batterySaver === false);
  if (!warning) return null;
  const Icon = ICON[warning];

  return (
    <div role="status" className="mt-3 flex items-start gap-3 rounded-lg border border-warning/40 bg-warning/10 p-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-warning" />
      <div className="flex min-w-0 flex-col gap-2">
        <div>
          <div className="text-sm font-medium">{t(`streamWarning.${warning}.title`)}</div>
          <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{t(`streamWarning.${warning}.body`, { tip: t("streamWarning.tip") })}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="xs" variant="outline" onClick={() => setPage("camera")}>
            {t("common.cameraSettings")}
          </Button>
          {warning !== "bandwidth" && saverOff && (
            <Button size="xs" variant="outline" onClick={() => void api.setPhoneSettings({ batterySaver: true }).catch(() => {})}>
              {t("streamWarning.batterySaver")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
