pub mod model;
pub mod parser;

pub use model::{BootEntry, MenuNode};
pub use parser::parse_grub_cfg;
