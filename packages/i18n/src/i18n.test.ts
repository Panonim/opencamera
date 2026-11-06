import { describe, expect, it } from "vitest";
import * as desktop from "./desktop";
import { createI18n, languageName, resolveLocale } from "./index";
import * as mobile from "./mobile";

const PLURAL = /_(zero|one|two|few|many|other)$/;

function flatten(node: object, prefix = ""): Record<string, string> {
  return Object.entries(node).reduce<Record<string, string>>(
    (out, [k, v]) => ({ ...out, ...(typeof v === "string" ? { [prefix + k]: v } : flatten(v, `${prefix}${k}.`)) }),
    {},
  );
}

const vars = (text: string) => [...text.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();

const sample = createI18n({
  en: { greeting: "Hello {{name}}", files_one: "{{count}} file", files_other: "{{count}} files", only: "English only" },
  pl: { greeting: "Cześć {{name}}", files_one: "{{count}} plik", files_few: "{{count}} pliki", files_many: "{{count}} plików" },
});

describe("t", () => {
  it("fills variables", () => {
    sample.setLocale("en");
    expect(sample.t("greeting", { name: "Ada" })).toBe("Hello Ada");
  });
  it("keeps unknown placeholders", () => expect(sample.t("greeting")).toBe("Hello {{name}}"));
  it("returns the key when missing", () => expect(sample.t("nope" as never)).toBe("nope"));
  it("picks English plural forms", () => {
    sample.setLocale("en");
    expect(sample.t("files", { count: 1 })).toBe("1 file");
    expect(sample.t("files", { count: 3 })).toBe("3 files");
  });
  it("picks Polish plural forms", () => {
    sample.setLocale("pl");
    expect(sample.t("files", { count: 1 })).toBe("1 plik");
    expect(sample.t("files", { count: 3 })).toBe("3 pliki");
    expect(sample.t("files", { count: 5 })).toBe("5 plików");
  });
  it("falls back to English per key", () => {
    sample.setLocale("pl");
    expect(sample.t("only")).toBe("English only");
  });
});

describe("setLocale", () => {
  it("notifies only on change", () => {
    const i18n = createI18n({ en: {}, de: {} });
    let calls = 0;
    const off = i18n.subscribe(() => calls++);
    i18n.setLocale("de-AT");
    i18n.setLocale("de");
    off();
    i18n.setLocale("en");
    expect(i18n.locale).toBe("en");
    expect(calls).toBe(1);
  });
});

describe("resolveLocale", () => {
  const available = ["en", "pt-BR", "pl"];
  it("matches exact tags case-insensitively", () => expect(resolveLocale(["PT-br"], available)).toBe("pt-BR"));
  it("matches base language", () => expect(resolveLocale(["pl-PL"], available)).toBe("pl"));
  it("uses the first match in order", () => expect(resolveLocale(["fr", "pl", "en"], available)).toBe("pl"));
  it("falls back to English", () => expect(resolveLocale(["ja"], available)).toBe("en"));
});

describe("languageName", () => {
  it("uses the native name", () => expect(languageName("en")).toBe("English"));
});

describe("locale files", () => {
  it.each([
    ["desktop", desktop.locales],
    ["mobile", mobile.locales],
  ] as const)("%s translations match English keys and placeholders", (_, locales) => {
    const en = flatten(locales.en);
    const bases = new Set(Object.keys(en).map((k) => k.replace(PLURAL, "")));
    for (const [code, messages] of Object.entries(locales)) {
      for (const [key, text] of Object.entries(flatten(messages))) {
        const base = key.replace(PLURAL, "");
        expect(bases.has(base), `${code}: unknown key ${key}`).toBe(true);
        const source = en[key] ?? en[base] ?? en[`${base}_other`];
        expect(vars(text).filter((v) => v !== "count"), `${code}: ${key}`).toEqual(vars(source).filter((v) => v !== "count"));
      }
    }
  });
});
