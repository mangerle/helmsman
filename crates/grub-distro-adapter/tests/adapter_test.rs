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
    assert_eq!(profile.update_command, "update-grub");
    assert_eq!(profile.check_command, "grub-script-check");
    assert_eq!(profile.config_path, "/boot/grub/grub.cfg");
    assert!(profile.command_args.is_empty());
}

#[test]
fn test_arch_adapter() {
    let profile = DistroProfile::parse_from_os_release(ARCH_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::Arch);
    assert_eq!(profile.name, "Arch Linux");
    assert_eq!(profile.update_command, "grub-mkconfig");
    assert_eq!(profile.config_path, "/boot/grub/grub.cfg");
    assert_eq!(profile.command_args, vec!["-o", "/boot/grub/grub.cfg"]);
}

#[test]
fn test_fedora_uefi_adapter() {
    let profile = DistroProfile::parse_from_os_release(FEDORA_OS_RELEASE, FirmwareType::Uefi);
    assert_eq!(profile.family, DistroFamily::FedoraRhel);
    assert_eq!(profile.update_command, "grub2-mkconfig");
    assert_eq!(profile.config_path, "/boot/efi/EFI/fedora/grub.cfg");
    assert_eq!(
        profile.command_args,
        vec!["-o", "/boot/efi/EFI/fedora/grub.cfg"]
    );
}

#[test]
fn test_fedora_bios_adapter() {
    let profile = DistroProfile::parse_from_os_release(FEDORA_OS_RELEASE, FirmwareType::Bios);
    assert_eq!(profile.family, DistroFamily::FedoraRhel);
    assert_eq!(profile.update_command, "grub2-mkconfig");
    assert_eq!(profile.config_path, "/boot/grub2/grub.cfg");
    assert_eq!(profile.command_args, vec!["-o", "/boot/grub2/grub.cfg"]);
}
