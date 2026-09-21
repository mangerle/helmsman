pub mod diff_view;
pub mod i18n;
pub mod state;
pub mod theme;
pub mod validator;

pub use diff_view::{DiffLine, DiffLineType, DiffViewModel};
pub use i18n::{Language, TextKey, detect_system_language, format_text, get_text};
pub use state::AppState;
pub use theme::{FontConfig, ThemeMode, WindowConfig};
pub use validator::{RiskLevel, SafetyValidator, ValidationIssue};
