import { t } from "@opencamera/i18n/mobile";
import { Logo } from "@opencamera/ui/Logo";
import { Camera, Loader2, Lock, ShieldCheck } from "lucide-react";
import { controller } from "../lib/connection";
import { useMobile } from "../store";
import { Hero, PrimaryButton, RISE } from "./ui";

const NOTES = [
  { icon: ShieldCheck, key: "intro.permissionNote" },
  { icon: Lock, key: "intro.warningNote" },
] as const;

export function Intro() {
  const desktopName = useMobile((s) => s.desktopName);
  const starting = useMobile((s) => s.screen === "starting");

  return (
    <main className="mx-auto flex h-full max-w-md flex-col justify-between gap-8 overflow-y-auto px-safe pt-safe pb-safe">
      <div className="flex items-center gap-2 pt-2 text-sm font-medium text-muted-foreground">
        <Logo className="size-4" /> OpenCamera
      </div>
      <Hero icon={<Logo className="size-10" />} title={desktopName ? t("intro.titleNamed", { name: desktopName }) : t("intro.title")}>
        {t("intro.body")}
      </Hero>
      <div className={`flex flex-col gap-4 ${RISE} delay-200`}>
        <ul className="flex flex-col divide-y rounded-2xl bg-card text-[13px] leading-snug text-muted-foreground">
          {NOTES.map(({ icon: Icon, key }) => (
            <li key={key} className="flex items-start gap-3 p-3.5">
              <Icon className="mt-px size-4 shrink-0 text-foreground" />
              {t(key)}
            </li>
          ))}
        </ul>
        <PrimaryButton onClick={() => void controller.start()} disabled={starting}>
          {starting ? <Loader2 className="size-5 animate-spin" /> : <Camera className="size-5" />}
          {t(starting ? "intro.starting" : "intro.start")}
        </PrimaryButton>
      </div>
    </main>
  );
}
