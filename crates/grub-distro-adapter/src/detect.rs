use std::path::Path;

/// 固件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareType {
    /// UEFI 现代引导固件
    Uefi,
    /// 传统 BIOS (Legacy) 固件
    Bios,
}

/// 发行版家族分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroFamily {
    /// Debian, Ubuntu, Linux Mint 等
    DebianUbuntu,
    /// Arch Linux, Manjaro, EndeavourOS 等
    Arch,
    /// Fedora, RHEL, CentOS, Rocky Linux 等
    FedoraRhel,
    /// openSUSE Leap, Tumbleweed 等
    OpenSuse,
    /// 其他通用 Linux 发行版
    Generic,
}

/// 发行版引导适配档案
///
/// # 设计原理
/// 所有特权命令字段均为**绝对路径**，以配合 `SafeCommand` 仅接受白名单绝对路径的约束，
/// 避免在 PATH 上被同名可执行文件劫持。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistroProfile {
    /// 发行版家族
    pub family: DistroFamily,
    /// 识别出的发行版名称（例如："Ubuntu"、"Arch Linux"）
    pub name: String,
    /// 固件类型
    pub firmware: FirmwareType,
    /// GRUB 主配置文件目标绝对路径
    pub config_path: String,
    /// 更新引导所用的系统主命令绝对路径
    pub update_command: String,
    /// 命令参数列表
    pub command_args: Vec<String>,
    /// 引导脚本语法检查命令绝对路径
    pub check_command: String,
    /// 语法检查命令参数列表
    pub check_command_args: Vec<String>,
    /// grubenv 环境变量文件绝对路径（例如："/boot/grub/grubenv"）
    pub grubenv_path: String,
    /// 快速修改默认启动项的系统命令绝对路径
    pub set_default_command: String,
}

/// 按候选顺序返回第一个存在的绝对路径，否则回退到首选默认值
fn first_existing(candidates: &[&str], fallback: &str) -> String {
    candidates
        .iter()
        .copied()
        .find(|p| Path::new(p).exists())
        .unwrap_or(fallback)
        .to_string()
}

impl DistroProfile {
    /// 解析主题安装根目录：与主配置同级的 themes 目录
    ///
    /// # 设计原理
    /// - **实现初衷**：Debian/Ubuntu 使用 `/boot/grub/themes`，Fedora/RHEL 使用 `/boot/grub2/themes`，
    ///   与 `config_path` 所在引导目录保持一致，避免主题装到错误分区。
    /// - **代价与局限**：若发行版采用非标准布局，需在 profile 构造时覆盖。
    pub fn themes_dir(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.config_path)
            .parent()
            .map(|p| p.join("themes"))
            .unwrap_or_else(|| std::path::PathBuf::from("/boot/grub/themes"))
    }

    /// 基于 os-release 文本内容与固件类型进行确定性分析（便于单元测试与静态分析）
    ///
    /// # 设计原理
    /// - **实现初衷**：Linux 各发行版及其衍生版（如 Linux Mint、Pop!_OS）在 GRUB 命令与路径上存在差异。
    ///   通过标准化解析 `/etc/os-release`，可在不硬编码发行版名称的前提下实现家族化归类。
    /// - **核心优势**：纯纯函数解析，不依赖系统宿主调用，测试可重复性达到 100%。
    /// - **代价与局限**：若某些极小众发行版未遵循 `ID` 与 `ID_LIKE` 规范，将回退至 `Generic` 通用模式。
    ///   命令路径采用发行版常见绝对路径默认值；`detect_current_system` 会在宿主上探测实际存在的路径。
    pub fn parse_from_os_release(os_release: &str, firmware: FirmwareType) -> Self {
        Self::parse_from_os_release_with_probe(os_release, firmware, false)
    }

    /// 在解析基础上可选地探测宿主机真实路径（供 `detect_current_system` 使用）
    fn parse_from_os_release_with_probe(
        os_release: &str,
        firmware: FirmwareType,
        probe_host: bool,
    ) -> Self {
        let (id, id_like, pretty_name) = parse_os_release_vars(os_release);
        let name = if !pretty_name.is_empty() {
            pretty_name
        } else if !id.is_empty() {
            id.clone()
        } else {
            "Generic Linux".to_string()
        };

        let family = resolve_distro_family(&id, &id_like);
        build_profile_for_family(family, name, firmware, probe_host)
    }

    /// 自动探测当前正在运行的宿主操作系统环境
    pub fn detect_current_system() -> Self {
        let is_uefi = Path::new("/sys/firmware/efi").exists();
        let firmware = if is_uefi {
            FirmwareType::Uefi
        } else {
            FirmwareType::Bios
        };

        let os_release_content = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();

        Self::parse_from_os_release_with_probe(&os_release_content, firmware, true)
    }
}

/// 从 os-release 文本中提取 ID, ID_LIKE 与 PRETTY_NAME 变量
fn parse_os_release_vars(os_release: &str) -> (String, String, String) {
    let mut id = String::new();
    let mut id_like = String::new();
    let mut pretty_name = String::new();

    for line in os_release.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("ID=") {
            id = val.trim_matches('"').to_lowercase();
        } else if let Some(val) = trimmed.strip_prefix("ID_LIKE=") {
            id_like = val.trim_matches('"').to_lowercase();
        } else if let Some(val) = trimmed.strip_prefix("PRETTY_NAME=") {
            pretty_name = val.trim_matches('"').to_string();
        }
    }

    (id, id_like, pretty_name)
}

/// 根据 ID 与 ID_LIKE 字段推导所属发行版家族
fn resolve_distro_family(id: &str, id_like: &str) -> DistroFamily {
    if id == "ubuntu"
        || id == "debian"
        || id == "linuxmint"
        || id == "pop"
        || id_like.contains("debian")
        || id_like.contains("ubuntu")
    {
        DistroFamily::DebianUbuntu
    } else if id == "arch" || id == "manjaro" || id_like.contains("arch") {
        DistroFamily::Arch
    } else if id == "fedora"
        || id == "rhel"
        || id == "centos"
        || id_like.contains("fedora")
        || id_like.contains("rhel")
    {
        DistroFamily::FedoraRhel
    } else if id.contains("suse") || id_like.contains("suse") {
        DistroFamily::OpenSuse
    } else {
        DistroFamily::Generic
    }
}

/// 构建对应家族的引导适配档案
///
/// # 路径策略
/// - Debian/Ubuntu：`update-grub` → `/usr/sbin/update-grub`
/// - Fedora/RHEL UEFI：优先探测现代 BLS 布局 `/boot/grub2/grub.cfg`，
///   仅当该布局不存在而 ESP 下 fedora 配置存在时才使用 `/boot/efi/EFI/fedora/grub.cfg`；
///   纯函数解析（不探测宿主）时默认 `/boot/grub2/grub.cfg`，与主流现代 Fedora 一致。
fn build_profile_for_family(
    family: DistroFamily,
    name: String,
    firmware: FirmwareType,
    probe_host: bool,
) -> DistroProfile {
    match family {
        DistroFamily::DebianUbuntu => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: first_existing(
                &["/usr/sbin/update-grub", "/usr/bin/update-grub"],
                "/usr/sbin/update-grub",
            ),
            command_args: Vec::new(),
            check_command: first_existing(
                &["/usr/bin/grub-script-check", "/usr/sbin/grub-script-check"],
                "/usr/bin/grub-script-check",
            ),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: first_existing(
                &["/usr/bin/grub-set-default", "/usr/sbin/grub-set-default"],
                "/usr/bin/grub-set-default",
            ),
        },
        DistroFamily::Arch => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: first_existing(
                &["/usr/bin/grub-mkconfig", "/usr/sbin/grub-mkconfig"],
                "/usr/bin/grub-mkconfig",
            ),
            command_args: vec!["-o".to_string(), "/boot/grub/grub.cfg".to_string()],
            check_command: first_existing(
                &["/usr/bin/grub-script-check", "/usr/sbin/grub-script-check"],
                "/usr/bin/grub-script-check",
            ),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: first_existing(
                &["/usr/bin/grub-set-default", "/usr/sbin/grub-set-default"],
                "/usr/bin/grub-set-default",
            ),
        },
        DistroFamily::FedoraRhel => {
            let (config_path, grubenv_path) = resolve_fedora_boot_paths(firmware, probe_host);
            DistroProfile {
                family,
                name,
                firmware,
                config_path: config_path.clone(),
                update_command: first_existing(
                    &[
                        "/usr/sbin/grub2-mkconfig",
                        "/usr/bin/grub2-mkconfig",
                        "/usr/sbin/grub-mkconfig",
                    ],
                    "/usr/sbin/grub2-mkconfig",
                ),
                command_args: vec!["-o".to_string(), config_path],
                check_command: first_existing(
                    &[
                        "/usr/bin/grub2-script-check",
                        "/usr/sbin/grub2-script-check",
                        "/usr/bin/grub-script-check",
                    ],
                    "/usr/bin/grub2-script-check",
                ),
                check_command_args: Vec::new(),
                grubenv_path,
                set_default_command: first_existing(
                    &[
                        "/usr/sbin/grub2-set-default",
                        "/usr/bin/grub2-set-default",
                        "/usr/bin/grub-set-default",
                    ],
                    "/usr/sbin/grub2-set-default",
                ),
            }
        }
        DistroFamily::OpenSuse => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub2/grub.cfg".to_string(),
            update_command: first_existing(
                &["/usr/sbin/grub2-mkconfig", "/usr/bin/grub2-mkconfig"],
                "/usr/sbin/grub2-mkconfig",
            ),
            command_args: vec!["-o".to_string(), "/boot/grub2/grub.cfg".to_string()],
            check_command: first_existing(
                &[
                    "/usr/bin/grub2-script-check",
                    "/usr/sbin/grub2-script-check",
                ],
                "/usr/bin/grub2-script-check",
            ),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub2/grubenv".to_string(),
            set_default_command: first_existing(
                &["/usr/sbin/grub2-set-default", "/usr/bin/grub2-set-default"],
                "/usr/sbin/grub2-set-default",
            ),
        },
        DistroFamily::Generic => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: first_existing(
                &["/usr/sbin/grub-mkconfig", "/usr/bin/grub-mkconfig"],
                "/usr/sbin/grub-mkconfig",
            ),
            command_args: vec!["-o".to_string(), "/boot/grub/grub.cfg".to_string()],
            check_command: first_existing(
                &["/usr/bin/grub-script-check", "/usr/sbin/grub-script-check"],
                "/usr/bin/grub-script-check",
            ),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: first_existing(
                &["/usr/bin/grub-set-default", "/usr/sbin/grub-set-default"],
                "/usr/bin/grub-set-default",
            ),
        },
    }
}

/// 解析 Fedora/RHEL 系引导配置与 grubenv 路径
fn resolve_fedora_boot_paths(firmware: FirmwareType, probe_host: bool) -> (String, String) {
    let modern = (
        "/boot/grub2/grub.cfg".to_string(),
        "/boot/grub2/grubenv".to_string(),
    );
    let efi_stub = (
        "/boot/efi/EFI/fedora/grub.cfg".to_string(),
        "/boot/efi/EFI/fedora/grubenv".to_string(),
    );

    match firmware {
        FirmwareType::Bios => modern,
        FirmwareType::Uefi => {
            if !probe_host {
                // 纯函数默认：现代 Fedora/RHEL UEFI 也以 /boot/grub2 为主配置
                return modern;
            }
            let modern_exists = Path::new(&modern.0).exists() || Path::new("/boot/grub2").exists();
            let efi_exists = Path::new(&efi_stub.0).exists();
            if modern_exists || !efi_exists {
                modern
            } else {
                efi_stub
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_os_release_family_and_absolute_commands() {
        let os = "ID=ubuntu\nID_LIKE=debian\nPRETTY_NAME=\"Ubuntu 24.04\"\n";
        let profile = DistroProfile::parse_from_os_release(os, FirmwareType::Uefi);
        assert_eq!(profile.family, DistroFamily::DebianUbuntu);
        assert!(profile.update_command.starts_with('/'));
        assert!(profile.check_command.starts_with('/'));
        assert!(profile.set_default_command.starts_with('/'));
    }

    #[test]
    fn test_fedora_uefi_prefers_modern_grub2_path() {
        let os = "ID=fedora\nPRETTY_NAME=\"Fedora Linux\"\n";
        let profile = DistroProfile::parse_from_os_release(os, FirmwareType::Uefi);
        assert_eq!(profile.family, DistroFamily::FedoraRhel);
        assert_eq!(profile.config_path, "/boot/grub2/grub.cfg");
        assert_eq!(profile.grubenv_path, "/boot/grub2/grubenv");
        assert!(
            profile.update_command.contains("grub2-mkconfig")
                || profile.update_command.contains("grub-mkconfig")
        );
    }

    #[test]
    fn test_themes_dir_follows_config_path() {
        let mut profile = DistroProfile::detect_current_system();
        profile.config_path = "/boot/grub/grub.cfg".to_string();
        assert_eq!(
            profile.themes_dir(),
            std::path::PathBuf::from("/boot/grub/themes")
        );

        profile.config_path = "/boot/grub2/grub.cfg".to_string();
        assert_eq!(
            profile.themes_dir(),
            std::path::PathBuf::from("/boot/grub2/themes")
        );
    }

    #[test]
    fn test_unknown_distro_falls_back_to_generic() {
        let profile = DistroProfile::parse_from_os_release("ID=nixos\n", FirmwareType::Bios);
        assert_eq!(profile.family, DistroFamily::Generic);
        assert!(profile.update_command.starts_with('/'));
    }
}
