import { cn } from "@/lib/utils";
import { t } from "@opencamera/i18n/desktop";
import { ChevronRight } from "lucide-react";

const REASONS = ["https", "authority", "encrypted", "remembered"] as const;

// Explains the phone browser's self-signed certificate warning.
export function CertificateHelp({ className }: { className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1.5 text-muted-foreground", className)}>
      <p>{t("certificate.intro")}</p>
      <CertificateWhy />
    </div>
  );
}

export function CertificateWhy({ className }: { className?: string }) {
  return (
    <details className={cn("group text-muted-foreground", className)}>
      <summary className="flex w-fit cursor-pointer list-none items-center gap-1 text-xs font-medium transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden">
        <ChevronRight className="size-3.5 transition-transform group-open:rotate-90" /> {t("certificate.why")}
      </summary>
      <ul className="mt-1.5 flex list-disc flex-col gap-1 pl-5 text-xs leading-relaxed">
        {REASONS.map((r) => (
          <li key={r}>{t(`certificate.reasons.${r}`)}</li>
        ))}
      </ul>
    </details>
  );
}
