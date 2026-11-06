import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { languageName } from "@opencamera/i18n";
import { i18n, t } from "@opencamera/i18n/desktop";
import { useState } from "react";
import { Card, Page, PageHeader, SaveBar, SettingRow } from "../components/common";
import { VirtualCameraCard } from "../components/VirtualCameraCard";
import type { Settings } from "../lib/api";
import { useApp } from "../store";

const AUTO = "auto";
const SYSTEM = "system";

type Draft = Pick<Settings, "theme" | "language" | "launchAtLogin" | "keepRunning" | "autoReconnect" | "notifications" | "address" | "developerMode"> & {
  port: string;
  iceServer: string;
};

const toDraft = (s: Settings): Draft => ({
  theme: s.theme,
  language: s.language,
  launchAtLogin: s.launchAtLogin,
  keepRunning: s.keepRunning,
  autoReconnect: s.autoReconnect,
  notifications: s.notifications,
  address: s.address,
  port: String(s.port),
  iceServer: s.iceServer,
  developerMode: s.developerMode,
});

export function SettingsPage() {
  const settings = useApp((s) => s.settings);
  return settings ? <SettingsForm settings={settings} /> : null;
}

function SettingsForm({ settings }: { settings: Settings }) {
  const saveSettings = useApp((s) => s.saveSettings);
  const addresses = useApp((s) => s.addresses);
  const server = useApp((s) => s.state?.server);
  const [draft, setDraft] = useState(() => toDraft(settings));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const set = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));

  const next = { ...draft, port: Number(draft.port), iceServer: draft.iceServer.trim() };
  const portValid = Number.isInteger(next.port) && next.port >= 1024 && next.port <= 65535;
  const iceValid = next.iceServer === "" || /^(stun|turn)s?:/.test(next.iceServer);
  const dirty = (Object.keys(next) as (keyof typeof next)[]).some((k) => next[k] !== settings[k]);

  const save = () => {
    setError(null);
    setSaving(true);
    saveSettings(next)
      .catch((e) => setError(String(e)))
      .finally(() => setSaving(false));
  };
  const discard = () => {
    setError(null);
    setDraft(toDraft(settings));
  };

  return (
    <Page className="max-w-3xl">
      <PageHeader title={t("nav.settings")} description={t("settings.description")} />
      {error && <div className="mb-5 rounded-md border border-danger/30 bg-danger/10 p-3 text-sm">{error}</div>}
      <div className="flex flex-col gap-5">
        <Card title={t("settings.general")}>
          <div className="divide-y">
            <SettingRow label={t("settings.language")}>
              <Select value={draft.language} onValueChange={(language) => set({ language })}>
                <SelectTrigger className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={SYSTEM}>{t("settings.system")}</SelectItem>
                  {i18n.locales.map((code) => (
                    <SelectItem key={code} value={code}>
                      {languageName(code)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </SettingRow>
            <SettingRow label={t("settings.theme")}>
              <Select value={draft.theme} onValueChange={(theme) => set({ theme: theme as Settings["theme"] })}>
                <SelectTrigger className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={SYSTEM}>{t("settings.system")}</SelectItem>
                  <SelectItem value="light">{t("settings.themeMode.light")}</SelectItem>
                  <SelectItem value="dark">{t("settings.themeMode.dark")}</SelectItem>
                </SelectContent>
              </Select>
            </SettingRow>
            <SettingRow label={t("settings.launchAtLogin")} description={t("settings.launchAtLoginBody")}>
              <Switch checked={draft.launchAtLogin} onCheckedChange={(launchAtLogin) => set({ launchAtLogin })} />
            </SettingRow>
            <SettingRow label={t("settings.keepRunning")} description={t("settings.keepRunningBody")}>
              <Switch checked={draft.keepRunning} onCheckedChange={(keepRunning) => set({ keepRunning })} />
            </SettingRow>
            <SettingRow label={t("settings.autoReconnect")} description={t("settings.autoReconnectBody")}>
              <Switch checked={draft.autoReconnect} onCheckedChange={(autoReconnect) => set({ autoReconnect })} />
            </SettingRow>
            <SettingRow label={t("settings.notifications")} description={t("settings.notificationsBody")}>
              <Switch checked={draft.notifications} onCheckedChange={(notifications) => set({ notifications })} />
            </SettingRow>
            <SettingRow label={t("settings.setupGuide")} description={t("settings.setupGuideBody")}>
              <Button size="sm" variant="outline" onClick={() => void saveSettings({ onboarded: false })}>
                {t("settings.openSetupGuide")}
              </Button>
            </SettingRow>
          </div>
        </Card>

        <Card title={t("settings.network")}>
          <div className="divide-y">
            <SettingRow label={t("settings.status")} description={t("settings.statusBody")}>
              <span className="font-mono text-xs text-muted-foreground">{server?.running ? `${server.address}:${server.port}` : t("settings.notListening")}</span>
            </SettingRow>
            <SettingRow label={t("settings.address")} description={t("settings.addressBody")}>
              <Select value={draft.address || AUTO} onValueChange={(a) => set({ address: a === AUTO ? "" : a })}>
                <SelectTrigger className="w-48">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={AUTO}>{t("settings.automatic")}</SelectItem>
                  {addresses.map((a) => (
                    <SelectItem key={a} value={a}>
                      {a}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </SettingRow>
            <SettingRow label={t("settings.port")} description={t("settings.portBody")}>
              <Input className="w-28" inputMode="numeric" value={draft.port} aria-invalid={!portValid} onChange={(e) => set({ port: e.target.value })} />
            </SettingRow>
            <SettingRow label={t("settings.iceServer")} description={t("settings.iceServerBody")}>
              <Input
                className="w-64"
                placeholder="stun:stun.example.com:3478"
                value={draft.iceServer}
                aria-invalid={!iceValid}
                onChange={(e) => set({ iceServer: e.target.value })}
              />
            </SettingRow>
          </div>
        </Card>

        <VirtualCameraCard />

        <Card title={t("settings.advanced")}>
          <SettingRow label={t("settings.developerMode")} description={t("settings.developerModeBody")}>
            <Switch checked={draft.developerMode} onCheckedChange={(developerMode) => set({ developerMode })} />
          </SettingRow>
        </Card>
      </div>
      <SaveBar visible={dirty} canSave={portValid && iceValid} saving={saving} onSave={save} onDiscard={discard} />
    </Page>
  );
}
