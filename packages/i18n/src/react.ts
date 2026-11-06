import { createElement, Fragment, useSyncExternalStore, type ReactNode } from "react";
import type { I18n } from "./index";

/** Re-renders the caller whenever the locale changes. */
export function useLocale(i18n: Pick<I18n, "subscribe" | "locale">) {
  return useSyncExternalStore(i18n.subscribe, () => i18n.locale);
}

/** Renders `<tag>text</tag>` parts of a message with the matching component. */
export function rich(text: string, tags: Record<string, (chunk: string) => ReactNode>) {
  const parts = text.split(/<(\w+)>(.*?)<\/\1>/);
  const nodes: ReactNode[] = [];
  for (let i = 0; i < parts.length; i += 3) {
    nodes.push(parts[i]);
    if (i + 1 < parts.length) nodes.push(tags[parts[i + 1]]?.(parts[i + 2]) ?? parts[i + 2]);
  }
  return createElement(Fragment, null, ...nodes);
}
