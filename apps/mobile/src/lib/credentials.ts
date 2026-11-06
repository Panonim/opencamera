export interface Credentials {
  deviceId: string;
  deviceSecret: string;
  desktopName: string;
}

// Saved pairing details are kept separately for each computer address.
const KEY = "opencamera.device";

export function loadCredentials(): Credentials | null {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as Credentials) : null;
  } catch {
    return null;
  }
}

export function saveCredentials(c: Credentials) {
  try {
    localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    // Private browsing can block saving, so reconnecting will need a new QR scan.
  }
}

export function clearCredentials() {
  try {
    localStorage.removeItem(KEY);
  } catch {
    // Nothing to clear when the browser blocks saved data.
  }
}

function phoneName(ua: string): string {
  if (/iPhone/.test(ua)) return "iPhone";
  if (/iPad/.test(ua)) return "iPad";
  const model = /Android [\d.]+; ([^;)]+)/.exec(ua)?.[1]?.replace(/ Build.*/, "").trim();
  // Newer Chrome hides the phone model and shows "K" instead.
  if (model && model !== "K") return model;
  if (/Android/.test(ua)) return "Android";
  return "Phone";
}

function browserName(ua: string): string {
  if (/EdgA|EdgiOS/.test(ua)) return "Edge";
  if (/CriOS|Chrome/.test(ua)) return "Chrome";
  if (/FxiOS|Firefox/.test(ua)) return "Firefox";
  if (/Safari/.test(ua)) return "Safari";
  return "Browser";
}

export function deviceName(ua = navigator.userAgent): string {
  return `${phoneName(ua)} · ${browserName(ua)}`;
}
