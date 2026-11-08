import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { t } from "@opencamera/i18n/desktop";
import type { PhoneSettings } from "@opencamera/protocol";
import { Bookmark, X } from "lucide-react";
import { useState } from "react";
import type { Processing } from "../lib/api";
import { addPreset, pickPhone, removePreset, type Preset } from "../lib/presets";
import { Card } from "./common";

export function PresetsCard({
  presets,
  phone,
  processing,
  onChange,
  onApply,
}: {
  presets: Preset[];
  phone: PhoneSettings | undefined;
  processing: Processing;
  onChange: (presets: Preset[]) => void;
  onApply: (preset: Preset) => void;
}) {
  const [name, setName] = useState("");
  const save = () => {
    const trimmed = name.trim();
    if (!trimmed || !phone) return;
    onChange(addPreset(presets, { id: crypto.randomUUID(), name: trimmed, phone: pickPhone(phone), processing }));
    setName("");
  };

  return (
    <Card title={t("presets.title")} badge={<Bookmark className="size-4 text-muted-foreground" />}>
      {presets.length === 0 ? (
        <p className="mb-4 text-sm text-muted-foreground">{t("presets.empty")}</p>
      ) : (
        <div className="mb-4 flex flex-wrap gap-2">
          {presets.map((p) => (
            <div key={p.id} className="inline-flex items-center rounded-md bg-elevated">
              <button onClick={() => onApply(p)} className="h-8 rounded-l-md px-3 text-sm font-medium transition-colors hover:bg-accent">
                {p.name}
              </button>
              <button aria-label={t("presets.delete", { name: p.name })} onClick={() => onChange(removePreset(presets, p.id))} className="grid h-8 w-8 place-items-center rounded-r-md text-muted-foreground transition-colors hover:text-foreground">
                <X className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Input value={name} maxLength={24} placeholder={t("presets.namePlaceholder")} className="w-48 max-w-full" onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
        <Button size="sm" variant="outline" disabled={!name.trim() || !phone} onClick={save}>
          {t("presets.save")}
        </Button>
      </div>
      {!phone && <p className="mt-2 text-xs text-muted-foreground">{t("presets.needsPhone")}</p>}
    </Card>
  );
}
