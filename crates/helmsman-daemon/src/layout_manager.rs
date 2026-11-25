use crate::audit::{AuditAction, with_audit};
use crate::custom_manager::CustomManager;
use crate::service::apply_support::execute_update_with_rollback;
use crate::service::{
    DEFAULT_SNAPSHOT_RETENTION, DaemonError, GrubService, TransactionOptions, TransactionResult,
};
use grub_boot_reader::MenuLayout;
use grub_transaction_engine::{atomic_write, create_snapshot, prune_snapshots};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

impl CustomManager {
    /// 获取菜单布局编排 JSON 文件路径
    pub fn menu_layout_path(&self) -> PathBuf {
        self.aliases_path
            .parent()
            .map(|dir| dir.join("menu_layout.json"))
            .unwrap_or_else(|| PathBuf::from("/etc/helmsman/menu_layout.json"))
    }

    /// 读取持久化的全量菜单布局规则（缺失或损坏时回退为空布局）
    pub fn load_menu_layout(&self) -> MenuLayout {
        let path = self.menu_layout_path();
        let Ok(content) = fs::read_to_string(&path) else {
            return MenuLayout::default();
        };
        serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!("菜单布局配置解析失败，已忽略: {}", e);
            MenuLayout::default()
        })
    }

    /// 保存全量菜单布局规则并触发引导菜单重排更新（含原子快照与失败自动回滚）
    pub fn save_menu_layout_as(
        &self,
        service: &GrubService,
        layout: &MenuLayout,
        reason: &str,
        options: &TransactionOptions,
        caller_uid: u32,
    ) -> Result<TransactionResult, DaemonError> {
        let _custom_guard = self.acquire_write_lock();
        let _service_guard = service.acquire_write_lock();

        with_audit(
            caller_uid,
            AuditAction::ApplyChanges,
            reason,
            None,
            Some(format!(
                "order={}, hidden={}, overrides={}",
                layout.order.len(),
                layout.hidden.len(),
                layout.submenu_overrides.len()
            )),
            |r: &TransactionResult| (r.success, Some(r.snapshot_id.clone())),
            || {
                let layout_path = self.menu_layout_path();
                if let Some(parent) = layout_path.parent()
                    && !parent.exists()
                {
                    fs::create_dir_all(parent).map_err(|e| DaemonError::AtomicWriteFailed {
                        reason: format!("创建菜单布局目录失败: {e}"),
                    })?;
                }

                let target_cfg = PathBuf::from(&service.distro_profile.config_path);
                let snapshot = if target_cfg.exists() {
                    create_snapshot(&target_cfg, &self.backup_dir, reason)
                } else {
                    create_snapshot(&service.default_config_path, &self.backup_dir, reason)
                }
                .map_err(|e| DaemonError::SnapshotFailed {
                    reason: e.to_string(),
                })?;

                let serialized = serde_json::to_string_pretty(layout).map_err(|e| {
                    DaemonError::AtomicWriteFailed {
                        reason: format!("序列化菜单布局失败: {e}"),
                    }
                })?;
                atomic_write(&layout_path, &serialized).map_err(|e| {
                    DaemonError::AtomicWriteFailed {
                        reason: e.to_string(),
                    }
                })?;
                if service.menu_layout_path != layout_path
                    && let Some(parent) = service.menu_layout_path.parent()
                {
                    let _ = fs::create_dir_all(parent);
                    let _ = atomic_write(&service.menu_layout_path, &serialized);
                }

                let res = execute_update_with_rollback(service, &snapshot, options)?;
                let _ = prune_snapshots(&self.backup_dir, DEFAULT_SNAPSHOT_RETENTION);
                info!(
                    "菜单布局编排提交成功，快照 ID: {}，隐藏 {} 项",
                    res.snapshot_id,
                    layout.hidden.len()
                );
                Ok(res)
            },
        )
    }
}
