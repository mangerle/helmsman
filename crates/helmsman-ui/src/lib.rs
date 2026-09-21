pub mod diff_view;
pub mod entry_view;
pub mod i18n;
pub mod impact;
pub mod preview;
pub mod state;
pub mod theme;
pub mod validator;

pub use diff_view::{DiffLine, DiffLineType, DiffViewModel};
pub use entry_view::{BootEntryItem, flatten_boot_entries};
pub use i18n::{Language, TextKey, detect_system_language, format_text, get_text};
pub use impact::{ImpactAnalyzer, ImpactItem, ImpactReport};
pub use preview::{MenuPreviewModel, PreviewEntry};
pub use state::AppState;
pub use theme::{
    ColorPalette, FontConfig, FontScale, ResolvedTheme, RgbaColor, SystemColorScheme, ThemeMode,
    WindowConfig,
};
pub use validator::{RiskLevel, SafetyValidator, ValidationIssue};
