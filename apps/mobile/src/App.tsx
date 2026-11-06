import { readPairingToken } from "@opencamera/protocol";
import { useEffect } from "react";
import { Intro } from "./components/Intro";
import { Live } from "./components/Live";
import { Message } from "./components/Message";
import { browserSupportError } from "./lib/camera";
import { controller } from "./lib/connection";
import { loadCredentials } from "./lib/credentials";
import { setMobile, useMobile } from "./store";

export function App() {
  const screen = useMobile((s) => s.screen);

  useEffect(() => {
    const token = readPairingToken(location.hash);
    // Removes the one-time code from the address bar so reloading the page can't reuse it.
    if (location.hash) history.replaceState(null, "", location.pathname);
    controller.setToken(token);

    const unsupported = browserSupportError();
    if (unsupported) {
      setMobile({ screen: "message", message: unsupported });
      return;
    }
    const creds = loadCredentials();
    if (!token && !creds) {
      setMobile({ screen: "message", message: "no_pairing" });
      return;
    }
    setMobile({ desktopName: creds?.desktopName ?? null });
    // A phone that was paired before starts streaming right away without the welcome screen.
    if (!token) void controller.start();
  }, []);

  if (screen === "live") return <Live />;
  if (screen === "message") return <Message />;
  return <Intro />;
}
