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
    /// 更新引导所用的系统主命令（例如："update-grub"、"grub-mkconfig"）
    pub update_command: String,
    /// 命令参数列表
    pub command_args: Vec<String>,
    /// 引导脚本语法检查命令（例如："grub-script-check"、"grub2-script-check"）
    pub check_command: String,
    /// 语法检查命令参数列表
    pub check_command_args: Vec<String>,
    /// grubenv 环境变量文件绝对路径（例如："/boot/grub/grubenv"）
    pub grubenv_path: String,
    /// 快速修改默认启动项的系统命令（例如："grub-set-default"、"grub2-set-default"）
    pub set_default_command: String,
}

impl DistroProfile {
    /// 基于 os-release 文本内容与固件类型进行确定性分析（便于单元测试与静态分析）
    ///
    /// # 设计原理
    /// - **实现初衷**：Linux 各发行版及其衍生版（如 Linux Mint、Pop!_OS）在 GRUB 命令与路径上存在差异。
    ///   通过标准化解析 `/etc/os-release`，可在不硬编码发行版名称的前提下实现家族化归类。
    /// - **核心优势**：纯纯函数解析，不依赖系统宿主调用，测试可重复性达到 100%。
    /// - **代价与局限**：若某些极小众发行版未遵循 `ID` 与 `ID_LIKE` 规范，将回退至 `Generic` 通用模式。
    pub fn parse_from_os_release(os_release: &str, firmware: FirmwareType) -> Self {
        let (id, id_like, pretty_name) = parse_os_release_vars(os_release);
        let name = if !pretty_name.is_empty() {
            pretty_name
        } else if !id.is_empty() {
            id.clone()
        } else {
            "Generic Linux".to_string()
        };

        let family = resolve_distro_family(&id, &id_like);
        build_profile_for_family(family, name, firmware)
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

        Self::parse_from_os_release(&os_release_content, firmware)
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
fn build_profile_for_family(
    family: DistroFamily,
    name: String,
    firmware: FirmwareType,
) -> DistroProfile {
    match family {
        DistroFamily::DebianUbuntu => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: "update-grub".to_string(),
            command_args: Vec::new(),
            check_command: "grub-script-check".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: "grub-set-default".to_string(),
        },
        DistroFamily::Arch => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: "grub-mkconfig".to_string(),
            command_args: vec!["-o".to_string(), "/boot/grub/grub.cfg".to_string()],
            check_command: "grub-script-check".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: "grub-set-default".to_string(),
        },
        DistroFamily::FedoraRhel => {
            let (config_path, grubenv_path) = match firmware {
                FirmwareType::Uefi => (
                    "/boot/efi/EFI/fedora/grub.cfg".to_string(),
                    "/boot/efi/EFI/fedora/grubenv".to_string(),
                ),
                FirmwareType::Bios => (
                    "/boot/grub2/grub.cfg".to_string(),
                    "/boot/grub2/grubenv".to_string(),
                ),
            };
            DistroProfile {
                family,
                name,
                firmware,
                config_path: config_path.clone(),
                update_command: "grub2-mkconfig".to_string(),
                command_args: vec!["-o".to_string(), config_path],
                check_command: "grub2-script-check".to_string(),
                check_command_args: Vec::new(),
                grubenv_path,
                set_default_command: "grub2-set-default".to_string(),
            }
        }
        DistroFamily::OpenSuse => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub2/grub.cfg".to_string(),
            update_command: "grub2-mkconfig".to_string(),
            command_args: vec!["-o".to_string(), "/boot/grub2/grub.cfg".to_string()],
            check_command: "grub2-script-check".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub2/grubenv".to_string(),
            set_default_command: "grub2-set-default".to_string(),
        },
        DistroFamily::Generic => DistroProfile {
            family,
            name,
            firmware,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: "grub-mkconfig".to_string(),
            command_args: vec!["-o".to_string(), "/boot/grub/grub.cfg".to_string()],
            check_command: "grub-script-check".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: "grub-set-default".to_string(),
        },
    }
}
