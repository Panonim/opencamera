import en from "../locales/en/mobile.json";
import fr from "../locales/fr/mobile.json";
import pl from "../locales/pl/mobile.json";
import { createI18n } from "./index";

export const locales = { en, fr, pl };
export const i18n = createI18n(locales);
export const { t } = i18n;
