use crate::audit::{AuditAction, resolve_caller_uid, with_audit};
use crate::service::{DaemonError, GrubService};
use helmsman_client::GrubScriptDto;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tracing::info;

/// 单个引导脚本最大读取字节数（256 KiB），防止异常大文件撑爆内存
const MAX_SCRIPT_READ_BYTES: usize = 256 * 1024;

/// 校验脚本名称仅为单段合法文件名，并规范化确认其位于受控 `/etc/grub.d` 目录内
fn resolve_safe_script_path(grub_d_dir: &Path, script_name: &str) -> Result<PathBuf, DaemonError> {
    let trimmed = script_name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
    {
        return Err(DaemonError::SystemToolFailed {
            operation: "原生脚本校验".to_string(),
            reason: format!("非法脚本名称 '{script_name}'，仅允许单段文件名"),
        });
    }

    let base_canon = fs::canonicalize(grub_d_dir).map_err(|e| DaemonError::SystemToolFailed {
        operation: "读取脚本目录".to_string(),
        reason: format!("无法访问目录 {}: {e}", grub_d_dir.display()),
    })?;
    let target_canon =
        fs::canonicalize(base_canon.join(trimmed)).map_err(|e| DaemonError::SystemToolFailed {
            operation: "定位脚本文件".to_string(),
            reason: format!("找不到脚本 '{trimmed}': {e}"),
        })?;

    if !target_canon.starts_with(&base_canon) || !target_canon.is_file() {
        return Err(DaemonError::SystemToolFailed {
            operation: "脚本路径防穿越校验".to_string(),
            reason: format!(
                "目标路径 '{}' 超出受控目录范围或不是常规文件",
                target_canon.display()
            ),
        });
    }

    Ok(target_canon)
}

/// 在指定 `/etc/grub.d` 目录列出全部原生脚本（解耦供测试直接复用）
pub(crate) fn list_grub_scripts_in_dir(
    grub_d_dir: &Path,
) -> Result<Vec<GrubScriptDto>, DaemonError> {
    if !grub_d_dir.exists() {
        return Err(DaemonError::SystemToolFailed {
            operation: "扫描原生引导脚本".to_string(),
            reason: format!("目录不存在: {}", grub_d_dir.display()),
        });
    }

    let entries = fs::read_dir(grub_d_dir).map_err(|e| DaemonError::SystemToolFailed {
        operation: "扫描原生引导脚本".to_string(),
        reason: format!("读取目录 {} 失败: {e}", grub_d_dir.display()),
    })?;

    let mut scripts = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // 跳过备份文件或包管理器残留后缀
        if name.starts_with('.') || name.ends_with(".dpkg-old") || name.ends_with(".dpkg-dist") {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let mode = meta.permissions().mode();
        let enabled = (mode & 0o111) != 0;
        let bytes = fs::read(&path).unwrap_or_default();
        let truncated = bytes.len() > MAX_SCRIPT_READ_BYTES;
        let slice = if truncated {
            &bytes[..MAX_SCRIPT_READ_BYTES]
        } else {
            &bytes[..]
        };
        let mut content = String::from_utf8_lossy(slice).into_owned();
        if truncated {
            content.push_str("\n\n# ...（内容超过 256 KiB 已截断）");
        }
        let is_helmsman_managed = name.contains("helmsman");
        scripts.push(GrubScriptDto {
            name: name.to_string(),
            enabled,
            size_bytes: meta.len(),
            is_helmsman_managed,
            content,
        });
    }

    scripts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(scripts)
}

/// 在指定 `/etc/grub.d` 目录切换脚本可执行权限（0o755 / 0o644）
pub(crate) fn set_script_enabled_in_dir(
    grub_d_dir: &Path,
    script_name: &str,
    enabled: bool,
) -> Result<PathBuf, DaemonError> {
    let target = resolve_safe_script_path(grub_d_dir, script_name)?;
    let meta = fs::metadata(&target).map_err(|e| DaemonError::SystemToolFailed {
        operation: "读取脚本权限".to_string(),
        reason: e.to_string(),
    })?;
    let mut perms = meta.permissions();
    let new_mode = if enabled { 0o755 } else { 0o644 };
    perms.set_mode(new_mode);
    fs::set_permissions(&target, perms).map_err(|e| DaemonError::SystemToolFailed {
        operation: "更新脚本权限".to_string(),
        reason: e.to_string(),
    })?;
    Ok(target)
}

impl GrubService {
    /// 列出当前目标环境 `/etc/grub.d/` 下的原生引导脚本
    ///
    /// # Errors
    /// 当脚本目录不存在或无法读取时返回 [`DaemonError::SystemToolFailed`]。
    pub fn list_grub_scripts(&self) -> Result<Vec<GrubScriptDto>, DaemonError> {
        let dir = self.effective_grub_d_dir();
        list_grub_scripts_in_dir(&dir)
    }

    /// 启用或禁用 `/etc/grub.d/` 中的指定引导脚本（审计 UID 取自进程回退）
    ///
    /// # Errors
    /// 当脚本名称非法、路径越界或修改权限失败时返回 [`DaemonError::SystemToolFailed`]。
    pub fn set_grub_script_enabled(
        &self,
        script_name: &str,
        enabled: bool,
    ) -> Result<(), DaemonError> {
        self.set_grub_script_enabled_as(script_name, enabled, resolve_caller_uid())
    }

    /// 以显式调用方 UID 启用或禁用 `/etc/grub.d/` 中的指定引导脚本
    ///
    /// # Errors
    /// 当脚本名称非法、路径越界或修改权限失败时返回 [`DaemonError::SystemToolFailed`]。
    pub fn set_grub_script_enabled_as(
        &self,
        script_name: &str,
        enabled: bool,
        caller_uid: u32,
    ) -> Result<(), DaemonError> {
        let _write_guard = self.acquire_write_lock();
        let action_desc = if enabled { "启用" } else { "禁用" };
        with_audit(
            caller_uid,
            AuditAction::ToggleGrubScript,
            format!("{action_desc}引导脚本 {script_name}"),
            None,
            None,
            |_| (true, None),
            || {
                let dir = self.effective_grub_d_dir();
                let target = set_script_enabled_in_dir(&dir, script_name, enabled)?;
                info!(
                    "已{}原生引导脚本 {} (路径: {})",
                    action_desc,
                    script_name,
                    target.display()
                );
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grub_scripts_list_toggle_and_traversal_guard() {
        let dir =
            std::env::temp_dir().join(format!("helmsman_grub_scripts_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("10_linux");
        fs::write(&script_path, "#!/bin/sh\necho linux\n").unwrap();

        set_script_enabled_in_dir(&dir, "10_linux", true).unwrap();
        let list = list_grub_scripts_in_dir(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].enabled);

        set_script_enabled_in_dir(&dir, "10_linux", false).unwrap();
        let list2 = list_grub_scripts_in_dir(&dir).unwrap();
        assert!(!list2[0].enabled);

        assert!(set_script_enabled_in_dir(&dir, "../evil", true).is_err());
        assert!(set_script_enabled_in_dir(&dir, "a/b", true).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
