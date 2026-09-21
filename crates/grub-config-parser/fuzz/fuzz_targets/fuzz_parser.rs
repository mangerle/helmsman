#![no_main]

use grub_config_parser::GrubConfig;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let config = GrubConfig::parse(s);
        let formatted = config.format();
        let _ = GrubConfig::parse(&formatted);
    }
});
