//! Translations. Every string the UI shows lives in `locales/<lang>.lang`
//! (`key = text` lines, `#` comments, `{name}` placeholders), after term's
//! `.lang` files. English is the fallback for any key a language lacks.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    /// The TV's (or the desktop's) own language.
    #[default]
    Auto,
    English,
    French,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::Auto, Language::English, Language::French];
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Lang {
    En,
    Fr,
}

const EN: &str = include_str!("../locales/en.lang");
const FR: &str = include_str!("../locales/fr.lang");

fn table(lang: Lang) -> &'static HashMap<&'static str, &'static str> {
    static EN_T: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    static FR_T: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    let parse = |src: &'static str| {
        src.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim(), v.trim())))
            .collect()
    };
    match lang {
        Lang::En => EN_T.get_or_init(|| parse(EN)),
        Lang::Fr => FR_T.get_or_init(|| parse(FR)),
    }
}

static CURRENT: RwLock<Lang> = RwLock::new(Lang::En);
/// The system locale ("fr-FR"), set by the TV backend or from $LANG.
static SYSTEM: RwLock<Option<String>> = RwLock::new(None);

#[cfg_attr(not(feature = "webos"), allow(dead_code))] // the desktop reads $LANG in select()
pub fn set_system_locale(locale: &str) {
    if let Ok(mut s) = SYSTEM.write() {
        *s = Some(locale.to_string());
    }
}

/// Picks the language for `choice`, resolving Auto from the system locale.
pub fn select(choice: Language) {
    let lang = match choice {
        Language::English => Lang::En,
        Language::French => Lang::Fr,
        Language::Auto => {
            let sys = SYSTEM.read().ok().and_then(|s| s.clone()).or_else(|| {
                ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
            });
            if sys.is_some_and(|s| s.to_ascii_lowercase().starts_with("fr")) { Lang::Fr } else { Lang::En }
        }
    };
    if let Ok(mut c) = CURRENT.write() {
        *c = lang;
    }
}

/// The text for `key` in the current language (the key itself if unknown).
pub fn t(key: &'static str) -> &'static str {
    let lang = CURRENT.read().map(|l| *l).unwrap_or(Lang::En);
    table(lang).get(key).or_else(|| table(Lang::En).get(key)).copied().unwrap_or(key)
}

/// `t(key)` with `{name}` placeholders filled.
pub fn tf(key: &'static str, args: &[(&str, &str)]) -> String {
    let mut s = t(key).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_french_key_exists_in_english_and_back() {
        let en = table(Lang::En);
        let fr = table(Lang::Fr);
        for k in fr.keys() {
            assert!(en.contains_key(k), "fr has unknown key {k}");
        }
        for k in en.keys() {
            assert!(fr.contains_key(k), "fr is missing {k}");
        }
    }

    fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
        text.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(name, _)| name)).collect()
    }

    #[test]
    fn placeholders_match_between_languages() {
        let en = table(Lang::En);
        let fr = table(Lang::Fr);
        let mut checked = 0;
        for (k, e) in en {
            let Some(f) = fr.get(k) else { continue };
            assert_eq!(placeholders(e), placeholders(f), "placeholders of {k}: en {e:?}, fr {f:?}");
            checked += placeholders(e).len();
        }
        assert!(checked > 0, "no placeholder found at all");
    }
}
