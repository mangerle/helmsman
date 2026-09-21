pub mod i18n;
pub mod state;

pub use i18n::{Language, TextKey, detect_system_language, format_text, get_text};
pub use state::AppState;
