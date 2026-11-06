import { cn } from "@/lib/utils";
import { useLayoutEffect, useRef } from "react";

// Shows log text, dims debug lines and stays scrolled to the bottom unless the user scrolls up.
export function LogView({ text, className }: { text: string; className?: string }) {
  const ref = useRef<HTMLPreElement>(null);
  const stick = useRef(true);
  const lines = text.split("\n");

  useLayoutEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [text]);

  return (
    <pre
      ref={ref}
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className={cn("max-h-80 overflow-auto rounded-md bg-elevated p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap select-text", className)}
    >
      {lines.map((line, i) => (
        <div key={i} className={cn(line.includes(" DEBUG ") && "text-muted-foreground")}>
          {line || " "}
        </div>
      ))}
    </pre>
  );
}
