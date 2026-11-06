import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Skeleton } from "@/components/ui/skeleton";
import { cn, formatCountdown } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import { rich } from "@opencamera/i18n/react";
import { Logo } from "@opencamera/ui/Logo";
import { AlertCircle, ArrowRight, CheckCircle2, ChevronLeft, Loader2, Lock, Wifi } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { PairError, usePairing } from "../components/PairDialog";
import { useVirtualCamera } from "../components/VirtualCameraCard";
import { api } from "../lib/api";
import { useApp } from "../store";

const STEPS = ["camera", "phone"] as const;
const FADE_MS = 200;
const rise = "animate-in fade-in slide-in-from-bottom-4 fill-mode-both duration-700 ease-out motion-reduce:animate-none";

const TONES = {
  neutral: { icon: Loader2, className: "animate-spin text-muted-foreground" },
  success: { icon: CheckCircle2, className: "text-success" },
  warning: { icon: AlertCircle, className: "text-warning" },
};

function StatusLine({ tone, title, body }: { tone: keyof typeof TONES; title: string; body?: ReactNode }) {
  const { icon: Icon, className } = TONES[tone];
  return (
    <div className="flex items-start gap-3 rounded-lg border bg-card p-4">
      <Icon className={cn("mt-0.5 size-4 shrink-0", className)} />
      <div className="min-w-0">
        <div className="text-sm font-medium">{title}</div>
        {body && <p className="mt-0.5 text-sm break-words text-muted-foreground">{body}</p>}
      </div>
    </div>
  );
}

function GuideStep({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-primary/10 text-xs font-medium text-primary">{n}</span>
      <div className="min-w-0 text-sm">
        <div className="font-medium">{title}</div>
        <p className="mt-0.5 text-muted-foreground">{children}</p>
      </div>
    </li>
  );
}

function IntroStep({ onStart }: { onStart: () => void }) {
  return (
    <div className="flex flex-col items-center text-center">
      <div className={cn("relative mb-8", rise)}>
        <div className="absolute -inset-4 rounded-full bg-primary/15 blur-2xl" />
        <div className="relative flex size-20 items-center justify-center rounded-3xl border bg-card shadow-lg">
          <Logo className="size-10" />
        </div>
      </div>
      <h1 className={cn("text-3xl font-semibold tracking-tight text-balance", rise, "delay-100")}>{t("welcome.title")}</h1>
      <p className={cn("mt-3 max-w-sm text-balance text-muted-foreground", rise, "delay-200")}>{t("welcome.body")}</p>
      <Button size="lg" className={cn("mt-10 min-w-44", rise, "delay-300")} onClick={onStart}>
        {t("welcome.start")} <ArrowRight />
      </Button>
    </div>
  );
}

function StepHeader({ step, title, body }: { step: number; title: string; body: ReactNode }) {
  return (
    <div>
      <div className="text-xs font-medium text-muted-foreground">{t("welcome.stepOf", { step, total: STEPS.length })}</div>
      <h1 className="mt-1 text-xl font-semibold tracking-tight">{title}</h1>
      <p className="mt-1 text-sm text-muted-foreground">{body}</p>
    </div>
  );
}

function VirtualCameraStep() {
  const { vc, platform, busy, error, canInstall, installLabel, install, retry } = useVirtualCamera();
  const status = vc?.status ?? "starting";

  return (
    <div className="flex flex-col gap-6">
      <StepHeader
        step={1}
        title={t("welcome.cameraTitle")}
        body={rich(t("welcome.cameraBody"), { b: (s) => <span className="font-medium text-foreground">{s}</span> })}
      />
      {status === "starting" && <StatusLine tone="neutral" title={t("welcome.checking")} />}
      {status === "active" && <StatusLine tone="success" title={t("welcome.ready")} body={t("welcome.readyBody")} />}
      {status === "unsupported" && <StatusLine tone="warning" title={t("welcome.unsupported")} body={vc?.message} />}
      {(status === "needsSetup" || status === "error") && (
        <>
          <StatusLine tone="warning" title={t(status === "error" ? "welcome.broken" : "welcome.notInstalled")} body={vc?.message} />
          <div className="flex flex-col gap-2">
            <div className="flex flex-wrap gap-2">
              {status === "needsSetup" && canInstall && (
                <Button disabled={busy} onClick={install}>
                  {busy && <Loader2 className="animate-spin" />}
                  {installLabel}
                </Button>
              )}
              <Button variant="outline" disabled={busy} onClick={retry}>
                {t("common.checkAgain")}
              </Button>
            </div>
            {status === "needsSetup" && canInstall && (
              <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <Lock className="size-3" />
                {t(platform === "windows" ? "welcome.permissionWindows" : "welcome.permissionPassword")}
              </p>
            )}
          </div>
        </>
      )}
      {error && <p className="text-xs break-words whitespace-pre-wrap text-danger select-text">{error}</p>}
    </div>
  );
}

function PhoneStep() {
  const state = useApp((s) => s.state);
  const phase = state?.phase ?? "idle";

  let content = <PairPanel />;
  const name = state?.device?.name;
  if (phase === "connected")
    content = <StatusLine tone="success" title={name ? t("welcome.connectedTo", { name }) : t("welcome.connectedPhone")} body={t("welcome.allSet")} />;
  else if (phase === "connecting" || phase === "reconnecting") content = <StatusLine tone="neutral" title={t("welcome.connecting")} />;

  return (
    <div className="flex flex-col gap-6">
      <StepHeader
        step={2}
        title={t("welcome.phoneTitle")}
        body={
          <span className="flex items-center gap-1.5">
            <Wifi className="size-3.5 shrink-0" /> {t("welcome.phoneBody")}
          </span>
        }
      />
      {content}
    </div>
  );
}

function PairPanel() {
  const { pairing, qr, error, remaining } = usePairing(true);
  const address = useApp((s) => s.state?.server.address);
  const host = pairing ? new URL(pairing.url).hostname : (address ?? t("welcome.thisComputer"));

  // Drops the unused code when leaving the step without connecting.
  useEffect(
    () => () => {
      if (useApp.getState().state?.phase === "pairing") void api.cancelPairing();
    },
    [],
  );

  if (error) return <PairError {...error} />;
  return (
    <div className="@container">
      <div className="flex flex-col items-center gap-6 @md:flex-row @md:items-start">
        <div className="flex shrink-0 flex-col items-center gap-2">
          <div className="size-44 rounded-xl border bg-white p-3">
            {qr ? <img alt={t("pair.qrAlt")} className="size-full" src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(qr)}`} /> : <Skeleton className="size-full" />}
          </div>
          <span className="text-xs tabular-nums text-muted-foreground">{pairing ? t("pair.newCodeIn", { time: formatCountdown(remaining) }) : t("pair.creatingCode")}</span>
        </div>
        <ol className="flex w-full min-w-0 flex-col gap-4">
          <GuideStep n={1} title={t("welcome.scanTitle")}>
            {t("welcome.scanBody")}
          </GuideStep>
          <GuideStep n={2} title={t("welcome.warningTitle")}>
            {t("welcome.warningBody")} <CertificatePopover host={host} />
          </GuideStep>
          <GuideStep n={3} title={t("welcome.allowTitle")}>
            {t("welcome.allowBody")}
          </GuideStep>
        </ol>
      </div>
    </div>
  );
}

function CertificatePopover({ host }: { host: string }) {
  return (
    <Popover>
      <PopoverTrigger className="rounded-sm font-medium text-foreground underline underline-offset-4 outline-none hover:text-primary focus-visible:ring-2 focus-visible:ring-ring/50">
        {t("welcome.showHow")}
      </PopoverTrigger>
      <PopoverContent className="flex w-96 flex-col gap-3">
        <div>
          <div className="font-medium">{t("welcome.expectedTitle")}</div>
          <p className="mt-1 text-muted-foreground">{t("welcome.expectedBody")}</p>
        </div>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 border-t pt-3">
          <dt className="font-medium">{t("welcome.android")}</dt>
          <dd className="break-words text-muted-foreground">{t("welcome.androidSteps", { host })}</dd>
          <dt className="font-medium">{t("welcome.iphone")}</dt>
          <dd className="text-muted-foreground">{t("welcome.iphoneSteps")}</dd>
        </dl>
      </PopoverContent>
    </Popover>
  );
}

function Progress({ step }: { step: number }) {
  return (
    <ol className="flex items-center gap-1.5" aria-label={t("welcome.stepOf", { step, total: STEPS.length })}>
      {STEPS.map((id, i) => (
        <li
          key={id}
          title={t(`welcome.steps.${id}`)}
          className={cn("h-1.5 rounded-full transition-all duration-500 ease-out", i + 1 === step ? "w-8 bg-primary" : "w-1.5", i + 1 < step ? "bg-primary/60" : i + 1 > step && "bg-input")}
        />
      ))}
    </ol>
  );
}

export function Welcome() {
  const [step, setStep] = useState(0);
  const [forward, setForward] = useState(true);
  const go = (to: number) => {
    setForward(to > step);
    setStep(to);
  };
  const saveSettings = useApp((s) => s.saveSettings);
  const setPage = useApp((s) => s.setPage);
  const vcStatus = useApp((s) => s.state?.virtualCamera.status);
  const connected = useApp((s) => s.state?.phase === "connected");
  const last = step === STEPS.length;

  const [leaving, setLeaving] = useState(false);
  const finish = () => {
    setLeaving(true);
    // Waits for the fade out before swapping to the app, skipped when reduced motion is on.
    const delay = matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : FADE_MS;
    setTimeout(() => {
      setPage("dashboard");
      void saveSettings({ onboarded: true }).catch(() => {});
    }, delay);
  };
  const next = last ? finish : () => go(step + 1);
  // Checks if this step is finished so the button can say Continue instead of Skip.
  const done = (step === 1 && (vcStatus === "active" || vcStatus === "unsupported")) || (step === 2 && connected);
  const slide = cn("animate-in fade-in duration-500 ease-out motion-reduce:animate-none", forward ? "slide-in-from-right-12" : "slide-in-from-left-12");

  return (
    <div className={cn("h-full overflow-x-hidden overflow-y-auto bg-background text-foreground", leaving && "pointer-events-none animate-out fade-out fill-mode-forwards duration-200")}>
      <div className="mx-auto flex min-h-full w-full max-w-xl flex-col p-6 sm:p-8">
        {step > 0 && (
          <header className="grid grid-cols-[1fr_auto_1fr] items-center duration-500 animate-in fade-in motion-reduce:animate-none">
            <Button variant="ghost" size="sm" className="justify-self-start" onClick={() => go(step - 1)}>
              <ChevronLeft /> {t("welcome.back")}
            </Button>
            <Progress step={step} />
            <Button variant="ghost" size="sm" className="justify-self-end" onClick={finish}>
              {t("welcome.skipSetup")}
            </Button>
          </header>
        )}

        <main className="flex flex-1 flex-col justify-center py-10">
          {step === 0 ? (
            <IntroStep onStart={() => go(1)} />
          ) : (
            <div key={step} className={cn("flex flex-col gap-8", slide)}>
              {step === 1 ? <VirtualCameraStep /> : <PhoneStep />}
              <Button variant={done ? "default" : "outline"} className="self-end" onClick={next}>
                {t(done ? (last ? "welcome.finish" : "welcome.continue") : last ? "welcome.skipFinish" : "welcome.skip")}
                {done && <ArrowRight />}
              </Button>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
