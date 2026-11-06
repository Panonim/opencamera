import type { ButtonHTMLAttributes, ReactNode } from "react";

/** Fades content up into place, staggered by `delay` classes. */
export const RISE = "animate-in fade-in slide-in-from-bottom-3 fill-mode-both duration-500 ease-out motion-reduce:animate-none";

export function PrimaryButton({ className = "", ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      {...props}
      className={`flex h-14 w-full items-center justify-center gap-2 rounded-2xl bg-foreground text-[16px] font-semibold text-background shadow-lg shadow-black/10 transition-[opacity,transform] duration-150 active:scale-[0.98] active:opacity-90 disabled:opacity-60 ${className}`}
    />
  );
}

/** Centered icon, title and text used by the full-page screens. */
export function Hero({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-center text-center">
      <div className={`relative mb-7 ${RISE}`}>
        <div className="absolute -inset-5 rounded-full bg-primary/20 blur-2xl" />
        <div className="relative flex size-20 items-center justify-center rounded-3xl border bg-card shadow-lg">{icon}</div>
      </div>
      <h1 className={`text-2xl font-semibold tracking-tight text-balance ${RISE} delay-75`}>{title}</h1>
      <p className={`mt-3 max-w-xs text-[15px] leading-relaxed text-balance text-muted-foreground ${RISE} delay-150`}>{children}</p>
    </div>
  );
}

const ROUND_SIZE = { sm: "size-11", md: "size-14", lg: "size-[72px]" };

export function RoundButton({
  label,
  children,
  active = false,
  size = "md",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; children: ReactNode; active?: boolean; size?: keyof typeof ROUND_SIZE }) {
  return (
    <button
      {...props}
      aria-label={label}
      className={`flex items-center justify-center rounded-full ring-1 backdrop-blur-md transition-[background-color,transform] duration-150 active:scale-95 disabled:opacity-40 ${ROUND_SIZE[size]} ${
        active ? "bg-white text-black ring-white" : "bg-black/45 text-white ring-white/15"
      }`}
    >
      {children}
    </button>
  );
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string; disabled?: boolean }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="grid auto-cols-fr grid-flow-col gap-1 rounded-xl bg-elevated p-1">
      {options.map((o) => (
        <button
          key={String(o.value)}
          disabled={o.disabled}
          onClick={() => onChange(o.value)}
          className={`h-10 min-w-0 truncate rounded-lg px-1 text-sm font-medium transition-colors duration-150 disabled:opacity-35 ${
            o.value === value ? "bg-card text-foreground shadow-sm" : "text-muted-foreground"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
