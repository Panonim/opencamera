import en from "../locales/en/desktop.json";
import fr from "../locales/fr/desktop.json";
import pl from "../locales/pl/desktop.json";
import { createI18n } from "./index";

export const locales = { en, fr, pl };
export const i18n = createI18n(locales);
export const { t } = i18n;
