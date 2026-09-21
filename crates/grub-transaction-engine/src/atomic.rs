use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tracing::debug;

/// RAII 临时文件守卫，确保在任何异常、早期返回或 panic 场景下自动清理未提交的临时文件
struct TempFileGuard {
    path: PathBuf,
    committed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.committed && self.path.exists() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// 安全原子替换写入文件
///
/// # 设计原理
/// - **实现初衷**：直接覆盖目标配置文件存在写入中途断电、系统崩溃导致文件损坏或截断为空的风险。
///   原子替换模式通过内核级 `rename` 保证写入要么全成功、要么全失败。
/// - **核心优势**：
///   1. 临时文件创建在目标文件同级父目录下，严格确保与目标文件处于同一文件系统分区，保证 `rename` 具有原子性；
///   2. 显式调用 `sync_all` 强制将内核页缓存刷入持久化介质；
///   3. 借助 `TempFileGuard` 实现 RAII 资源回收，杜绝任何失败分支残留垃圾临时文件。
/// - **代价与局限**：需要目标文件所在目录具备写入权限，且瞬时需要约两倍于目标文件大小的存储空间。
///
/// # Errors
/// - 当父目录无法创建时返回 `io::Error`；
/// - 当临时文件创建、写入或刷盘失败时返回 `io::Error`；
/// - 当原子重命名覆盖操作被系统拒绝时返回 `io::Error`。
pub fn atomic_write(target_path: &Path, content: &str) -> io::Result<()> {
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let temp_name = format!(
        ".tmp_{}_{}",
        std::process::id(),
        target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
    );
    let temp_path = parent.join(temp_name);
    let mut guard = TempFileGuard::new(temp_path.clone());

    // 写入临时文件并刷盘
    {
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    debug!("临时文件写入并刷盘成功: {}", temp_path.display());

    // 执行原子替换
    fs::rename(&temp_path, target_path)?;
    guard.commit();
    debug!(
        "原子替换目标文件完成: {} -> {}",
        temp_path.display(),
        target_path.display()
    );

    Ok(())
}
