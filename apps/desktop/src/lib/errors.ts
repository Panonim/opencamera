import { t } from "@opencamera/i18n/desktop";

export type RecoveryAction = "pair" | "settings" | "dismiss";

export interface ErrorInfo {
  title: string;
  body: string;
  action: RecoveryAction;
  actionLabel: string;
}

// Recovery button shown for each kind of problem, the text lives in the locale files under "errors".
const ACTIONS = {
  expired_token: "pair",
  lost_connection: "pair",
  ice_failed: "settings",
  negotiation_failed: "pair",
  no_network: "settings",
  port_in_use: "settings",
  server_failed: "settings",
  unknown: "dismiss",
} satisfies Record<string, RecoveryAction>;

export function errorInfo(code: string): ErrorInfo {
  const key = (Object.hasOwn(ACTIONS, code) ? code : "unknown") as keyof typeof ACTIONS;
  return { title: t(`errors.${key}.title`), body: t(`errors.${key}.body`), action: ACTIONS[key], actionLabel: t(`errors.${key}.action`) };
}
