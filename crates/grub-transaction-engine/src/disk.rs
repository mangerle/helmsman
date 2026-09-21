use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// 磁盘空间检查错误
#[derive(Debug, PartialEq, Eq)]
pub enum DiskSpaceError {
    /// 目标路径可用空间不足
    InsufficientSpace {
        /// 目标路径
        path: PathBuf,
        /// 当前剩余可用字节数
        available_bytes: u64,
        /// 最低要求可用字节数
        required_bytes: u64,
    },
    /// 查询空间失败
    QueryFailed {
        /// 目标路径
        path: PathBuf,
        /// 失败原因
        reason: String,
    },
}

impl fmt::Display for DiskSpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiskSpaceError::InsufficientSpace {
                path,
                available_bytes,
                required_bytes,
            } => {
                let avail_mb = available_bytes / (1024 * 1024);
                let req_mb = required_bytes / (1024 * 1024);
                write!(
                    f,
                    "目标分区可用空间不足，路径: {}，当前剩余: {} MB，最低要求: {} MB。请清理磁盘空间以防引导文件截断损坏。",
                    path.display(),
                    avail_mb,
                    req_mb
                )
            }
            DiskSpaceError::QueryFailed { path, reason } => {
                write!(
                    f,
                    "查询目标路径可用磁盘空间失败，路径: {}，原因: {}",
                    path.display(),
                    reason
                )
            }
        }
    }
}

impl Error for DiskSpaceError {}

/// 获取指定路径所在分区的可用字节数（100% 纯安全实现）
pub fn get_available_bytes(path: &Path) -> Result<u64, String> {
    get_available_bytes_with_lookup(path, query_system_disk_space)
}

/// 内部支持依赖注入的查询函数
pub(crate) fn get_available_bytes_with_lookup<F>(path: &Path, lookup: F) -> Result<u64, String>
where
    F: Fn(&Path) -> Result<u64, String>,
{
    lookup(path)
}

/// 实际系统空间查询实现（Linux 通过 df 命令安全查询；非 Unix 测试环境默认提供安全充足值）
fn query_system_disk_space(path: &Path) -> Result<u64, String> {
    #[cfg(unix)]
    {
        let output = std::process::Command::new("df")
            .arg("-B1")
            .arg(path)
            .output()
            .map_err(|e| format!("执行 df 命令失败: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("df 命令返回非零: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_df_output(&stdout)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(100 * 1024 * 1024 * 1024)
    }
}

/// 解析 `df -B1` 命令的控制台输出
#[cfg(any(unix, test))]
pub(crate) fn parse_df_output(output: &str) -> Result<u64, String> {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() < 2 {
        return Err("df 输出格式不符合预期（行数过少）".to_string());
    }
    let data_line = lines[1];
    let fields: Vec<&str> = data_line.split_whitespace().collect();
    if fields.len() < 4 {
        return Err(format!("无法解析 df 输出字段: {}", data_line));
    }
    fields[3]
        .parse::<u64>()
        .map_err(|e| format!("解析可用空间字节数失败 '{}': {}", fields[3], e))
}

/// 检查指定路径的可用磁盘空间是否满足最小字节要求
///
/// # 设计原理
/// - **实现初衷**：在许多系统上 `/boot` 是独立小分区，一旦在空间不足时调用 `grub-mkconfig`，
///   外部命令会在写入到一半时抛出磁盘满错误，导致引导文件被截断为空、开机瘫痪。
/// - **核心优势**：在执行任何写入和命令前预检可用空间，提前安全阻断。
///
/// # Errors
/// 当可用空间小于 `min_bytes` 时返回 `DiskSpaceError::InsufficientSpace`。
pub fn check_disk_space(path: &Path, min_bytes: u64) -> Result<(), DiskSpaceError> {
    check_disk_space_with_lookup(path, min_bytes, get_available_bytes)
}

/// 内部支持依赖注入测试的磁盘空间校验函数
pub(crate) fn check_disk_space_with_lookup<F>(
    path: &Path,
    min_bytes: u64,
    lookup: F,
) -> Result<(), DiskSpaceError>
where
    F: Fn(&Path) -> Result<u64, String>,
{
    let available = lookup(path).map_err(|reason| DiskSpaceError::QueryFailed {
        path: path.to_path_buf(),
        reason,
    })?;

    if available < min_bytes {
        return Err(DiskSpaceError::InsufficientSpace {
            path: path.to_path_buf(),
            available_bytes: available,
            required_bytes: min_bytes,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_df_output() {
        let sample = "Filesystem     1B-blocks      Used Available Use% Mounted on\n/dev/nvme0n1p2 511705088 153511936 332036096  32% /boot\n";
        let avail = parse_df_output(sample).unwrap();
        assert_eq!(avail, 332036096);
    }

    #[test]
    fn test_check_disk_space_mock_insufficient() {
        let test_path = Path::new("/boot/grub/grub.cfg");
        // 模拟当前仅剩余 5MB (5 * 1024 * 1024 字节)
        let mock_lookup = |_p: &Path| Ok(5 * 1024 * 1024);

        // 要求至少 10MB
        let res = check_disk_space_with_lookup(test_path, 10 * 1024 * 1024, mock_lookup);
        assert!(res.is_err());
        match res.unwrap_err() {
            DiskSpaceError::InsufficientSpace {
                available_bytes,
                required_bytes,
                ..
            } => {
                assert_eq!(available_bytes, 5 * 1024 * 1024);
                assert_eq!(required_bytes, 10 * 1024 * 1024);
            }
            _ => panic!("预期返回 InsufficientSpace 错误"),
        }
    }

    #[test]
    fn test_check_disk_space_mock_sufficient() {
        let test_path = Path::new("/boot/grub/grub.cfg");
        // 模拟当前剩余 50MB
        let mock_lookup = |_p: &Path| Ok(50 * 1024 * 1024);

        // 要求至少 10MB
        let res = check_disk_space_with_lookup(test_path, 10 * 1024 * 1024, mock_lookup);
        assert!(res.is_ok());
    }
}
