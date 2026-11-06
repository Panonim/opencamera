import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { Phase } from "@/lib/api";
import { t } from "@opencamera/i18n/desktop";
import { Loader2 } from "lucide-react";
import type { ReactNode } from "react";

export function PageHeader({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) {
  return (
    <header className="flex items-start justify-between gap-6 pb-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}

export function Page({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("mx-auto w-full max-w-6xl px-8 py-7", className)}>{children}</div>;
}

export function Card({
  title,
  badge,
  children,
  className,
  collapsed = false,
}: {
  title?: string;
  badge?: ReactNode;
  children: ReactNode;
  className?: string;
  collapsed?: boolean;
}) {
  return (
    <section className={cn("rounded-lg border bg-card", className)}>
      {title && (
        <div className={cn("flex items-center justify-between gap-3 border-b px-4 py-3 transition-[border-color] duration-200", collapsed && "border-transparent")}>
          <h2 className="text-base font-semibold">{title}</h2>
          {badge}
        </div>
      )}
      {/* Collapses the card body by animating its grid height. */}
      <div className={cn("grid transition-[grid-template-rows] duration-200 ease-out", collapsed ? "grid-rows-[0fr]" : "grid-rows-[1fr]")} inert={collapsed}>
        <div className="overflow-hidden">
          <div className="p-4">{children}</div>
        </div>
      </div>
    </section>
  );
}

export function SettingRow({ label, description, children, disabled }: { label: string; description?: ReactNode; children: ReactNode; disabled?: boolean }) {
  return (
    <div className={cn("flex items-center justify-between gap-6 py-3 first:pt-0 last:pb-0", disabled && "opacity-50")}>
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {description && <div className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{description}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: "neutral" | "success" | "warning" | "danger" }) {
  const tones = {
    neutral: "border-border bg-elevated text-muted-foreground",
    success: "border-success/30 bg-success/10 text-success",
    warning: "border-warning/30 bg-warning/10 text-warning",
    danger: "border-danger/30 bg-danger/10 text-danger",
  };
  return <span className={cn("inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-medium", tones[tone])}>{children}</span>;
}

export const phaseLabel = (phase: Phase) => t(`phase.${phase}`);

export function phaseTone(phase: Phase): "neutral" | "success" | "warning" | "danger" {
  if (phase === "connected") return "success";
  if (phase === "connecting" || phase === "reconnecting" || phase === "pairing") return "warning";
  if (phase === "failed") return "danger";
  return "neutral";
}

export function Metric({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1.5 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="truncate font-medium tabular-nums">{value}</span>
    </div>
  );
}

export function StatusDot({ tone, pulse }: { tone: "neutral" | "success" | "warning" | "danger"; pulse?: boolean }) {
  const colors = { neutral: "bg-muted-foreground/50", success: "bg-success", warning: "bg-warning", danger: "bg-danger" };
  return (
    <span className="relative flex size-2">
      {pulse && <span className={cn("absolute inline-flex size-full animate-ping rounded-full opacity-60 motion-reduce:hidden", colors[tone])} />}
      <span className={cn("relative inline-flex size-2 rounded-full", colors[tone])} />
    </span>
  );
}

// Shows a floating save bar while there are unsaved changes.
export function SaveBar({ visible, canSave, saving, onSave, onDiscard }: { visible: boolean; canSave: boolean; saving: boolean; onSave: () => void; onDiscard: () => void }) {
  return (
    <div
      aria-hidden={!visible}
      className={cn(
        "pointer-events-none sticky bottom-5 z-10 mt-6 flex justify-center transition-all duration-200",
        visible ? "translate-y-0 opacity-100" : "translate-y-4 opacity-0",
      )}
    >
      <div
        role="status"
        className={cn("flex items-center gap-4 rounded-lg border bg-card py-2.5 pr-2.5 pl-4 shadow-lg", visible && "pointer-events-auto")}
      >
        <span className="text-sm font-medium">{t("common.unsavedChanges")}</span>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" disabled={saving} tabIndex={visible ? 0 : -1} onClick={onDiscard}>
            {t("common.discard")}
          </Button>
          <Button size="sm" disabled={!canSave || saving} tabIndex={visible ? 0 : -1} onClick={onSave}>
            {saving && <Loader2 className="animate-spin" />}
            {t("common.saveChanges")}
          </Button>
        </div>
      </div>
    </div>
  );
}
