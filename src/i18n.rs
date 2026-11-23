//! Application copy only. User content and media metadata are never translated.
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Indonesian,
    English,
}
static ENGLISH: AtomicBool = AtomicBool::new(false);
pub fn set_language(language: Language) {
    ENGLISH.store(language == Language::English, Ordering::Relaxed);
}
pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}
fn catalog() -> &'static std::collections::HashMap<String, String> {
    static CATALOG: OnceLock<std::collections::HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/locales/en.json"))
            .expect("validated English catalog")
    })
}
pub fn translate_for(language: Language, text: &str) -> &str {
    if language == Language::English {
        catalog().get(text).map_or(text, String::as_str)
    } else {
        text
    }
}
pub fn tr(text: &str) -> &str {
    translate_for(
        if english() {
            Language::English
        } else {
            Language::Indonesian
        },
        text,
    )
}
pub fn message(text: &str) -> std::borrow::Cow<'_, str> {
    if !english() {
        return text.into();
    }
    for (prefix, english) in [
        ("Belum tersimpan: ", "Not saved: "),
        ("Data tidak diubah: ", "Data was not changed: "),
        ("Pengaturan belum diubah: ", "Settings were not changed: "),
        ("Startup belum diperbarui: ", "Startup was not updated: "),
    ] {
        if let Some(detail) = text.strip_prefix(prefix) {
            return std::format!("{english}{}", tr(detail)).into();
        }
    }
    tr(text).into()
}
pub fn date(date: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    std::format!(
        "{}, {} {} {}",
        tr(crate::calendar::DAYS_FULL[date.weekday().num_days_from_monday() as usize]),
        date.day(),
        tr(crate::calendar::MONTHS[date.month0() as usize]),
        date.year()
    )
}

// Both branches use Rust's format checking; values are formatted once and are
// never searched/replaced as translated text.
macro_rules! format {
    ($id:literal, $en:literal $(, $args:expr)* $(,)?) => {
        if $crate::i18n::english() { std::format!($en $(, $args)*) }
        else { std::format!($id $(, $args)*) }
    };
}
pub(crate) use format;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_fallback_preserves_content_and_switches_application_copy() {
        assert_eq!(translate_for(Language::English, "Tampilan"), "Appearance");
        assert_eq!(translate_for(Language::Indonesian, "Tampilan"), "Tampilan");
        assert_eq!(
            translate_for(Language::English, "My task: Belum ada tugas"),
            "My task: Belum ada tugas"
        );
        assert!(catalog().len() > 150);
    }
    #[test]
    fn translated_formats_preserve_every_placeholder() {
        fn fields(text: &str) -> Vec<&str> {
            text.split('{')
                .skip(1)
                .filter_map(|s| s.split_once('}').map(|(field, _)| field))
                .collect()
        }
        for (id, en) in catalog() {
            assert_eq!(fields(id), fields(en), "Placeholder mismatch: {id}");
        }
    }
}
