use crate::audit::{AuditAction, with_audit};
use crate::executor::SafeCommand;
use crate::service::{DaemonError, GrubService};
use grub_distro_adapter::DistroProfile;
use helmsman_client::{BlockDeviceDto, SystemFontDto};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::info;

/// 允许转换的字体文件扩展名白名单
const ALLOWED_FONT_EXTS: &[&str] = &["ttf", "otf", "ttc"];
/// `grub-install` 执行超时上限（秒）
const GRUB_INSTALL_TIMEOUT_SECS: u64 = 120;

/// 字体转换参数对象
#[derive(Debug, Clone)]
pub struct ConvertFontParams {
    /// 源字体文件绝对路径
    pub src_path: String,
    /// 目标字号（像素，约束在 8..=72）
    pub font_size: u16,
    /// 输出文件名（不含扩展名，自动输出至受控字体目录）
    pub output_name: String,
}

/// GRUB 引导程序重装模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrubInstallMode {
    /// UEFI 模式（写入 ESP 分区）
    Uefi,
    /// 传统 BIOS / MBR 模式（写入磁盘主引导记录）
    Bios,
}

impl GrubInstallMode {
    /// 从前端契约字符串解析安装模式
    ///
    /// # Errors
    /// 当模式不是 `uefi` 或 `bios` 时返回 [`DaemonError::SystemToolFailed`]。
    pub fn parse(raw: &str) -> Result<Self, DaemonError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "uefi" => Ok(Self::Uefi),
            "bios" => Ok(Self::Bios),
            other => Err(DaemonError::SystemToolFailed {
                operation: "解析引导安装模式".to_string(),
                reason: format!("不支持的模式 '{other}'，仅允许 uefi 或 bios"),
            }),
        }
    }
}

/// GRUB 重装引导参数对象
#[derive(Debug, Clone)]
pub struct ReinstallGrubParams {
    /// 安装模式（UEFI / BIOS）
    pub mode: GrubInstallMode,
    /// 目标磁盘或设备路径（如 `/dev/sda`）
    pub device_path: String,
    /// UEFI 模式下的 EFI 系统分区挂载目录（如 `/boot/efi`）
    pub efi_directory: String,
}

#[derive(Debug, Deserialize)]
struct LsblkRoot {
    #[serde(default)]
    blockdevices: Vec<LsblkNode>,
}

#[derive(Debug, Deserialize)]
struct LsblkNode {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default, rename = "type")]
    device_type: String,
    #[serde(default)]
    mountpoint: Option<String>,
    #[serde(default)]
    fstype: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    children: Vec<LsblkNode>,
}

/// 查找首个存在的系统命令绝对路径
fn find_first_existing(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|p| Path::new(p).exists())
}

/// 校验并规范化输出字体文件名（防路径穿越）
fn sanitize_font_output_name(raw: &str) -> Result<String, DaemonError> {
    let trimmed = raw.trim();
    let stem = trimmed.strip_suffix(".pf2").unwrap_or(trimmed);
    if stem.is_empty()
        || stem.len() > 64
        || !stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(DaemonError::SystemToolFailed {
            operation: "校验输出字体名称".to_string(),
            reason: "输出字体名仅允许 1~64 位字母、数字、连字符或下划线".to_string(),
        });
    }
    Ok(format!("{stem}.pf2"))
}

/// 校验源字体文件路径合法性（必须存在、为常规文件且扩展名为 ttf/otf/ttc）
fn validate_source_font_path(raw_path: &str) -> Result<PathBuf, DaemonError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(DaemonError::SystemToolFailed {
            operation: "校验源字体路径".to_string(),
            reason: "源字体路径不能为空".to_string(),
        });
    }
    let canon = fs::canonicalize(trimmed).map_err(|e| DaemonError::SystemToolFailed {
        operation: "校验源字体路径".to_string(),
        reason: format!("无法访问字体文件 '{trimmed}': {e}"),
    })?;
    if !canon.is_file() {
        return Err(DaemonError::SystemToolFailed {
            operation: "校验源字体路径".to_string(),
            reason: format!("'{}' 不是常规文件", canon.display()),
        });
    }
    let ext = canon
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !ALLOWED_FONT_EXTS.contains(&ext.as_str()) {
        return Err(DaemonError::SystemToolFailed {
            operation: "校验源字体格式".to_string(),
            reason: format!("仅支持 .ttf / .otf / .ttc 字体文件，当前为 '.{ext}'"),
        });
    }
    Ok(canon)
}

fn flatten_lsblk_nodes(nodes: &[LsblkNode], out: &mut Vec<BlockDeviceDto>) {
    for node in nodes {
        if node.device_type == "disk" || node.device_type == "part" {
            let path = node
                .path
                .clone()
                .unwrap_or_else(|| format!("/dev/{}", node.name));
            out.push(BlockDeviceDto {
                name: node.name.clone(),
                path,
                size: node.size.clone().unwrap_or_default(),
                device_type: node.device_type.clone(),
                mountpoint: node.mountpoint.clone().unwrap_or_default(),
                fstype: node.fstype.clone().unwrap_or_default(),
                model: node.model.as_deref().unwrap_or("").trim().to_string(),
            });
        }
        flatten_lsblk_nodes(&node.children, out);
    }
}

impl GrubService {
    /// 列出宿主机可用的系统字体（调用 `fc-list`）
    ///
    /// # Errors
    /// 当未安装 fontconfig 或命令执行失败时返回 [`DaemonError::SystemToolFailed`]。
    pub fn list_system_fonts(&self) -> Result<Vec<SystemFontDto>, DaemonError> {
        let fc_list =
            find_first_existing(&["/usr/bin/fc-list", "/bin/fc-list"]).ok_or_else(|| {
                DaemonError::SystemToolFailed {
                    operation: "枚举系统字体".to_string(),
                    reason: "未找到 /usr/bin/fc-list，请确认已安装 fontconfig".to_string(),
                }
            })?;

        let cmd =
            SafeCommand::new(fc_list)?.args(["--format=%{family[0]}|%{style[0]}|%{file}\n"])?;
        let output = cmd
            .output_with_timeout(Duration::from_secs(15))
            .map_err(|e| DaemonError::SystemToolFailed {
                operation: "枚举系统字体".to_string(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(DaemonError::SystemToolFailed {
                operation: "枚举系统字体".to_string(),
                reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut seen = HashSet::with_capacity(128);
        let mut fonts = Vec::with_capacity(128);

        for line in stdout.lines() {
            let mut parts = line.splitn(3, '|');
            let family = parts.next().unwrap_or("").trim();
            let style = parts.next().unwrap_or("").trim();
            let file_path = parts.next().unwrap_or("").trim();
            if family.is_empty() || file_path.is_empty() {
                continue;
            }
            let lower = file_path.to_ascii_lowercase();
            if !(lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")) {
                continue;
            }
            if seen.insert(file_path.to_string()) {
                fonts.push(SystemFontDto {
                    family: family.to_string(),
                    style: style.to_string(),
                    file_path: file_path.to_string(),
                });
            }
        }

        fonts.sort_by(|a, b| a.family.cmp(&b.family).then(a.style.cmp(&b.style)));
        fonts.truncate(200);
        Ok(fonts)
    }

    /// 调用 `grub-mkfont` 将系统字体转换为 GRUB `.pf2` 字体文件
    ///
    /// # Errors
    /// 当参数非法、工具缺失或转换失败时返回 [`DaemonError::SystemToolFailed`]。
    pub fn convert_grub_font_as(
        &self,
        params: &ConvertFontParams,
        caller_uid: u32,
    ) -> Result<String, DaemonError> {
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::ConvertFont,
            format!("转换字体 {} ({}px)", params.output_name, params.font_size),
            None,
            None,
            |_| (true, None),
            || {
                if !(8..=72).contains(&params.font_size) {
                    return Err(DaemonError::SystemToolFailed {
                        operation: "校验字体字号".to_string(),
                        reason: "字号必须位于 8 到 72 像素之间".to_string(),
                    });
                }
                let src_canon = validate_source_font_path(&params.src_path)?;
                let out_name = sanitize_font_output_name(&params.output_name)?;

                let mkfont = find_first_existing(&[
                    "/usr/bin/grub-mkfont",
                    "/usr/sbin/grub-mkfont",
                    "/usr/bin/grub2-mkfont",
                    "/usr/sbin/grub2-mkfont",
                ])
                .ok_or_else(|| DaemonError::SystemToolFailed {
                    operation: "转换 GRUB 字体".to_string(),
                    reason: "未找到 grub-mkfont / grub2-mkfont 工具".to_string(),
                })?;

                let fonts_dir = self.effective_root().join("boot/grub/fonts");
                fs::create_dir_all(&fonts_dir).map_err(|e| DaemonError::SystemToolFailed {
                    operation: "创建字体输出目录".to_string(),
                    reason: format!("无法创建 {}: {e}", fonts_dir.display()),
                })?;
                let out_path = fonts_dir.join(out_name);

                let size_str = params.font_size.to_string();
                let out_str = out_path.to_string_lossy().into_owned();
                let src_str = src_canon.to_string_lossy().into_owned();
                let cmd =
                    SafeCommand::new(mkfont)?.args(["-s", &size_str, "-o", &out_str, &src_str])?;
                let output = cmd
                    .output_with_timeout(Duration::from_secs(30))
                    .map_err(|e| DaemonError::SystemToolFailed {
                        operation: "转换 GRUB 字体".to_string(),
                        reason: e.to_string(),
                    })?;

                if !output.status.success() {
                    return Err(DaemonError::SystemToolFailed {
                        operation: "转换 GRUB 字体".to_string(),
                        reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                    });
                }

                info!("已转换生成 GRUB 字体文件: {}", out_str);
                Ok(out_str)
            },
        )
    }

    /// 列出系统块设备与分区（调用 `lsblk -J`）
    ///
    /// # Errors
    /// 当 `lsblk` 缺失或解析输出失败时返回 [`DaemonError::SystemToolFailed`]。
    pub fn list_block_devices(&self) -> Result<Vec<BlockDeviceDto>, DaemonError> {
        let lsblk = find_first_existing(&["/usr/bin/lsblk", "/bin/lsblk"]).ok_or_else(|| {
            DaemonError::SystemToolFailed {
                operation: "扫描块设备".to_string(),
                reason: "未找到 /usr/bin/lsblk".to_string(),
            }
        })?;

        let cmd = SafeCommand::new(lsblk)?.args([
            "-J",
            "-o",
            "NAME,PATH,SIZE,TYPE,MOUNTPOINT,FSTYPE,MODEL",
        ])?;
        let output = cmd
            .output_with_timeout(Duration::from_secs(10))
            .map_err(|e| DaemonError::SystemToolFailed {
                operation: "扫描块设备".to_string(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(DaemonError::SystemToolFailed {
                operation: "扫描块设备".to_string(),
                reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let root: LsblkRoot =
            serde_json::from_slice(&output.stdout).map_err(|e| DaemonError::SystemToolFailed {
                operation: "解析 lsblk 输出".to_string(),
                reason: e.to_string(),
            })?;
        let mut devices = Vec::with_capacity(16);
        flatten_lsblk_nodes(&root.blockdevices, &mut devices);
        Ok(devices)
    }

    /// 重装 GRUB 引导程序至指定 MBR 磁盘或 UEFI ESP 分区
    ///
    /// # Errors
    /// 当设备路径/参数非法或 `grub-install` 返回非零状态时返回 [`DaemonError::SystemToolFailed`]。
    pub fn reinstall_grub_as(
        &self,
        params: &ReinstallGrubParams,
        caller_uid: u32,
    ) -> Result<String, DaemonError> {
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::ReinstallGrub,
            format!("重装 GRUB ({:?}) -> {}", params.mode, params.device_path),
            None,
            None,
            |_| (true, None),
            || {
                let installer = find_first_existing(&[
                    "/usr/sbin/grub-install",
                    "/usr/bin/grub-install",
                    "/sbin/grub-install",
                    "/usr/sbin/grub2-install",
                    "/usr/bin/grub2-install",
                ])
                .ok_or_else(|| DaemonError::SystemToolFailed {
                    operation: "重装 GRUB 引导".to_string(),
                    reason: "未找到 grub-install / grub2-install 工具".to_string(),
                })?;

                let mut args: Vec<String> = Vec::new();
                let root = self.effective_root();
                if root != Path::new("/") {
                    args.push(format!("--boot-directory={}", root.join("boot").display()));
                }

                match params.mode {
                    GrubInstallMode::Uefi => {
                        let efi_dir = params.efi_directory.trim();
                        if efi_dir.is_empty() || !Path::new(efi_dir).is_dir() {
                            return Err(DaemonError::SystemToolFailed {
                                operation: "校验 EFI 目录".to_string(),
                                reason: format!("EFI 挂载目录不存在或不是有效目录: '{efi_dir}'"),
                            });
                        }
                        args.push(format!("--efi-directory={efi_dir}"));
                        args.push("--recheck".to_string());
                    }
                    GrubInstallMode::Bios => {
                        let dev = params.device_path.trim();
                        if !dev.starts_with("/dev/") || !Path::new(dev).exists() {
                            return Err(DaemonError::SystemToolFailed {
                                operation: "校验目标磁盘".to_string(),
                                reason: format!(
                                    "BIOS 模式必须指定合法的 /dev/ 磁盘设备，当前为 '{dev}'"
                                ),
                            });
                        }
                        args.push("--recheck".to_string());
                        args.push(dev.to_string());
                    }
                }

                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let cmd = SafeCommand::new(installer)?.args(&arg_refs)?;
                let output = cmd
                    .output_with_timeout(Duration::from_secs(GRUB_INSTALL_TIMEOUT_SECS))
                    .map_err(|e| DaemonError::SystemToolFailed {
                        operation: "执行 grub-install".to_string(),
                        reason: e.to_string(),
                    })?;

                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if !output.status.success() {
                    return Err(DaemonError::SystemToolFailed {
                        operation: "执行 grub-install".to_string(),
                        reason: format!("退出码 {:?}: {stderr}", output.status.code()),
                    });
                }

                info!("GRUB 引导程序重装完成 ({:?})", params.mode);
                Ok(if stdout.is_empty() { stderr } else { stdout })
            },
        )
    }

    /// 切换目标系统挂载根目录（传 "/" 或空串恢复宿主机默认环境）
    ///
    /// # Errors
    /// 当目标根目录不存在或缺少 `/etc/default/grub` 与 `/etc/grub.d` 时返回 [`DaemonError::SystemToolFailed`]。
    pub fn set_target_root_as(
        &self,
        root_path: &str,
        caller_uid: u32,
    ) -> Result<PathBuf, DaemonError> {
        let _write_guard = self.acquire_write_lock();
        let normalized = if root_path.trim().is_empty() || root_path.trim() == "/" {
            PathBuf::from("/")
        } else {
            fs::canonicalize(root_path.trim()).map_err(|e| DaemonError::SystemToolFailed {
                operation: "校验目标根目录".to_string(),
                reason: format!("无法访问目录 '{root_path}': {e}"),
            })?
        };

        with_audit(
            caller_uid,
            AuditAction::SwitchTargetRoot,
            format!("切换目标系统根目录至 {}", normalized.display()),
            None,
            None,
            |_| (true, None),
            || {
                if normalized != Path::new("/") {
                    if !normalized.is_dir() {
                        return Err(DaemonError::SystemToolFailed {
                            operation: "校验目标根目录".to_string(),
                            reason: format!("'{}' 不是目录", normalized.display()),
                        });
                    }
                    let has_default = normalized.join("etc/default/grub").is_file();
                    let has_grub_d = normalized.join("etc/grub.d").is_dir();
                    if !has_default && !has_grub_d {
                        return Err(DaemonError::SystemToolFailed {
                            operation: "校验目标系统结构".to_string(),
                            reason: format!(
                                "目录 '{}' 下既无 etc/default/grub 也无 etc/grub.d，不像是可修复的 Linux 根分区",
                                normalized.display()
                            ),
                        });
                    }
                }

                let mut guard = self.target_root.lock().unwrap_or_else(|p| p.into_inner());
                *guard = normalized.clone();
                info!("已切换 Helmsman 目标系统根目录为: {}", normalized.display());
                Ok(normalized)
            },
        )
    }

    /// 供状态查询返回当前目标根目录与实时探测轮廓
    pub fn current_distro_profile(&self) -> &DistroProfile {
        &self.distro_profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_font_output_name() {
        assert_eq!(
            sanitize_font_output_name("NotoSans-16").unwrap(),
            "NotoSans-16.pf2"
        );
        assert_eq!(
            sanitize_font_output_name("ubuntu_mono.pf2").unwrap(),
            "ubuntu_mono.pf2"
        );
        assert!(sanitize_font_output_name("../evil").is_err());
        assert!(sanitize_font_output_name("bad name").is_err());
        assert!(sanitize_font_output_name("").is_err());
    }

    #[test]
    fn test_install_mode_parse() {
        assert_eq!(
            GrubInstallMode::parse("uefi").unwrap(),
            GrubInstallMode::Uefi
        );
        assert_eq!(
            GrubInstallMode::parse("BIOS").unwrap(),
            GrubInstallMode::Bios
        );
        assert!(GrubInstallMode::parse("invalid").is_err());
    }
}
