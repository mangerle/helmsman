use grub_config_parser::parse_grub_config;

const SAMPLE_CONFIG: &str = r#"# If you change this file, run 'update-grub' afterwards to update
# /boot/grub/grub.cfg.
# For full documentation of the options in this file, see:
#   info -f grub -n 'Simple configuration'

GRUB_DEFAULT=0
GRUB_TIMEOUT_STYLE=hidden
GRUB_TIMEOUT=0
GRUB_DISTRIBUTOR=`lsb_release -i -s 2> /dev/null || echo Debian`
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash"
GRUB_CMDLINE_LINUX=""

# Uncomment to enable BadRAM filtering, modify to suit your needs
# This works with Linux-LIBRE as well
#GRUB_BADRAM="0x01234567,0xfedcba98"

# Uncomment to disable graphical terminal (grub-pc only)
#GRUB_TERMINAL=console
"#;

#[test]
fn test_roundtrip_preservation() {
    // 验证解析后再序列化能够百分之百还原原始内容（包含注释与空行）
    let config = parse_grub_config(SAMPLE_CONFIG);
    let serialized = config.serialize();
    assert_eq!(SAMPLE_CONFIG, serialized);
}

#[test]
fn test_get_values() {
    let config = parse_grub_config(SAMPLE_CONFIG);

    assert_eq!(config.get("GRUB_DEFAULT"), Some("0"));
    assert_eq!(config.get("GRUB_TIMEOUT_STYLE"), Some("hidden"));
    assert_eq!(config.get("GRUB_TIMEOUT"), Some("0"));
    assert_eq!(
        config.get("GRUB_CMDLINE_LINUX_DEFAULT"),
        Some("quiet splash")
    );
    assert_eq!(config.get("GRUB_CMDLINE_LINUX"), Some(""));
    assert_eq!(config.get("NON_EXISTENT"), None);
}

#[test]
fn test_update_existing_value_in_place() {
    let mut config = parse_grub_config(SAMPLE_CONFIG);

    // 修改 GRUB_TIMEOUT 为 10
    config.set("GRUB_TIMEOUT", "10");
    assert_eq!(config.get("GRUB_TIMEOUT"), Some("10"));

    // 序列化后应保持原有位置不变，其余行不受影响
    let serialized = config.serialize();
    assert!(serialized.contains("GRUB_TIMEOUT=10"));
    assert!(
        serialized.contains("# If you change this file, run 'update-grub' afterwards to update")
    );
    assert!(serialized.contains("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\""));
}

#[test]
fn test_add_new_entry() {
    let mut config = parse_grub_config(SAMPLE_CONFIG);

    // 添加全新配置项
    config.set("GRUB_DISABLE_OS_PROBER", "false");
    assert_eq!(config.get("GRUB_DISABLE_OS_PROBER"), Some("false"));

    let serialized = config.serialize();
    assert!(serialized.contains("GRUB_DISABLE_OS_PROBER=\"false\""));
}

#[test]
fn test_remove_entry() {
    let mut config = parse_grub_config(SAMPLE_CONFIG);

    let removed = config.remove("GRUB_TIMEOUT");
    assert!(removed);
    assert_eq!(config.get("GRUB_TIMEOUT"), None);

    let serialized = config.serialize();
    assert!(!serialized.contains("GRUB_TIMEOUT=0"));
}
