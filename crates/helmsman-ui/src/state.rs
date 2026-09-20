use grub_boot_reader::{BootEntry, MenuNode, parse_grub_cfg};
use grub_config_parser::{GrubConfigFile, parse_grub_config};
use grub_transaction_engine::{DiffReport, generate_unified_diff};

/// 前端主应用状态机
///
/// # 设计原理
/// - **实现初衷**：采用只读基准 (`original_config`) 与用户操作草稿 (`draft_config`) 的双缓冲机制。
///   普通用户在界面上的任何频繁操作（如拖拽、调整倒计时、切换开关）均在内存草稿中即时生效，
///   完全杜绝在未确认前触发任何高危的磁盘特权写入。
/// - **核心优势**：状态隔离清晰，可毫秒级判定 `has_unsaved_changes()` 并即时生成 Unified Diff；
///   支持用户随时一键“放弃修改”还原至初始加载状态。
/// - **代价与局限**：草稿状态驻留于客户端进程内存中，若客户端意外崩溃未保存的草稿会丢失（通过脏状态退出拦截保护）。
#[derive(Debug, Clone)]
pub struct AppState {
    /// 磁盘原始配置（只读基准）
    pub original_config: GrubConfigFile,
    /// 用户编辑中的草稿配置
    pub draft_config: GrubConfigFile,
    /// 当前系统引导项树
    pub menu_nodes: Vec<MenuNode>,
    /// 当前在列表中高亮选中的条目路径（用于右侧属性检查器）
    pub selected_entry_path: Option<String>,
}

impl AppState {
    /// 基于文本内容初始化状态（便于单元测试与无特权启动）
    pub fn new_from_content(default_grub_content: &str, grub_cfg_content: &str) -> Self {
        let original_config = parse_grub_config(default_grub_content);
        let draft_config = original_config.clone();
        let menu_nodes = parse_grub_cfg(grub_cfg_content);

        // 默认选中第一个可用条目
        let first_entry = menu_nodes.first().and_then(|node| match node {
            MenuNode::Entry(e) => Some(e.full_path.clone()),
            MenuNode::Submenu { children, .. } => children.first().and_then(|c| match c {
                MenuNode::Entry(e) => Some(e.full_path.clone()),
                _ => None,
            }),
        });

        Self {
            original_config,
            draft_config,
            menu_nodes,
            selected_entry_path: first_entry,
        }
    }

    /// 检查是否有未保存的更改
    pub fn has_unsaved_changes(&self) -> bool {
        self.original_config != self.draft_config
    }

    /// 生成待保存的 Diff 差异报告
    pub fn compute_diff(&self) -> DiffReport {
        let orig_str = self.original_config.serialize();
        let draft_str = self.draft_config.serialize();
        generate_unified_diff(&orig_str, &draft_str)
    }

    /// 设置默认启动项
    pub fn set_default_entry(&mut self, full_path: &str) {
        self.draft_config.set("GRUB_DEFAULT", full_path);
    }

    /// 获取当前设置的默认启动项
    pub fn get_default_entry(&self) -> Option<&str> {
        self.draft_config.get("GRUB_DEFAULT")
    }

    /// 设置倒计时秒数
    pub fn set_timeout(&mut self, timeout_seconds: i32) {
        self.draft_config
            .set("GRUB_TIMEOUT", &timeout_seconds.to_string());
    }

    /// 获取倒计时秒数
    pub fn get_timeout(&self) -> i32 {
        self.draft_config
            .get("GRUB_TIMEOUT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5)
    }

    /// 设置全局内核参数
    pub fn set_cmdline_default(&mut self, cmdline: &str) {
        self.draft_config.set("GRUB_CMDLINE_LINUX_DEFAULT", cmdline);
    }

    /// 获取全局内核参数
    pub fn get_cmdline_default(&self) -> &str {
        self.draft_config
            .get("GRUB_CMDLINE_LINUX_DEFAULT")
            .unwrap_or("")
    }

    /// 切换 os-prober 探测
    pub fn set_os_prober_enabled(&mut self, enabled: bool) {
        if enabled {
            self.draft_config.remove("GRUB_DISABLE_OS_PROBER");
        } else {
            self.draft_config.set("GRUB_DISABLE_OS_PROBER", "true");
        }
    }

    /// 设置菜单分辨率
    pub fn set_gfxmode(&mut self, mode: &str) {
        self.draft_config.set("GRUB_GFXMODE", mode);
    }

    /// 选中特定引导项（联动右侧检查器）
    pub fn select_entry(&mut self, full_path: &str) {
        self.selected_entry_path = Some(full_path.to_string());
    }

    /// 获取当前选中的引导项详情
    pub fn get_selected_entry(&self) -> Option<&BootEntry> {
        let target_path = self.selected_entry_path.as_deref()?;
        let mut entries = Vec::with_capacity(16);
        for node in &self.menu_nodes {
            node.collect_entries_into(&mut entries);
        }
        entries.into_iter().find(|e| e.full_path == target_path)
    }
}
