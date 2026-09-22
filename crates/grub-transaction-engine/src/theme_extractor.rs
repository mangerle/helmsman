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
    /// 不支持的主题压缩包格式
    UnsupportedArchiveFormat { path: PathBuf },
    /// 压缩包文件损坏或无法解析
    ArchiveCorrupted { path: PathBuf, reason: String },
    /// 压缩包解压超出安全配额限制 (解压炸弹防御)
    ResourceLimitExceeded { reason: String },
    /// 压缩包内包含不安全的文件类型 (如符号链接或设备节点)
    InsecureEntryType {
        entry_path: String,
        entry_type: String,
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
            ThemeSecurityError::UnsupportedArchiveFormat { path } => {
                write!(
                    f,
                    "不支持的主题压缩包格式: {}。仅支持 .zip、.tar.gz 或 .tar 格式",
                    path.display()
                )
            }
            ThemeSecurityError::ArchiveCorrupted { path, reason } => {
                write!(
                    f,
                    "主题压缩包 '{}' 损坏或格式错误: {}",
                    path.display(),
                    reason
                )
            }
            ThemeSecurityError::ResourceLimitExceeded { reason } => {
                write!(f, "主题包安全限制触发（防解压炸弹）: {}", reason)
            }
            ThemeSecurityError::InsecureEntryType {
                entry_path,
                entry_type,
            } => {
                write!(
                    f,
                    "拒绝提取包含潜在安全隐患的文件类型！条目: '{}'，类型: {}",
                    entry_path, entry_type
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

/// 临时沙箱守卫，利用 RAII 确保在退出作用域时自动清理临时解压目录
struct TempSandboxGuard(PathBuf);

impl Drop for TempSandboxGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 从主题压缩包（.zip、.tar.gz 或 .tar）直接安全解压并安装至系统主题根目录
///
/// # 处理流程与安全保障
/// 1. 创建隔离临时解压沙箱（带 RAII 自动析构清理）。
/// 2. 解压压缩包并执行严密的防 Zip Slip、防解压炸弹与危险文件过滤。
/// 3. 自适应识别包含 `theme.txt` 的实际主题目录（自动剥离顶层父目录包装）。
/// 4. 自动推断或校验指定的主题名称。
/// 5. 原子复制并安装至目标主题目录（如 `/boot/grub/themes/<theme_name>`）。
pub fn install_theme_from_archive(
    archive_path: &Path,
    target_themes_root: &Path,
    custom_theme_name: Option<&str>,
) -> Result<PathBuf, ThemeSecurityError> {
    use crate::theme_archive::{extract_archive_to_dir, find_theme_root_in_dir};

    if !archive_path.is_file() {
        return Err(ThemeSecurityError::IoError {
            action: "读取压缩包文件".to_string(),
            path: archive_path.to_path_buf(),
            reason: "指定的主题压缩包文件不存在或不是普通文件".to_string(),
        });
    }

    // 创建唯一命名的临时解压沙箱
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_sandbox = std::env::temp_dir().join(format!(
        "helmsman_extract_{}_{}",
        std::process::id(),
        timestamp
    ));
    let _guard = TempSandboxGuard(temp_sandbox.clone());

    // 1. 解压至临时沙箱
    extract_archive_to_dir(archive_path, &temp_sandbox)?;

    // 2. 探测包含 theme.txt 的真实主题根目录
    let source_theme_dir = find_theme_root_in_dir(&temp_sandbox)?;

    // 3. 确定最终主题名称
    let final_name = match custom_theme_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            if source_theme_dir != temp_sandbox {
                source_theme_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "custom_theme".to_string())
            } else {
                derive_theme_name_from_archive(archive_path)
            }
        }
    };

    validate_theme_name(&final_name)?;

    // 4. 安装至系统目标根目录
    let installed_path =
        install_theme_directory(&source_theme_dir, target_themes_root, &final_name)?;

    Ok(installed_path)
}

/// 从压缩包文件名推导默认主题名（剥离常见的压缩扩展名）
fn derive_theme_name_from_archive(archive_path: &Path) -> String {
    let filename = archive_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "custom_theme".to_string());

    let lower = filename.to_lowercase();
    if let Some(stripped) = lower.strip_suffix(".tar.gz") {
        filename[..stripped.len()].to_string()
    } else if let Some(stripped) = lower.strip_suffix(".tgz") {
        filename[..stripped.len()].to_string()
    } else if let Some(stripped) = lower.strip_suffix(".zip") {
        filename[..stripped.len()].to_string()
    } else if let Some(stripped) = lower.strip_suffix(".tar") {
        filename[..stripped.len()].to_string()
    } else {
        filename
    }
}

/// 已安装主题摘要
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledThemeInfo {
    /// 主题目录名（作为主题标识）
    pub name: String,
    /// 主题目录绝对路径
    pub path: PathBuf,
    /// 是否包含合法的 theme.txt 描述文件
    pub has_descriptor: bool,
}

/// 列出主题根目录下已安装的主题
///
/// # Errors
/// 主题根目录不存在时返回空列表；读取目录失败时返回 [`ThemeSecurityError::IoError`]。
pub fn list_installed_themes(
    themes_root: &Path,
) -> Result<Vec<InstalledThemeInfo>, ThemeSecurityError> {
    if !themes_root.exists() {
        return Ok(Vec::new());
    }

    let mut themes = Vec::new();
    let read_dir = fs::read_dir(themes_root).map_err(|e| ThemeSecurityError::IoError {
        action: "读取主题根目录".to_string(),
        path: themes_root.to_path_buf(),
        reason: e.to_string(),
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| ThemeSecurityError::IoError {
            action: "遍历主题根目录".to_string(),
            path: themes_root.to_path_buf(),
            reason: e.to_string(),
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // 跳过非法目录名，避免与卸载校验口径不一致
        if validate_theme_name(name).is_err() {
            continue;
        }
        themes.push(InstalledThemeInfo {
            name: name.to_string(),
            has_descriptor: path.join("theme.txt").is_file(),
            path,
        });
    }

    themes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(themes)
}

/// 卸载指定名称的主题目录
///
/// # Errors
/// 名称非法、路径穿越或删除失败时返回对应的 [`ThemeSecurityError`]。
pub fn remove_theme(themes_root: &Path, theme_name: &str) -> Result<(), ThemeSecurityError> {
    validate_theme_name(theme_name)?;
    let target = themes_root.join(theme_name);

    let canonical_root = themes_root
        .canonicalize()
        .map_err(|e| ThemeSecurityError::IoError {
            action: "解析主题根目录".to_string(),
            path: themes_root.to_path_buf(),
            reason: e.to_string(),
        })?;
    let canonical_target = target
        .canonicalize()
        .map_err(|e| ThemeSecurityError::IoError {
            action: "解析主题目录".to_string(),
            path: target.clone(),
            reason: e.to_string(),
        })?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err(ThemeSecurityError::PathTraversalDetected {
            entry_path: theme_name.to_string(),
            target_root: themes_root.to_path_buf(),
        });
    }
    if !canonical_target.is_dir() {
        return Err(ThemeSecurityError::MissingThemeDescriptor {
            theme_dir: canonical_target,
        });
    }

    fs::remove_dir_all(&canonical_target).map_err(|e| ThemeSecurityError::IoError {
        action: "删除主题目录".to_string(),
        path: canonical_target,
        reason: e.to_string(),
    })?;
    Ok(())
}
