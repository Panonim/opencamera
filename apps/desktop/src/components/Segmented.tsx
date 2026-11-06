import { cn } from "@/lib/utils";

// One highlight slides between equal-width options, so switching never shows two half-selected options.
export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  disabled,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  const index = options.findIndex((o) => o.value === value);
  return (
    <div role="radiogroup" className={cn("relative inline-grid grid-flow-col auto-cols-fr rounded-md border bg-elevated p-0.5", disabled && "opacity-50")}>
      {index >= 0 && (
        <span
          aria-hidden
          className="absolute inset-y-0.5 left-0.5 rounded-[5px] bg-primary shadow-xs transition-transform duration-200 ease-out motion-reduce:transition-none"
          style={{ width: `calc((100% - 4px) / ${options.length})`, transform: `translateX(${index * 100}%)` }}
        />
      )}
      {options.map((o, i) => (
        <button
          key={String(o.value)}
          role="radio"
          aria-checked={i === index}
          disabled={disabled}
          onClick={() => i !== index && onChange(o.value)}
          className={cn(
            "relative h-7 rounded-[5px] px-3 text-xs font-medium whitespace-nowrap outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-ring/50",
            i === index ? "text-primary-foreground" : "text-muted-foreground enabled:hover:text-foreground",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
