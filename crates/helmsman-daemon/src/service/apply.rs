use crate::audit::{AuditAction, AuditEvent, record_audit_event, resolve_caller_uid};
use crate::service::apply_support::execute_update_with_rollback;
use crate::service::{DaemonError, GrubService, TransactionOptions, TransactionResult};
use grub_transaction_engine::{
    atomic_write, check_disk_space, check_package_manager_locks, create_snapshot,
    generate_unified_diff,
};
use std::fs;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info};

/// 事务执行与配置提交相关实现
impl GrubService {
    /// 执行配置提交事务（审计 UID 取自进程环境回退）
    ///
    /// D-Bus 服务端请改用 [`Self::apply_changes_as`]。
    ///
    /// # Errors
    /// 当文件不存在、快照创建失败或写入失败时返回对应的 [`DaemonError`]。
    pub fn apply_changes(
        &self,
        new_config: &str,
        reason: &str,
        options: &TransactionOptions,
    ) -> Result<TransactionResult, DaemonError> {
        self.apply_changes_as(new_config, reason, options, resolve_caller_uid())
    }

    /// 以显式调用方 UID 执行配置提交事务
    ///
    /// # Errors
    /// 当文件不存在、快照创建失败或写入失败时返回对应的 [`DaemonError`]。
    pub fn apply_changes_as(
        &self,
        new_config: &str,
        reason: &str,
        options: &TransactionOptions,
        caller_uid: u32,
    ) -> Result<TransactionResult, DaemonError> {
        let start_time = Instant::now();
        let diff_summary = fs::read_to_string(&self.default_config_path)
            .ok()
            .map(|orig| {
                let diff = generate_unified_diff(&orig, new_config);
                let added = diff
                    .diff_text
                    .lines()
                    .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                    .count();
                let removed = diff
                    .diff_text
                    .lines()
                    .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                    .count();
                format!("+{} / -{}", added, removed)
            });

        let res = self.run_apply_transaction(new_config, reason, options);

        let (success, snapshot_id) = match &res {
            Ok(r) => (r.success, Some(r.snapshot_id.clone())),
            Err(_) => (false, None),
        };

        record_audit_event(&AuditEvent {
            caller_uid,
            action: AuditAction::ApplyChanges,
            reason: reason.to_string(),
            snapshot_id,
            success,
            duration: start_time.elapsed(),
            diff_summary,
        });

        res
    }

    /// 执行快照、原子写入与引导更新的核心事务流程
    fn run_apply_transaction(
        &self,
        new_config: &str,
        reason: &str,
        options: &TransactionOptions,
    ) -> Result<TransactionResult, DaemonError> {
        if !self.default_config_path.exists() {
            return Err(DaemonError::ConfigNotFound {
                path: self.default_config_path.clone(),
            });
        }

        check_package_manager_locks(&self.lock_descriptors).map_err(|e| {
            DaemonError::PackageManagerLocked {
                message: e.to_string(),
            }
        })?;

        let min_disk_space_bytes = 10 * 1024 * 1024;
        check_disk_space(&self.default_config_path, min_disk_space_bytes)?;

        let target_boot_path = Path::new(&self.distro_profile.config_path);
        if target_boot_path.exists() {
            check_disk_space(target_boot_path, min_disk_space_bytes)?;
        }

        debug!("开始准备配置变更事务，原因: {}", reason);

        let snapshot = create_snapshot(&self.default_config_path, &self.backup_dir, reason)
            .map_err(|e| DaemonError::SnapshotFailed {
                reason: e.to_string(),
            })?;

        if let Err(e) = atomic_write(&self.default_config_path, new_config) {
            return Err(DaemonError::AtomicWriteFailed {
                reason: e.to_string(),
            });
        }
        debug!("新配置原子替换成功，待触发引导更新");

        if options.skip_command_execution {
            info!("跳过引导命令执行（模拟测试模式），快照 ID: {}", snapshot.id);
            return Ok(TransactionResult {
                success: true,
                snapshot_id: snapshot.id,
                log_output: "跳过引导命令执行（模拟测试模式）".to_string(),
                error_message: None,
            });
        }

        execute_update_with_rollback(self, &snapshot, options)
    }
}
