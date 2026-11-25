use crate::executor::SafeCommand;
use crate::service::{
    DEFAULT_UPDATE_TIMEOUT_SECS, DaemonError, GrubService, TransactionOptions, TransactionResult,
};
use grub_boot_reader::{MenuLayout, apply_menu_layout_to_grub_cfg};
use grub_transaction_engine::{SnapshotMeta, atomic_write, restore_snapshot};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// 语法校验与引导更新命令执行（含失败自动回滚）
pub(crate) fn verify_grub_script_syntax(
    service: &GrubService,
    config_path: &Path,
) -> Result<(), String> {
    let cmd = SafeCommand::new(&service.distro_profile.check_command)
        .and_then(|c| c.args(&service.distro_profile.check_command_args))
        .and_then(|c| c.arg(&config_path.to_string_lossy()))
        .map_err(|e| format!("安全检查拦截: {}", e))?;

    let output = cmd.output().map_err(|e| {
        format!(
            "启动语法检查命令 '{}' 失败: {}",
            service.distro_profile.check_command, e
        )
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("引导脚本语法校验未通过: {}", stderr.trim()))
    }
}

/// 执行引导更新命令并在失败或语法不通过时自动触发快照回滚
pub(crate) fn execute_update_with_rollback(
    service: &GrubService,
    snapshot: &SnapshotMeta,
    options: &TransactionOptions,
) -> Result<TransactionResult, DaemonError> {
    let cmd = match SafeCommand::new(&service.distro_profile.update_command)
        .and_then(|c| c.args(&service.distro_profile.command_args))
    {
        Ok(c) => c,
        Err(e) => {
            let _ = restore_snapshot(snapshot);
            return Err(DaemonError::SecurityCheckFailed(e));
        }
    };

    let timeout = std::time::Duration::from_secs(
        options
            .timeout_seconds
            .unwrap_or(DEFAULT_UPDATE_TIMEOUT_SECS),
    );
    match cmd.output_with_timeout(timeout) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined_log = format!("{}\n{}", stdout, stderr);

            if output.status.success() {
                let target_path = Path::new(&service.distro_profile.config_path);
                if target_path.exists()
                    && let Err(layout_err) = apply_persisted_menu_layout(service, target_path)
                {
                    let _ = restore_snapshot(snapshot);
                    return Err(DaemonError::CommandLaunchFailed {
                        command: service.distro_profile.update_command.clone(),
                        reason: format!("应用菜单编排布局失败（已自动回滚）: {layout_err}"),
                    });
                }

                if !options.skip_syntax_check
                    && target_path.exists()
                    && let Err(syntax_err) = verify_grub_script_syntax(service, target_path)
                {
                    warn!(
                        "引导脚本语法校验未通过，触发自动回滚，快照 ID: {}，原因: {}",
                        snapshot.id, syntax_err
                    );
                    let rollback_err = restore_snapshot(snapshot)
                        .err()
                        .map(|e| format!("且自动回滚失败: {}", e))
                        .unwrap_or_else(|| "已成功自动回滚至初始状态".to_string());

                    return Err(DaemonError::CommandLaunchFailed {
                        command: service.distro_profile.update_command.clone(),
                        reason: format!("{}, {}", syntax_err, rollback_err),
                    });
                }

                info!(
                    "引导更新命令执行成功且语法校验通过，快照 ID: {}",
                    snapshot.id
                );
                Ok(TransactionResult {
                    success: true,
                    snapshot_id: snapshot.id.clone(),
                    log_output: combined_log,
                    error_message: None,
                })
            } else {
                warn!(
                    "引导生成命令退出码非零 ({:?})，触发自动回滚，快照 ID: {}",
                    output.status.code(),
                    snapshot.id
                );
                let rollback_err = restore_snapshot(snapshot)
                    .err()
                    .map(|e| format!("且自动回滚失败: {}", e))
                    .unwrap_or_else(|| "已成功自动回滚至初始状态".to_string());

                Err(DaemonError::CommandLaunchFailed {
                    command: service.distro_profile.update_command.clone(),
                    reason: format!(
                        "引导生成命令退出码非零 ({:?})，{}",
                        output.status.code(),
                        rollback_err
                    ),
                })
            }
        }
        Err(e) => {
            warn!("更新命令执行失败或超时，触发自动回滚，原因: {}", e);
            let _ = restore_snapshot(snapshot);
            Err(DaemonError::CommandLaunchFailed {
                command: service.distro_profile.update_command.clone(),
                reason: format!("{}（已自动回滚）", e),
            })
        }
    }
}

/// 若存在非空菜单编排规则，则在 `update-grub` 生成后非破坏性地重排 `grub.cfg`
fn apply_persisted_menu_layout(service: &GrubService, target_path: &Path) -> Result<(), String> {
    if !service.menu_layout_path.exists() {
        return Ok(());
    }
    let raw_json = fs::read_to_string(&service.menu_layout_path)
        .map_err(|e| format!("读取菜单布局配置失败: {e}"))?;
    let layout: MenuLayout =
        serde_json::from_str(&raw_json).map_err(|e| format!("解析菜单布局配置失败: {e}"))?;
    if layout.is_empty() {
        return Ok(());
    }
    let cfg_text =
        fs::read_to_string(target_path).map_err(|e| format!("读取引导菜单文件失败: {e}"))?;
    let arranged = apply_menu_layout_to_grub_cfg(&cfg_text, &layout);
    atomic_write(target_path, &arranged).map_err(|e| format!("原子写回重排后的引导菜单失败: {e}"))
}
