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
pub use preview::{MenuPreviewModel, PreviewEntry, Rect, VisualLayoutSnapshot};
pub use state::AppState;
// 重新导出引导默认项类型契约，供上层前端直接消费
pub use grub_config_parser::{
    BootKeyError, CmdlineError, DefaultEntry, DisplayConfigError, GfxMode, GrubColor,
    KernelCmdline, MenuVisibility, TimeoutSeconds, TimeoutStyle, default_entry_from_index,
    default_entry_from_title, well_known_flags,
};
pub use theme::{
    ColorPalette, FontConfig, FontScale, ResolvedTheme, RgbaColor, SystemColorScheme, ThemeMode,
    WindowConfig,
};
pub use validator::{RiskLevel, SafetyValidator, ValidationIssue};
