import { cn } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

export const IS_WINDOWS = navigator.userAgent.includes("Windows");

// Windows' own caption button glyphs: Fluent Icons on Windows 11, MDL2 Assets on Windows 10.
const GLYPH = { minimize: "\uE921", maximize: "\uE922", restore: "\uE923", close: "\uE8BB" };

function Control({ label, glyph, onClick, close }: { label: string; glyph: string; onClick: () => void; close?: boolean }) {
  return (
    <button
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "flex h-full w-[46px] cursor-default items-center justify-center font-['Segoe_Fluent_Icons','Segoe_MDL2_Assets'] text-[10px] text-foreground/80 transition-colors duration-100 outline-none",
        close ? "hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/90" : "hover:bg-foreground/10 hover:text-foreground active:bg-foreground/15",
      )}
    >
      <span aria-hidden>{glyph}</span>
    </button>
  );
}

// Draws our own window top bar on Windows so it matches the app.
export function TitleBar({ sidebar = false }: { sidebar?: boolean }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!IS_WINDOWS) return;
    const win = getCurrentWindow();
    const sync = () => void win.isMaximized().then(setMaximized).catch(() => {});
    sync();
    const unlisten = win.onResized(sync);
    return () => void unlisten.then((f) => f());
  }, []);

  if (!IS_WINDOWS) return null;
  const win = getCurrentWindow();

  return (
    <div data-tauri-drag-region className="flex h-8 shrink-0">
      {sidebar && <div data-tauri-drag-region className="w-56 shrink-0 border-r bg-sidebar" />}
      <div data-tauri-drag-region className="flex-1" />
      <div className="flex">
        <Control label={t("window.minimize")} glyph={GLYPH.minimize} onClick={() => void win.minimize()} />
        <Control label={t(maximized ? "window.restore" : "window.maximize")} glyph={maximized ? GLYPH.restore : GLYPH.maximize} onClick={() => void win.toggleMaximize()} />
        <Control label={t("window.close")} glyph={GLYPH.close} close onClick={() => void win.close()} />
      </div>
    </div>
  );
}
