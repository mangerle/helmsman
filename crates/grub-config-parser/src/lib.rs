pub mod ast;
pub mod parser;

pub use ast::{ConfigLine, GrubConfigFile, QuoteType};
pub use parser::{ParseError, parse_grub_config, parse_line};
