import { getCurrentWindow } from "@tauri-apps/api/window";
import { cn } from "@/lib/utils";
import { i18n } from "@opencamera/i18n/desktop";
import { useLocale } from "@opencamera/i18n/react";
import { useEffect, useLayoutEffect, useRef } from "react";
import { PairDialog } from "./components/PairDialog";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Toaster } from "./components/Toaster";
import { Skeleton } from "./components/ui/skeleton";
import { TooltipProvider } from "./components/ui/tooltip";
import { About } from "./pages/About";
import { Camera } from "./pages/Camera";
import { Dashboard } from "./pages/Dashboard";
import { SettingsPage } from "./pages/Settings";
import { Welcome } from "./pages/Welcome";
import { useApp } from "./store";

function useTheme() {
  const theme = useApp((s) => s.settings?.theme ?? "system");
  useEffect(() => {
    // Makes the window's top bar follow the chosen light or dark look.
    void getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {});
    const query = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && query.matches);
      document.documentElement.classList.toggle("dark", dark);
    };
    apply();
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, [theme]);
}

// Applies the chosen language before paint, and the locale change re-renders the whole tree.
function useLanguage() {
  const language = useApp((s) => s.settings?.language ?? "system");
  useLayoutEffect(() => i18n.setLocale(language === "system" ? navigator.languages : language), [language]);
  useLocale(i18n);
}

const PAGES = { dashboard: Dashboard, camera: Camera, settings: SettingsPage, about: About };

export function App() {
  useTheme();
  useLanguage();
  const ready = useApp((s) => s.ready);
  const page = useApp((s) => s.page);
  const onboarded = useApp((s) => s.settings?.onboarded ?? true);
  const Page = PAGES[page];
  const showWelcome = ready && !onboarded;
  // Remembers the tour was shown so the app fades in after it, but not on a normal launch.
  const toured = useRef(false);
  if (showWelcome) toured.current = true;

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full flex-col bg-background text-foreground">
        <TitleBar sidebar={!showWelcome} />
        {showWelcome ? (
          <div className="min-h-0 flex-1">
            <Welcome />
          </div>
        ) : (
          <div className={cn("flex min-h-0 flex-1", toured.current && "animate-in fade-in duration-200 motion-reduce:animate-none")}>
            <Sidebar />
            <main className="min-w-0 flex-1 overflow-y-auto">
              {ready ? (
                <Page />
              ) : (
                <div className="flex flex-col gap-6 p-8">
                  <Skeleton className="h-7 w-48" />
                  <Skeleton className="aspect-video w-full max-w-4xl" />
                </div>
              )}
            </main>
          </div>
        )}
      </div>
      <PairDialog />
      <Toaster />
    </TooltipProvider>
  );
}
