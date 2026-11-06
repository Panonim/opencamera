//! Looks up tray and notification text in the same locale files the app screens use.

use serde_json::Value;
use std::sync::LazyLock;

/// Locale codes with their desktop messages, English first since it is the fallback.
const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../../../../packages/i18n/locales/en/desktop.json")),
    ("fr", include_str!("../../../../packages/i18n/locales/fr/desktop.json")),
    ("pl", include_str!("../../../../packages/i18n/locales/pl/desktop.json")),
];

static PARSED: LazyLock<Vec<(&str, Value)>> =
    LazyLock::new(|| LOCALES.iter().map(|(code, json)| (*code, serde_json::from_str(json).expect("invalid locale file"))).collect());

/// Turns the language setting ("system" or a code) into an available locale.
pub fn resolve(language: &str) -> &'static str {
    let preferred: Vec<String> = if language == "system" { sys_locale::get_locales().collect() } else { vec![language.into()] };
    pick(&preferred)
}

/// Same matching as the frontend: exact tag first, then base language, per preferred tag in order.
fn pick(preferred: &[String]) -> &'static str {
    for tag in preferred {
        let tag = tag.to_lowercase().replace('_', "-");
        let base = tag.split('-').next().unwrap_or_default();
        let found = LOCALES.iter().find(|(c, _)| c.to_lowercase() == tag).or_else(|| LOCALES.iter().find(|(c, _)| c.to_lowercase() == base));
        if let Some((code, _)) = found {
            return code;
        }
    }
    "en"
}

/// Returns the message at a dotted key with `{{name}}` placeholders filled, falling back to English.
pub fn t(locale: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let find = |code: &str| {
        let (_, messages) = PARSED.iter().find(|(c, _)| *c == code)?;
        key.split('.').try_fold(messages, |node, part| node.get(part))?.as_str()
    };
    let text = find(locale).or_else(|| find("en")).unwrap_or(key);
    vars.iter().fold(text.to_string(), |s, (name, value)| s.replace(&format!("{{{{{name}}}}}"), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_exact_then_base_language() {
        assert_eq!(pick(&["de-DE".into(), "en_GB".into()]), "en");
        assert_eq!(pick(&["fr-FR".into()]), "fr");
        assert_eq!(pick(&["ja".into()]), "en");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(t("en", "notify.connected.body", &[("name", "Pixel")]), "Pixel is streaming to OpenCamera Virtual Camera.");
    }

    #[test]
    fn falls_back_to_key() {
        assert_eq!(t("en", "missing.key", &[]), "missing.key");
    }

    #[test]
    fn every_used_key_exists() {
        let keys = ["tray.open", "tray.launchAtLogin", "tray.quit", "notify.yourPhone"];
        let notices = ["notify.connected", "notify.disconnected", "notify.batteryLow"];
        let all = keys.iter().map(|k| k.to_string()).chain(notices.iter().flat_map(|n| [format!("{n}.title"), format!("{n}.body")]));
        for key in all {
            assert_ne!(t("en", &key, &[]), key, "missing {key}");
        }
    }
}
