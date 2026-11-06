import { t } from "@opencamera/i18n/mobile";
import { Ban, CameraOff, Clock, Link2Off, MonitorSmartphone, QrCode, RefreshCw, TriangleAlert, WifiOff } from "lucide-react";
import type { ComponentType } from "react";
import { controller } from "../lib/connection";
import { useMobile, type MessageKind } from "../store";
import { Hero, PrimaryButton, RISE } from "./ui";

const isIOS = /iPhone|iPad/.test(navigator.userAgent);

type Action = "tryAgain" | "reload" | "useThisTab" | "reconnect";

// Title and body text live in the locale files under "message.<kind>".
const CONTENT: Record<MessageKind, { icon: ComponentType<{ className?: string; strokeWidth?: number }>; action?: Action }> = {
  no_pairing: { icon: QrCode },
  permission_denied: { icon: CameraOff, action: "tryAgain" },
  no_camera: { icon: CameraOff, action: "tryAgain" },
  camera_busy: { icon: CameraOff, action: "tryAgain" },
  unsupported: { icon: Ban },
  unknown: { icon: TriangleAlert, action: "tryAgain" },
  invalid_token: { icon: Clock },
  expired_token: { icon: Clock },
  unknown_device: { icon: Link2Off },
  busy: { icon: MonitorSmartphone, action: "tryAgain" },
  version_mismatch: { icon: RefreshCw, action: "reload" },
  replaced: { icon: MonitorSmartphone, action: "useThisTab" },
  desktop_disconnect: { icon: Link2Off, action: "reconnect" },
  user_disconnect: { icon: Link2Off, action: "reconnect" },
  unreachable: { icon: WifiOff, action: "tryAgain" },
};

export function Message() {
  const kind = useMobile((s) => s.message) ?? "unknown";
  const { icon: Icon, action } = CONTENT[kind];
  const body = kind === "permission_denied" && isIOS ? t("message.permission_denied.bodyIos") : t(`message.${kind}.body`);

  function runAction() {
    if (action === "reload") location.reload();
    else void controller.start();
  }

  return (
    <main key={kind} className="mx-auto flex h-full max-w-md flex-col justify-between gap-8 overflow-y-auto px-safe pt-safe pb-safe">
      <div />
      <Hero icon={<Icon className="size-8" strokeWidth={1.5} />} title={t(`message.${kind}.title`)}>
        {body}
      </Hero>
      {action ? (
        <PrimaryButton className={`${RISE} delay-200`} onClick={runAction}>
          {t(`message.actions.${action}`)}
        </PrimaryButton>
      ) : (
        <div />
      )}
    </main>
  );
}
