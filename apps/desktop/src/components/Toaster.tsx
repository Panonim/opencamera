import { cn } from "@/lib/utils";
import { dismissToast, useToasts, type ToastTone } from "@/lib/toast";
import { t } from "@opencamera/i18n/desktop";
import { CircleAlert, CircleCheck, Info, X, type LucideIcon } from "lucide-react";

const TONES: Record<ToastTone, { icon: LucideIcon; gradient: string; text: string }> = {
  success: { icon: CircleCheck, gradient: "from-success/20", text: "text-success" },
  info: { icon: Info, gradient: "from-primary/15", text: "text-primary" },
  error: { icon: CircleAlert, gradient: "from-danger/20", text: "text-danger" },
};

export function Toaster() {
  const toasts = useToasts((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed right-3 bottom-3 z-[60] flex w-[clamp(15rem,32vw,24rem)] max-w-[calc(100%-1.5rem)] flex-col gap-2 sm:right-5 sm:bottom-5 sm:gap-3">
      {toasts.map(({ id, tone, title, description, leaving }) => {
        const { icon: Icon, gradient, text } = TONES[tone];
        return (
          <div
            key={id}
            role={tone === "error" ? "alert" : "status"}
            className={cn(
              "pointer-events-auto relative flex items-start gap-2 overflow-hidden rounded-xl bg-card p-3 shadow-lg motion-reduce:animate-none sm:gap-3 sm:p-4",
              leaving ? "animate-out fade-out slide-out-to-right-full fill-mode-forwards duration-200 ease-in" : "animate-in fade-in slide-in-from-right-full duration-200 ease-out",
            )}
          >
            <span className={cn("absolute inset-0 bg-gradient-to-r to-transparent to-70%", gradient)} aria-hidden />
            <Icon className={cn("relative mt-0.5 size-4 shrink-0 sm:size-5", text)} />
            <div className="relative min-w-0 flex-1">
              <div className="text-xs font-medium sm:text-sm">{title}</div>
              {description && <p className="mt-0.5 text-xs leading-relaxed break-words text-muted-foreground">{description}</p>}
            </div>
            <button className="relative shrink-0 text-muted-foreground transition-colors hover:text-foreground" aria-label={t("common.dismiss")} onClick={() => dismissToast(id)}>
              <X className="size-4" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
