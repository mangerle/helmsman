use crate::theme_extractor::{ThemeSecurityError, validate_entry_path};
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tar::{Archive as TarArchive, EntryType};
use zip::ZipArchive;

/// 单个解压文件最大允许字节数 (100 MB)
pub const MAX_SINGLE_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// 解压总容量最大允许字节数 (200 MB)
pub const MAX_TOTAL_UNCOMPRESSED_SIZE: u64 = 200 * 1024 * 1024;

/// 压缩包内最大允许条目数量 (2000 个)
pub const MAX_ENTRIES_COUNT: usize = 2000;

/// 支持的主题压缩包格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// 标准 ZIP 格式 (.zip)
    Zip,
    /// Gzip 压缩 TAR 格式 (.tar.gz, .tgz)
    TarGz,
    /// 未压缩 TAR 格式 (.tar)
    Tar,
}

/// 识别压缩包格式（结合魔数与扩展名双重判定）
pub fn detect_archive_format(path: &Path) -> Result<ArchiveFormat, ThemeSecurityError> {
    let mut file = File::open(path).map_err(|e| ThemeSecurityError::IoError {
        action: "打开压缩文件".to_string(),
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let mut header = [0u8; 512];
    let n = file.read(&mut header).unwrap_or(0);

    // 1. 基于魔数优先判断
    if n >= 4 && &header[0..4] == b"PK\x03\x04" {
        return Ok(ArchiveFormat::Zip);
    }
    if n >= 2 && &header[0..2] == b"\x1f\x8b" {
        return Ok(ArchiveFormat::TarGz);
    }
    if n >= 262 && &header[257..262] == b"ustar" {
        return Ok(ArchiveFormat::Tar);
    }

    // 2. 回退基于文件扩展名判断
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if filename.ends_with(".zip") {
        Ok(ArchiveFormat::Zip)
    } else if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        Ok(ArchiveFormat::TarGz)
    } else if filename.ends_with(".tar") {
        Ok(ArchiveFormat::Tar)
    } else {
        Err(ThemeSecurityError::UnsupportedArchiveFormat {
            path: path.to_path_buf(),
        })
    }
}

/// 将压缩包安全解压至指定的临时目标目录中
///
/// # 安全保障
/// 1. **防 Zip Slip**：所有条目路径均经过 `validate_entry_path` 校验。
/// 2. **防解压炸弹**：严格限制最大条目数、单文件大小及总解压容量。
/// 3. **防危险文件**：严禁提取符号链接、硬链接和设备节点文件。
pub fn extract_archive_to_dir(
    archive_path: &Path,
    target_dir: &Path,
) -> Result<(), ThemeSecurityError> {
    let format = detect_archive_format(archive_path)?;

    fs::create_dir_all(target_dir).map_err(|e| ThemeSecurityError::IoError {
        action: "创建解压目标目录".to_string(),
        path: target_dir.to_path_buf(),
        reason: e.to_string(),
    })?;

    let file = File::open(archive_path).map_err(|e| ThemeSecurityError::IoError {
        action: "打开压缩文件".to_string(),
        path: archive_path.to_path_buf(),
        reason: e.to_string(),
    })?;

    match format {
        ArchiveFormat::Zip => extract_zip(file, target_dir, archive_path),
        ArchiveFormat::TarGz => {
            let gz_decoder = GzDecoder::new(file);
            extract_tar(gz_decoder, target_dir, archive_path)
        }
        ArchiveFormat::Tar => extract_tar(file, target_dir, archive_path),
    }
}

/// 解压 ZIP 压缩包
fn extract_zip(
    file: File,
    target_dir: &Path,
    archive_path: &Path,
) -> Result<(), ThemeSecurityError> {
    let mut zip = ZipArchive::new(file).map_err(|e| ThemeSecurityError::ArchiveCorrupted {
        path: archive_path.to_path_buf(),
        reason: e.to_string(),
    })?;

    if zip.len() > MAX_ENTRIES_COUNT {
        return Err(ThemeSecurityError::ResourceLimitExceeded {
            reason: format!(
                "压缩包内条目数量 ({} 个) 超出安全上限 ({} 个)",
                zip.len(),
                MAX_ENTRIES_COUNT
            ),
        });
    }

    let mut total_uncompressed: u64 = 0;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| ThemeSecurityError::ArchiveCorrupted {
                path: archive_path.to_path_buf(),
                reason: e.to_string(),
            })?;

        let entry_name = entry.name().to_string();

        // 检查是否为符号链接（Unix 权限位检测 0o120000 = S_IFLNK）
        if let Some(mode) = entry.unix_mode()
            && (mode & 0o170000) == 0o120000
        {
            return Err(ThemeSecurityError::InsecureEntryType {
                entry_path: entry_name,
                entry_type: "符号链接 (Symbolic Link)".to_string(),
            });
        }

        let dest_path = validate_entry_path(target_dir, &entry_name)?;

        if entry.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| ThemeSecurityError::IoError {
                action: "创建目录".to_string(),
                path: dest_path,
                reason: e.to_string(),
            })?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ThemeSecurityError::IoError {
                    action: "创建父级目录".to_string(),
                    path: parent.to_path_buf(),
                    reason: e.to_string(),
                })?;
            }

            let mut out = File::create(&dest_path).map_err(|e| ThemeSecurityError::IoError {
                action: "创建解压文件".to_string(),
                path: dest_path.clone(),
                reason: e.to_string(),
            })?;

            // 限制单文件读取大小，防止未压缩大小被伪造
            let mut limited_reader = (&mut entry).take(MAX_SINGLE_FILE_SIZE + 1);
            let written = io::copy(&mut limited_reader, &mut out).map_err(|e| {
                ThemeSecurityError::IoError {
                    action: "写入解压文件".to_string(),
                    path: dest_path.clone(),
                    reason: e.to_string(),
                }
            })?;

            if written > MAX_SINGLE_FILE_SIZE {
                return Err(ThemeSecurityError::ResourceLimitExceeded {
                    reason: format!("条目 '{}' 解压大小超出单文件安全上限 (100 MB)", entry_name),
                });
            }

            total_uncompressed = total_uncompressed.saturating_add(written);
            if total_uncompressed > MAX_TOTAL_UNCOMPRESSED_SIZE {
                return Err(ThemeSecurityError::ResourceLimitExceeded {
                    reason: format!(
                        "压缩包解压总容量超出安全上限 (200 MB)，已解压 {} 字节",
                        total_uncompressed
                    ),
                });
            }
        }
    }

    Ok(())
}

/// 解压 TAR / TAR.GZ 归档流
fn extract_tar<R: Read>(
    reader: R,
    target_dir: &Path,
    archive_path: &Path,
) -> Result<(), ThemeSecurityError> {
    let mut tar = TarArchive::new(reader);
    let mut total_uncompressed: u64 = 0;
    let mut entry_count: usize = 0;

    let entries = tar
        .entries()
        .map_err(|e| ThemeSecurityError::ArchiveCorrupted {
            path: archive_path.to_path_buf(),
            reason: e.to_string(),
        })?;

    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| ThemeSecurityError::ArchiveCorrupted {
            path: archive_path.to_path_buf(),
            reason: e.to_string(),
        })?;

        entry_count = entry_count.saturating_add(1);
        if entry_count > MAX_ENTRIES_COUNT {
            return Err(ThemeSecurityError::ResourceLimitExceeded {
                reason: format!("归档内条目数量超出安全上限 ({} 个)", MAX_ENTRIES_COUNT),
            });
        }

        let entry_type = entry.header().entry_type();
        let path_bytes = entry.path_bytes();
        let path_str = String::from_utf8_lossy(&path_bytes).to_string();

        // 仅允许普通文件与目录，坚决拒绝符号链接、硬链接、设备节点等
        match entry_type {
            EntryType::Regular | EntryType::Continuous => {
                let dest_path = validate_entry_path(target_dir, &path_str)?;
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| ThemeSecurityError::IoError {
                        action: "创建父级目录".to_string(),
                        path: parent.to_path_buf(),
                        reason: e.to_string(),
                    })?;
                }

                let mut out =
                    File::create(&dest_path).map_err(|e| ThemeSecurityError::IoError {
                        action: "创建解压文件".to_string(),
                        path: dest_path.clone(),
                        reason: e.to_string(),
                    })?;

                let mut limited_reader = (&mut entry).take(MAX_SINGLE_FILE_SIZE + 1);
                let written = io::copy(&mut limited_reader, &mut out).map_err(|e| {
                    ThemeSecurityError::IoError {
                        action: "写入解压文件".to_string(),
                        path: dest_path.clone(),
                        reason: e.to_string(),
                    }
                })?;

                if written > MAX_SINGLE_FILE_SIZE {
                    return Err(ThemeSecurityError::ResourceLimitExceeded {
                        reason: format!("条目 '{}' 解压大小超出单文件安全上限 (100 MB)", path_str),
                    });
                }

                total_uncompressed = total_uncompressed.saturating_add(written);
                if total_uncompressed > MAX_TOTAL_UNCOMPRESSED_SIZE {
                    return Err(ThemeSecurityError::ResourceLimitExceeded {
                        reason: format!(
                            "压缩包解压总容量超出安全上限 (200 MB)，已解压 {} 字节",
                            total_uncompressed
                        ),
                    });
                }
            }
            EntryType::Directory => {
                let dest_path = validate_entry_path(target_dir, &path_str)?;
                fs::create_dir_all(&dest_path).map_err(|e| ThemeSecurityError::IoError {
                    action: "创建目录".to_string(),
                    path: dest_path,
                    reason: e.to_string(),
                })?;
            }
            EntryType::Symlink => {
                return Err(ThemeSecurityError::InsecureEntryType {
                    entry_path: path_str,
                    entry_type: "符号链接 (Symbolic Link)".to_string(),
                });
            }
            EntryType::Link => {
                return Err(ThemeSecurityError::InsecureEntryType {
                    entry_path: path_str,
                    entry_type: "硬链接 (Hard Link)".to_string(),
                });
            }
            other => {
                return Err(ThemeSecurityError::InsecureEntryType {
                    entry_path: path_str,
                    entry_type: format!("特殊文件类型: {:?}", other),
                });
            }
        }
    }

    Ok(())
}

/// 在解压目录中智能定位包含 `theme.txt` 的实际主题根目录
///
/// # 适用场景
/// 许多第三方 GRUB 主题压缩包在打包时带有外层根目录（如 `vimix-theme-master/theme.txt`），
/// 本函数可自适应向下探测（深度 ≤ 2），自动剔除冗余外层目录包装。
pub fn find_theme_root_in_dir(dir: &Path) -> Result<PathBuf, ThemeSecurityError> {
    // 1. 检查根目录下是否直接存在 theme.txt
    if dir.join("theme.txt").is_file() {
        return Ok(dir.to_path_buf());
    }

    // 2. 探寻深度为 1 的直接子目录
    let mut candidates = Vec::new();
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            if let Ok(ft) = entry.file_type()
                && ft.is_dir()
            {
                let sub_dir = entry.path();
                if sub_dir.join("theme.txt").is_file() {
                    candidates.push(sub_dir);
                }
            }
        }
    }

    if candidates.len() == 1 {
        return Ok(candidates.remove(0));
    } else if candidates.len() > 1 {
        // 多个子目录均包含 theme.txt，默认返回第一个
        return Ok(candidates.remove(0));
    }

    // 3. 探寻深度为 2 的二级子目录（例如 repo-master/themes/theme.txt）
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            if let Ok(ft) = entry.file_type()
                && ft.is_dir()
            {
                let sub_dir = entry.path();
                if let Ok(sub_read) = fs::read_dir(&sub_dir) {
                    for sub_entry in sub_read.flatten() {
                        if let Ok(sub_ft) = sub_entry.file_type()
                            && sub_ft.is_dir()
                        {
                            let sub_sub_dir = sub_entry.path();
                            if sub_sub_dir.join("theme.txt").is_file() {
                                return Ok(sub_sub_dir);
                            }
                        }
                    }
                }
            }
        }
    }

    Err(ThemeSecurityError::MissingThemeDescriptor {
        theme_dir: dir.to_path_buf(),
    })
}
