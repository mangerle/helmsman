use crate::dbus_api::{DBUS_INTERFACE_V1, DBUS_OBJECT_PATH, DBUS_SERVICE_NAME, polkit_actions};
use crate::idle::IdleWatcher;
use crate::service::GrubService;
use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::dbus_api::HelmsmanDbusAdapter;

fn get_dbus_test_dir(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join("grub_dbus_test").join(name);
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let config_file = base.join("default_grub");
    let backup_dir = base.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();
    (config_file, backup_dir)
}

#[test]
fn test_dbus_constants_and_contract() {
    assert_eq!(DBUS_SERVICE_NAME, "org.freedesktop.Helmsman");
    assert_eq!(DBUS_OBJECT_PATH, "/org/freedesktop/Helmsman");
    assert_eq!(DBUS_INTERFACE_V1, "org.freedesktop.Helmsman.v1");
    assert_eq!(polkit_actions::ACTION_READ, "org.freedesktop.Helmsman.read");
    assert_eq!(
        polkit_actions::ACTION_SET_DEFAULT,
        "org.freedesktop.Helmsman.set-default"
    );
    assert_eq!(
        polkit_actions::ACTION_APPLY_CHANGES,
        "org.freedesktop.Helmsman.apply-changes"
    );
    assert_eq!(
        polkit_actions::ACTION_ROLLBACK,
        "org.freedesktop.Helmsman.rollback"
    );
    assert_eq!(
        polkit_actions::ACTION_SET_ALIAS,
        "org.freedesktop.Helmsman.set-alias"
    );
    assert_eq!(
        polkit_actions::ACTION_INSTALL_THEME,
        "org.freedesktop.Helmsman.install-theme"
    );
}

#[tokio::test]
async fn test_dbus_adapter_idle_watcher_integration() {
    let (config_file, backup_dir) = get_dbus_test_dir("idle_integration");
    fs::write(&config_file, "GRUB_DEFAULT=0\n").unwrap();

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Ubuntu Linux".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: "/usr/sbin/update-grub".to_string(),
        command_args: Vec::new(),
        check_command: "/usr/bin/grub-script-check".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "/usr/bin/grub-set-default".to_string(),
    };

    let service = Arc::new(GrubService::new_with_paths(
        config_file,
        backup_dir,
        distro_profile,
    ));
    let watcher = Arc::new(IdleWatcher::new());
    let adapter = HelmsmanDbusAdapter::with_idle_watcher(service, Arc::clone(&watcher));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    watcher.set_last_active_for_test(now - 120);
    assert!(watcher.is_idle_timeout(60));

    let preview = adapter.idle_watcher().is_idle_timeout(60);
    assert!(preview);

    watcher.set_last_active_for_test(now);
    assert!(!watcher.is_idle_timeout(60));
    assert!(Arc::ptr_eq(adapter.idle_watcher(), &watcher));
}
