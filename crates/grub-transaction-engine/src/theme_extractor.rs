use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// 主题包安全与解压错误枚举
///
/// # 设计原理
/// - **实现初衷**：在导入外部第三方 GRUB 主题时，必须严格防御 Zip Slip 路径穿越攻击，
///   并确保导入的内容具备合法的 `theme.txt` 描述符。
#[derive(Debug, PartialEq, Eq)]
pub enum ThemeSecurityError {
    /// 检测到路径穿越攻击 (包含 `..` 或试图逃逸出目标沙箱根目录)
    PathTraversalDetected {
        entry_path: String,
        target_root: PathBuf,
    },
    /// 主题名称非法 (包含路径分隔符或保留字符)
    InvalidThemeName { name: String, reason: String },
    /// 主题解压后缺少必须的 theme.txt 描述符文件
    MissingThemeDescriptor { theme_dir: PathBuf },
    /// 文件系统 I/O 操作失败
    IoError {
        action: String,
        path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for ThemeSecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeSecurityError::PathTraversalDetected {
                entry_path,
                target_root,
            } => {
                write!(
                    f,
                    "检测到潜在路径穿越攻击！条目路径: '{}'，试图逃逸出目标沙箱目录: {}",
                    entry_path,
                    target_root.display()
                )
            }
            ThemeSecurityError::InvalidThemeName { name, reason } => {
                write!(f, "主题名称 '{}' 非法: {}", name, reason)
            }
            ThemeSecurityError::MissingThemeDescriptor { theme_dir } => {
                write!(
                    f,
                    "主题包不完整，在目录 '{}' 下未找到合法的 theme.txt 描述文件",
                    theme_dir.display()
                )
            }
            ThemeSecurityError::IoError {
                action,
                path,
                reason,
            } => {
                write!(
                    f,
                    "执行主题文件操作 '{}' 失败，路径: {}，原因: {}",
                    action,
                    path.display(),
                    reason
                )
            }
        }
    }
}

impl Error for ThemeSecurityError {}

/// 校验主题名称合法性（防止主题名自身包含路径穿越字符）
pub fn validate_theme_name(name: &str) -> Result<(), ThemeSecurityError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ThemeSecurityError::InvalidThemeName {
            name: name.to_string(),
            reason: "主题名称不能为空".to_string(),
        });
    }

    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(ThemeSecurityError::InvalidThemeName {
            name: name.to_string(),
            reason: "主题名称不能包含路径分隔符或 '..' 相对符号".to_string(),
        });
    }

    Ok(())
}

/// 校验压缩包条目相对路径并安全解析为目标绝对路径（防 Zip Slip 核心红线）
///
/// # 安全保障
/// 1. 拦截以 `/` 或 Windows 盘符开头的绝对路径。
/// 2. 遍历路径组件，凡包含 `Component::ParentDir` (`..`) 立即拦截。
/// 3. 计算拼接后的完整路径，确保其必须以 `target_root` 为前缀。
pub fn validate_entry_path(
    target_root: &Path,
    entry_rel_path: &str,
) -> Result<PathBuf, ThemeSecurityError> {
    let trimmed = entry_rel_path.trim().replace('\\', "/");

    // 绝对路径拒绝
    if trimmed.starts_with('/') || trimmed.starts_with('\\') || trimmed.contains(':') {
        return Err(ThemeSecurityError::PathTraversalDetected {
            entry_path: entry_rel_path.to_string(),
            target_root: target_root.to_path_buf(),
        });
    }

    let rel_path = Path::new(&trimmed);
    for comp in rel_path.components() {
        match comp {
            Component::ParentDir => {
                return Err(ThemeSecurityError::PathTraversalDetected {
                    entry_path: entry_rel_path.to_string(),
                    target_root: target_root.to_path_buf(),
                });
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ThemeSecurityError::PathTraversalDetected {
                    entry_path: entry_rel_path.to_string(),
                    target_root: target_root.to_path_buf(),
                });
            }
            Component::Normal(_) | Component::CurDir => {}
        }
    }

    let dest = target_root.join(rel_path);
    // 二次确认：确保前缀严格匹配
    if !dest.starts_with(target_root) {
        return Err(ThemeSecurityError::PathTraversalDetected {
            entry_path: entry_rel_path.to_string(),
            target_root: target_root.to_path_buf(),
        });
    }

    Ok(dest)
}

/// 校验主题目录是否包含合法的 `theme.txt` 描述文件
pub fn validate_theme_directory(theme_dir: &Path) -> Result<(), ThemeSecurityError> {
    let theme_txt = theme_dir.join("theme.txt");
    if theme_txt.is_file() {
        Ok(())
    } else {
        Err(ThemeSecurityError::MissingThemeDescriptor {
            theme_dir: theme_dir.to_path_buf(),
        })
    }
}

/// 安全解压/展开归档条目列表到目标主题目录中
///
/// 凡遇到试图越权逃逸出 `target_dir` 的条目，立即全部回滚清理并报错。
pub fn extract_safe_entries<'a, I>(target_dir: &Path, entries: I) -> Result<(), ThemeSecurityError>
where
    I: IntoIterator<Item = (&'a str, bool, &'a [u8])>,
{
    fs::create_dir_all(target_dir).map_err(|e| ThemeSecurityError::IoError {
        action: "创建主题目录".to_string(),
        path: target_dir.to_path_buf(),
        reason: e.to_string(),
    })?;

    for (entry_path, is_dir, content) in entries {
        let dest = validate_entry_path(target_dir, entry_path)?;
        if is_dir {
            fs::create_dir_all(&dest).map_err(|e| ThemeSecurityError::IoError {
                action: "创建子目录".to_string(),
                path: dest,
                reason: e.to_string(),
            })?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| ThemeSecurityError::IoError {
                    action: "创建父目录".to_string(),
                    path: parent.to_path_buf(),
                    reason: e.to_string(),
                })?;
            }
            fs::write(&dest, content).map_err(|e| ThemeSecurityError::IoError {
                action: "写入条目文件".to_string(),
                path: dest,
                reason: e.to_string(),
            })?;
        }
    }

    // 解压后完成完整性校验
    validate_theme_directory(target_dir)?;

    Ok(())
}

/// 从已存在的外部临时解压目录安全导入并安装主题至系统主题根目录
pub fn install_theme_directory(
    source_dir: &Path,
    target_themes_root: &Path,
    theme_name: &str,
) -> Result<PathBuf, ThemeSecurityError> {
    validate_theme_name(theme_name)?;
    validate_theme_directory(source_dir)?;

    let target_theme_dir = target_themes_root.join(theme_name);
    if target_theme_dir.exists() {
        fs::remove_dir_all(&target_theme_dir).map_err(|e| ThemeSecurityError::IoError {
            action: "清理旧主题目录".to_string(),
            path: target_theme_dir.clone(),
            reason: e.to_string(),
        })?;
    }

    fs::create_dir_all(&target_theme_dir).map_err(|e| ThemeSecurityError::IoError {
        action: "创建目标主题目录".to_string(),
        path: target_theme_dir.clone(),
        reason: e.to_string(),
    })?;

    // 递归安全复制
    copy_dir_recursive(source_dir, &target_theme_dir)?;

    // 安装后最终确认
    validate_theme_directory(&target_theme_dir)?;

    Ok(target_theme_dir)
}

/// 递归复制目录内容（严格做目标路径沙箱防穿越校验）
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), ThemeSecurityError> {
    let read_dir = fs::read_dir(source).map_err(|e| ThemeSecurityError::IoError {
        action: "读取源目录".to_string(),
        path: source.to_path_buf(),
        reason: e.to_string(),
    })?;

    for entry in read_dir.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        let dest_path = validate_entry_path(destination, &file_name_str)?;

        if file_type.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| ThemeSecurityError::IoError {
                action: "创建子目录".to_string(),
                path: dest_path.clone(),
                reason: e.to_string(),
            })?;
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest_path).map_err(|e| ThemeSecurityError::IoError {
                action: "复制文件".to_string(),
                path: dest_path,
                reason: e.to_string(),
            })?;
        }
    }

    Ok(())
}
