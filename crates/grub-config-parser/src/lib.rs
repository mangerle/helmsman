pub mod ast;
pub mod boot_defaults;
pub mod parser;

pub use ast::{ConfigLine, GrubConfigFile, QuoteType};
pub use boot_defaults::{
    BootKeyError, DefaultEntry, TimeoutSeconds, TimeoutStyle, default_entry_from_index,
    default_entry_from_title, get_default_entry, get_timeout, get_timeout_style, set_default_entry,
    set_timeout, set_timeout_style,
};
pub use parser::{ParseError, parse_grub_config, parse_line};
