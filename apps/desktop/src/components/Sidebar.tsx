import { cn } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import { Camera, Info, LayoutDashboard, Settings } from "lucide-react";
import { Logo } from "@opencamera/ui/Logo";
import { useUpdate } from "../lib/updates";
import { useApp, type Page } from "../store";
import { phaseLabel, phaseTone, StatusDot } from "./common";
import { IS_WINDOWS } from "./TitleBar";

const NAV: { page: Page; icon: typeof Camera }[] = [
  { page: "dashboard", icon: LayoutDashboard },
  { page: "camera", icon: Camera },
  { page: "settings", icon: Settings },
  { page: "about", icon: Info },
];

export function Sidebar() {
  const { page, setPage, state, version } = useApp();
  const update = useUpdate((s) => s.update);
  const phase = state?.phase ?? "idle";
  const tone = phaseTone(phase);

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r bg-sidebar">
      <div className={cn("flex items-center gap-2.5 px-5 pb-6", IS_WINDOWS ? "pt-1" : "pt-5")}>
        <Logo className="size-6" />
        <span className="text-sm font-semibold tracking-tight">OpenCamera</span>
      </div>
      <nav className="flex flex-col gap-0.5 px-3">
        {NAV.map(({ page: p, icon: Icon }) => {
          const active = page === p;
          return (
            <button
              key={p}
              onClick={() => setPage(p)}
              aria-current={active ? "page" : undefined}
              className={cn(
                "flex h-9 items-center gap-3 rounded-md px-3 text-sm transition-colors duration-150 outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                active ? "bg-primary/15 font-medium text-foreground" : "text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
              )}
            >
              <Icon className="size-4" />
              {t(`nav.${p}`)}
            </button>
          );
        })}
      </nav>
      <div className="mt-auto border-t px-5 py-4">
        <div className="flex items-center gap-2 text-xs">
          <StatusDot tone={tone} pulse={tone === "warning"} />
          <span className="truncate leading-none font-medium">{phaseLabel(phase)}</span>
        </div>
        {state?.device && phase === "connected" && <div className="mt-1 truncate text-xs text-muted-foreground">{state.device.name}</div>}
        <div className="mt-2 truncate text-[11px] text-muted-foreground">
          {t("common.version", { version })}
          {update && (
            <button onClick={() => setPage("about")} className="hover:text-foreground">
              &nbsp;· {t("common.updateAvailable")}
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
