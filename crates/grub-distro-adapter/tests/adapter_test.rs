use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};

const UBUNTU_OS_RELEASE: &str = r#"PRETTY_NAME="Ubuntu 24.04 LTS"
NAME="Ubuntu"
VERSION_ID="24.04"
VERSION="24.04 LTS (Noble Numbat)"
VERSION_CODENAME=noble
ID=ubuntu
ID_LIKE=debian
HOME_URL="https://www.ubuntu.com/"
SUPPORT_URL="https://help.ubuntu.com/"
BUG_REPORT_URL="https://bugs.launchpad.net/ubuntu/"
PRIVACY_POLICY_URL="https://www.ubuntu.com/legal/terms-and-policies/privacy-policy"
UBUNTU_CODENAME=noble
"#;

const ARCH_OS_RELEASE: &str = r#"NAME="Arch Linux"
PRETTY_NAME="Arch Linux"
ID=arch
BUILD_ID=rolling
ANSI_COLOR="38;2;23;147;209"
HOME_URL="https://archlinux.org/"
DOCUMENTATION_URL="https://wiki.archlinux.org/"
SUPPORT_URL="https://bbs.archlinux.org/"
BUG_REPORT_URL="https://gitlab.archlinux.org/groups/archlinux/-/issues"
PRIVACY_POLICY_URL="https://terms.archlinux.org/docs/privacy-policy/"
LOGO=archlinux-logo
"#;

const FEDORA_OS_RELEASE: &str = r#"NAME="Fedora Linux"
VERSION="40 (Workstation Edition)"
ID=fedora
VERSION_ID=40
VERSION_CODENAME=""
PLATFORM_ID="platform:f40"
PRETTY_NAME="Fedora Linux 40 (Workstation Edition)"
ANSI_COLOR="0;38;2;60;110;180"
LOGO=fedora-logo-icon
CPE_NAME="cpe:/o:fedoraproject:fedora:40"
DEFAULT_HOSTNAME="fedora"
HOME_URL="https://fedoraproject.org/"
DOCUMENTATION_URL="https://docs.fedoraproject.org/en-US/fedora/f40/system-administrators-guide/"
SUPPORT_URL="https://ask.fedoraproject.org/"
BUG_REPORT_URL="https://bugzilla.redhat.com/"
REDHAT_BUGZILLA_PRODUCT="Fedora"
REDHAT_BUGZILLA_PRODUCT_VERSION=40
REDHAT_SUPPORT_PRODUCT="Fedora"
REDHAT_SUPPORT_PRODUCT_VERSION=40
SUPPORT_END=2025-05-13
"#;

#[test]
fn test_ubuntu_adapter() {
    let profile = DistroProfile::parse_from_os_release(UBUNTU_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::DebianUbuntu);
    assert_eq!(profile.name, "Ubuntu 24.04 LTS");
    assert!(profile.update_command.ends_with("update-grub"));
    assert!(profile.update_command.starts_with('/'));
    assert!(profile.check_command.ends_with("grub-script-check"));
    assert!(profile.check_command.starts_with('/'));
    assert_eq!(profile.config_path, "/boot/grub/grub.cfg");
    assert_eq!(profile.grubenv_path, "/boot/grub/grubenv");
    assert!(profile.set_default_command.ends_with("grub-set-default"));
    assert!(profile.set_default_command.starts_with('/'));
    assert!(profile.command_args.is_empty());
}

#[test]
fn test_arch_adapter() {
    let profile = DistroProfile::parse_from_os_release(ARCH_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::Arch);
    assert_eq!(profile.name, "Arch Linux");
    assert!(profile.update_command.ends_with("grub-mkconfig"));
    assert!(profile.update_command.starts_with('/'));
    assert_eq!(profile.config_path, "/boot/grub/grub.cfg");
    assert_eq!(profile.grubenv_path, "/boot/grub/grubenv");
    assert!(profile.set_default_command.ends_with("grub-set-default"));
    assert_eq!(profile.command_args, vec!["-o", "/boot/grub/grub.cfg"]);
}

#[test]
fn test_fedora_uefi_adapter_prefers_boot_grub2() {
    let profile = DistroProfile::parse_from_os_release(FEDORA_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::FedoraRhel);
    assert!(
        profile.update_command.contains("grub2-mkconfig")
            || profile.update_command.contains("grub-mkconfig")
    );
    assert!(profile.update_command.starts_with('/'));
    // 现代 Fedora/RHEL（含 UEFI+BLS）主配置位于 /boot/grub2/grub.cfg
    assert_eq!(profile.config_path, "/boot/grub2/grub.cfg");
    assert_eq!(profile.grubenv_path, "/boot/grub2/grubenv");
    assert_eq!(profile.command_args, vec!["-o", "/boot/grub2/grub.cfg"]);
}

#[test]
fn test_fedora_bios_adapter() {
    let profile = DistroProfile::parse_from_os_release(FEDORA_OS_RELEASE, FirmwareType::Bios);
    assert_eq!(profile.family, DistroFamily::FedoraRhel);
    assert!(profile.update_command.starts_with('/'));
    assert_eq!(profile.config_path, "/boot/grub2/grub.cfg");
    assert_eq!(profile.command_args, vec!["-o", "/boot/grub2/grub.cfg"]);
}

#[test]
fn test_all_commands_are_absolute_paths() {
    for (os, fw) in [
        (UBUNTU_OS_RELEASE, FirmwareType::Uefi),
        (ARCH_OS_RELEASE, FirmwareType::Uefi),
        (FEDORA_OS_RELEASE, FirmwareType::Uefi),
        (FEDORA_OS_RELEASE, FirmwareType::Bios),
    ] {
        let profile = DistroProfile::parse_from_os_release(os, fw);
        for cmd in [
            profile.update_command.as_str(),
            profile.check_command.as_str(),
            profile.set_default_command.as_str(),
        ] {
            assert!(cmd.starts_with('/'), "命令必须是绝对路径: {cmd}");
        }
    }
}

const OPENSUSE_OS_RELEASE: &str = r#"NAME="openSUSE Tumbleweed"
ID="opensuse-tumbleweed"
ID_LIKE="opensuse suse"
VERSION_ID="20240101"
PRETTY_NAME="openSUSE Tumbleweed"
"#;

const MANJARO_OS_RELEASE: &str = r#"NAME="Manjaro Linux"
PRETTY_NAME="Manjaro Linux"
ID=manjaro
ID_LIKE=arch
BUILD_ID=rolling
"#;

const ROCKY_OS_RELEASE: &str = r#"NAME="Rocky Linux"
VERSION="9.3 (Blue Onyx)"
ID="rocky"
ID_LIKE="rhel centos fedora"
VERSION_ID="9.3"
PRETTY_NAME="Rocky Linux 9.3 (Blue Onyx)"
"#;

#[test]
fn test_opensuse_adapter() {
    let profile = DistroProfile::parse_from_os_release(OPENSUSE_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::OpenSuse);
    assert!(profile.update_command.starts_with('/'));
    assert!(
        profile.update_command.contains("grub2-mkconfig")
            || profile.update_command.contains("grub-mkconfig")
    );
    assert_eq!(profile.config_path, "/boot/grub2/grub.cfg");
    assert_eq!(profile.grubenv_path, "/boot/grub2/grubenv");
    assert_eq!(profile.package_manager_label(), "zypper");
    assert!(!profile.package_manager_lock_paths().is_empty());
}

#[test]
fn test_manjaro_adapter_maps_to_arch() {
    let profile = DistroProfile::parse_from_os_release(MANJARO_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::Arch);
    assert!(profile.update_command.contains("grub-mkconfig"));
    assert_eq!(profile.config_path, "/boot/grub/grub.cfg");
    assert_eq!(profile.package_manager_label(), "pacman");
}

#[test]
fn test_rocky_adapter_maps_to_fedora_rhel() {
    let profile = DistroProfile::parse_from_os_release(ROCKY_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::FedoraRhel);
    assert!(
        profile.update_command.contains("grub2-mkconfig")
            || profile.update_command.contains("grub-mkconfig")
    );
    assert_eq!(profile.config_path, "/boot/grub2/grub.cfg");
    assert_eq!(profile.package_manager_label(), "rpm/dnf");
}

#[test]
fn test_package_manager_lock_paths_by_family() {
    let ubuntu = DistroProfile::parse_from_os_release(UBUNTU_OS_RELEASE, FirmwareType::Uefi);
    assert!(
        ubuntu
            .package_manager_lock_paths()
            .contains(&"/var/lib/dpkg/lock-frontend")
    );

    let arch = DistroProfile::parse_from_os_release(ARCH_OS_RELEASE, FirmwareType::Uefi);
    assert!(
        arch.package_manager_lock_paths()
            .contains(&"/var/lib/pacman/db.lck")
    );

    let suse = DistroProfile::parse_from_os_release(OPENSUSE_OS_RELEASE, FirmwareType::Uefi);
    assert!(suse.package_manager_lock_paths().contains(&"/run/zypp.pid"));

    let fedora = DistroProfile::parse_from_os_release(FEDORA_OS_RELEASE, FirmwareType::Uefi);
    assert!(
        fedora
            .package_manager_lock_paths()
            .contains(&"/var/lib/dnf/metadata.lock")
    );
}
