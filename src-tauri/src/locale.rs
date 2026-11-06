//! Backend locale used for the tray menu and system notifications.
//!
//! The UI is localized in the frontend; the backend only needs to know the
//! language for strings it renders itself (tray, toasts, tooltips).

use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Ru,
    En,
}

static CURRENT: Mutex<Language> = Mutex::new(Language::Ru);

pub fn set(locale: &str) {
    let next = if locale.eq_ignore_ascii_case("en") {
        Language::En
    } else {
        Language::Ru
    };
    *CURRENT.lock().unwrap() = next;
}

pub fn get() -> Language {
    *CURRENT.lock().unwrap()
}

pub fn is_en() -> bool {
    get() == Language::En
}

/// Picks the English or Russian variant.
pub fn tr(ru: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        ru
    }
}
