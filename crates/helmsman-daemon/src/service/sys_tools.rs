use crate::audit::{AuditAction, with_audit};
use crate::executor::SafeCommand;
use crate::service::{DaemonError, GrubService};
use helmsman_client::SystemFontDto;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::info;

/// 允许转换的字体文件扩展名白名单
const ALLOWED_FONT_EXTS: &[&str] = &["ttf", "otf", "ttc"];

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
}
