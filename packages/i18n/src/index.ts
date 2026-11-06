export type PluralCategory = Intl.LDMLPluralRule;
export type Vars = Record<string, string | number>;

type Leaves<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string ? `${P}${K}` : Leaves<T[K], `${P}${K}.`>;
}[keyof T & string];
type Base<K> = K extends `${infer B}_${PluralCategory}` ? B : K;
/** Every dotted message key, with plural forms like `files_one` collapsed to `files`. */
export type Key<T> = Base<Leaves<T>>;

/** Picks the first available locale matching the preferred tags exactly or by base language. */
export function resolveLocale(preferred: readonly string[], available: readonly string[], fallback = "en") {
  const lower = available.map((a) => a.toLowerCase());
  for (const tag of preferred.map((p) => p.toLowerCase())) {
    const i = lower.indexOf(tag) >= 0 ? lower.indexOf(tag) : lower.indexOf(tag.split("-")[0]);
    if (i >= 0) return available[i];
  }
  return fallback;
}

/** Native name of a language, e.g. "Deutsch" for "de". */
export function languageName(code: string) {
  const name = new Intl.DisplayNames([code], { type: "language" }).of(code) ?? code;
  return name.charAt(0).toLocaleUpperCase(code) + name.slice(1);
}

/** English is the source locale, other locales may be partial and fall back to it key by key. */
export function createI18n<T extends object>(locales: { en: T } & Record<string, object>) {
  let locale = "en";
  const listeners = new Set<() => void>();
  const lookup = (lang: string, key: string) => {
    const value = key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown> | undefined)?.[part], locales[lang]);
    return typeof value === "string" ? value : undefined;
  };
  const find = (key: string) => lookup(locale, key) ?? lookup("en", key);

  return {
    get locale() {
      return locale;
    },
    locales: Object.keys(locales),
    t(key: Key<T>, vars: Vars = {}): string {
      const count = vars.count;
      const plural = typeof count === "number" ? (find(`${key}_${new Intl.PluralRules(locale).select(count)}`) ?? find(`${key}_other`)) : undefined;
      return (plural ?? find(key) ?? key).replace(/\{\{(\w+)\}\}/g, (match, name: string) => (name in vars ? String(vars[name]) : match));
    },
    setLocale(preferred: string | readonly string[]) {
      const next = resolveLocale(typeof preferred === "string" ? [preferred] : preferred, Object.keys(locales));
      if (typeof document !== "undefined") document.documentElement.lang = next;
      if (next === locale) return;
      locale = next;
      listeners.forEach((fn) => fn());
    },
    subscribe(fn: () => void) {
      listeners.add(fn);
      return () => void listeners.delete(fn);
    },
  };
}

export type I18n<T extends object = object> = ReturnType<typeof createI18n<T>>;
