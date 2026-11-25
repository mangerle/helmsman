use crate::audit::{AuditAction, resolve_caller_uid, with_audit};
use crate::service::{DaemonError, GrubService};
use grub_transaction_engine::{InstalledThemeInfo, list_installed_themes, remove_theme};
use std::fs;
use std::path::{Path, PathBuf};

/// 主题包校验、安装、列表查询与卸载等运维操作
impl GrubService {
    /// 校验主题压缩包源路径是否位于允许的用户可读沙箱目录内
    ///
    /// # 设计原理
    /// - **实现初衷**：D-Bus 客户端可传入任意本地路径，守护进程以 root 读取会造成任意文件读取与大文件 DoS。
    /// - **核心优势**：仅接受临时目录、用户主目录与运行时目录下的普通文件，并限制体积上限。
    /// - **代价与局限**：不支持通过 FileDescriptor/portal 直传，调用方需先把压缩包落到允许目录。
    fn validate_theme_archive_source(path: &Path, caller_uid: u32) -> Result<(), DaemonError> {
        let canonical = path
            .canonicalize()
            .map_err(|e| DaemonError::ThemeInstallFailed {
                reason: format!("主题压缩包路径无法解析 '{}': {}", path.display(), e),
            })?;

        let meta = std::fs::metadata(&canonical).map_err(|e| DaemonError::ThemeInstallFailed {
            reason: format!("无法读取主题压缩包元数据 '{}': {}", canonical.display(), e),
        })?;

        if !meta.is_file() {
            return Err(DaemonError::ThemeInstallFailed {
                reason: format!("主题压缩包不是普通文件: {}", canonical.display()),
            });
        }

        const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
        if meta.len() > MAX_ARCHIVE_BYTES {
            return Err(DaemonError::ThemeInstallFailed {
                reason: format!(
                    "主题压缩包过大 ({} 字节)，上限 {} 字节",
                    meta.len(),
                    MAX_ARCHIVE_BYTES
                ),
            });
        }

        let mut allowed_roots: Vec<PathBuf> = Vec::with_capacity(5);
        allowed_roots.push(std::env::temp_dir());
        allowed_roots.push(PathBuf::from("/tmp"));
        allowed_roots.push(PathBuf::from("/var/tmp"));
        allowed_roots.push(PathBuf::from(format!("/run/user/{caller_uid}")));
        if let Some(user_home) = resolve_user_home_dir(caller_uid) {
            allowed_roots.push(user_home);
        }

        let under_allowed = allowed_roots.iter().any(|root| {
            let root_canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
            canonical.starts_with(root_canonical)
        });
        if !under_allowed {
            return Err(DaemonError::ThemeInstallFailed {
                reason: format!(
                    "主题压缩包必须位于临时目录、用户主目录或运行时目录内，当前路径: {}",
                    canonical.display()
                ),
            });
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let owner = meta.uid();
            let proc_uid = std::fs::metadata("/proc/self").ok().map(|m| m.uid());
            let allowed_owner =
                owner == 0 || owner == caller_uid || (caller_uid == 0 && proc_uid == Some(owner));
            if !allowed_owner {
                return Err(DaemonError::ThemeInstallFailed {
                    reason: format!(
                        "主题压缩包属主 UID {} 与调用方 UID {} 不一致: {}",
                        owner,
                        caller_uid,
                        canonical.display()
                    ),
                });
            }
        }

        Ok(())
    }

    /// 从压缩包安全安装主题至系统主题目录（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 当压缩包校验失败、路径穿越或落盘失败时返回 [`DaemonError::ThemeInstallFailed`]。
    pub fn install_theme_archive(
        &self,
        archive_path: &Path,
        theme_name: Option<&str>,
    ) -> Result<PathBuf, DaemonError> {
        self.install_theme_archive_as(archive_path, theme_name, resolve_caller_uid())
    }

    /// 以显式调用方 UID 从压缩包安全安装主题
    ///
    /// # Errors
    /// 当压缩包校验失败、路径穿越或落盘失败时返回 [`DaemonError::ThemeInstallFailed`]。
    pub fn install_theme_archive_as(
        &self,
        archive_path: &Path,
        theme_name: Option<&str>,
        caller_uid: u32,
    ) -> Result<PathBuf, DaemonError> {
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::InstallTheme,
            format!("安装主题压缩包: {}", archive_path.display()),
            None,
            None,
            |_| (true, None),
            || {
                Self::validate_theme_archive_source(archive_path, caller_uid)?;
                let installed = grub_transaction_engine::install_theme_from_archive(
                    archive_path,
                    &self.themes_dir,
                    theme_name,
                )
                .map_err(|e| DaemonError::ThemeInstallFailed {
                    reason: e.to_string(),
                })?;
                Ok(installed)
            },
        )
    }

    /// 列出已安装的 GRUB 主题
    ///
    /// # Errors
    /// 主题目录读取失败时返回 [`DaemonError::ThemeInstallFailed`]。
    pub fn list_themes(&self) -> Result<Vec<InstalledThemeInfo>, DaemonError> {
        list_installed_themes(&self.themes_dir).map_err(|e| DaemonError::ThemeInstallFailed {
            reason: e.to_string(),
        })
    }

    /// 卸载指定名称的主题（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 名称非法、路径穿越或删除失败时返回对应的 [`DaemonError`]。
    pub fn remove_theme(&self, theme_name: &str) -> Result<(), DaemonError> {
        self.remove_theme_as(theme_name, resolve_caller_uid())
    }

    /// 以显式调用方 UID 卸载主题
    ///
    /// # Errors
    /// 名称非法、路径穿越或删除失败时返回对应的 [`DaemonError`]。
    pub fn remove_theme_as(&self, theme_name: &str, caller_uid: u32) -> Result<(), DaemonError> {
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::RemoveTheme,
            format!("卸载主题 '{theme_name}'"),
            None,
            None,
            |_| (true, None),
            || {
                remove_theme(&self.themes_dir, theme_name).map_err(|e| {
                    DaemonError::ThemeInstallFailed {
                        reason: format!("卸载主题 '{theme_name}' 失败: {e}"),
                    }
                })
            },
        )
    }
}

/// 根据调用方 Unix UID 解析其真实主目录（读取 `/etc/passwd`，避免 root 守护进程 `HOME` 误判）
fn resolve_user_home_dir(caller_uid: u32) -> Option<PathBuf> {
    if let Ok(passwd) = fs::read_to_string("/etc/passwd")
        && let Some(home) = parse_passwd_home_for_uid(&passwd, caller_uid)
    {
        return Some(home);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = fs::metadata("/proc/self")
            && meta.uid() == caller_uid
        {
            return std::env::var_os("HOME").map(PathBuf::from);
        }
    }
    None
}

/// 从 `/etc/passwd` 文本中提取指定 UID 的第 6 列主目录
fn parse_passwd_home_for_uid(passwd_content: &str, target_uid: u32) -> Option<PathBuf> {
    for line in passwd_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut fields = trimmed.split(':');
        let _name = fields.next()?;
        let _pass = fields.next()?;
        let uid_str = fields.next()?;
        let _gid = fields.next()?;
        let _gecos = fields.next()?;
        let home = fields.next()?;
        if uid_str.parse::<u32>().ok() == Some(target_uid) && !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_passwd_home_for_uid;
    use std::path::PathBuf;

    #[test]
    fn test_parse_passwd_home_for_uid() {
        let sample = "root:x:0:0:root:/root:/bin/bash\nwangle:x:1000:1000::/home/wangle:/bin/zsh\n";
        assert_eq!(
            parse_passwd_home_for_uid(sample, 0),
            Some(PathBuf::from("/root"))
        );
        assert_eq!(
            parse_passwd_home_for_uid(sample, 1000),
            Some(PathBuf::from("/home/wangle"))
        );
        assert_eq!(parse_passwd_home_for_uid(sample, 9999), None);
    }
}
