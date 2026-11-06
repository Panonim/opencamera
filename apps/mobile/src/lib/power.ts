import type { PhonePower, ThermalState } from "@opencamera/protocol";

interface BatteryManager extends EventTarget {
  level: number;
  charging: boolean;
}

type PressureRecord = { state: ThermalState };
declare const PressureObserver: (new (cb: (records: PressureRecord[]) => void) => { observe(source: "cpu"): Promise<void> }) | undefined;

export const UNKNOWN_POWER: PhonePower = { battery: null, charging: null, thermal: null };

/** Watches battery and processor heat where the browser supports it, values stay null elsewhere. */
export function watchPower(onChange: (patch: Partial<PhonePower>) => void) {
  const nav = navigator as Navigator & { getBattery?: () => Promise<BatteryManager> };
  void nav.getBattery?.().then((b) => {
    const emit = () => onChange({ battery: b.level, charging: b.charging });
    emit();
    b.addEventListener("levelchange", emit);
    b.addEventListener("chargingchange", emit);
  }).catch(() => {});
  try {
    if (typeof PressureObserver !== "undefined") {
      void new PressureObserver((records) => onChange({ thermal: records.at(-1)?.state ?? null })).observe("cpu").catch(() => {});
    }
  } catch {
    // Pressure readings may be blocked by browser policy.
  }
}
