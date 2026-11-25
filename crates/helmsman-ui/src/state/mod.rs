mod custom;
mod display;
mod layout;

use crate::entry_view::BootEntryItem;
use crate::theme::{FontScale, ThemeMode};
use grub_boot_reader::{BootEntry, CustomBootEntry, MenuLayout, MenuNode, parse_grub_cfg};
use grub_config_parser::{
    BootKeyError, CmdlineError, DefaultEntry, DiffReport, GrubConfigFile, KernelCmdline,
    TimeoutSeconds, TimeoutStyle, default_entry_from_index, default_entry_from_title,
    generate_unified_diff, get_cmdline_default, get_cmdline_linux, get_default_entry, get_timeout,
    get_timeout_style, parse_grub_config, set_cmdline_default as cfg_set_cmdline_default,
    set_cmdline_linux as cfg_set_cmdline_linux, set_default_entry as cfg_set_default_entry,
    set_timeout as cfg_set_timeout, set_timeout_style as cfg_set_timeout_style,
};
use std::collections::HashMap;

/// 草稿撤销历史最大深度（防止连续操作无限占用内存）
const MAX_UNDO_HISTORY: usize = 32;

/// 内存草稿撤销快照
#[derive(Debug, Clone)]
pub struct DraftSnapshot {
    pub config: GrubConfigFile,
    pub custom_entries: Vec<CustomBootEntry>,
    pub menu_layout: MenuLayout,
}

/// 前端主应用状态机
///
/// # 设计原理
/// - **实现初衷**：采用只读基准与用户操作草稿的双缓冲机制。普通用户在界面上的任何频繁操作
///   （如添加/删除自定义条目、为条目重命名、调整倒计时、切换原生过滤开关）均在内存草稿中即时生效，
///   完全杜绝在确认提交前触发任何高危的磁盘特权写入。
/// - **核心优势**：状态隔离清晰，支持一键放弃修改恢复初始状态；
///   将自定义脚本草稿、条目别名映射与原生 GRUB 过滤开关全面纳入统一状态机纳管。
#[derive(Debug, Clone)]
pub struct AppState {
    /// 磁盘原始配置（只读基准）
    pub original_config: GrubConfigFile,
    /// 用户编辑中的草稿配置
    pub draft_config: GrubConfigFile,
    /// 当前系统引导项树
    pub menu_nodes: Vec<MenuNode>,
    /// 原始自定义引导条目列表
    pub original_custom_entries: Vec<CustomBootEntry>,
    /// 用户编辑中的自定义引导条目草稿
    pub draft_custom_entries: Vec<CustomBootEntry>,
    /// 磁盘原始菜单编排布局（只读基准）
    pub original_menu_layout: MenuLayout,
    /// 用户编辑中的菜单编排布局草稿
    pub draft_menu_layout: MenuLayout,
    /// 条目友好别名映射表 (entry_id -> alias)
    pub aliases: HashMap<String, String>,
    /// 当前在列表中高亮选中的条目路径（用于右侧属性检查器）
    pub selected_entry_path: Option<String>,
    /// 界面视觉主题偏好
    pub theme_mode: ThemeMode,
    /// 界面字体缩放级别
    pub font_scale: FontScale,
    /// 草稿撤销栈（用于 Ctrl+Z 撤销操作）
    undo_stack: Vec<DraftSnapshot>,
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
            original_custom_entries: Vec::new(),
            draft_custom_entries: Vec::new(),
            original_menu_layout: MenuLayout::default(),
            draft_menu_layout: MenuLayout::default(),
            aliases: HashMap::new(),
            selected_entry_path: first_entry,
            theme_mode: ThemeMode::default(),
            font_scale: FontScale::default(),
            undo_stack: Vec::new(),
        }
    }

    /// 注入自定义引导项列表
    pub fn with_custom_entries(mut self, entries: Vec<CustomBootEntry>) -> Self {
        self.original_custom_entries = entries.clone();
        self.draft_custom_entries = entries;
        self
    }

    /// 注入条目别名映射表
    pub fn with_aliases(mut self, aliases: HashMap<String, String>) -> Self {
        self.aliases = aliases;
        self
    }

    /// 注入菜单编排布局
    pub fn with_menu_layout(mut self, layout: MenuLayout) -> Self {
        self.original_menu_layout = layout.clone();
        self.draft_menu_layout = layout;
        self
    }

    /// 检查是否有未保存的更改（包括常规配置、自定义引导项与菜单编排布局）
    pub fn has_unsaved_changes(&self) -> bool {
        self.original_config != self.draft_config
            || self.original_custom_entries != self.draft_custom_entries
            || self.original_menu_layout != self.draft_menu_layout
    }

    /// 检查自定义引导项是否有草稿变更
    pub fn has_custom_entry_changes(&self) -> bool {
        self.original_custom_entries != self.draft_custom_entries
    }

    /// 放弃所有未保存修改，一键重置草稿为原始基准配置
    pub fn reset_draft(&mut self) {
        self.draft_config = self.original_config.clone();
        self.draft_custom_entries = self.original_custom_entries.clone();
        self.draft_menu_layout = self.original_menu_layout.clone();
        self.undo_stack.clear();
    }

    /// 记录当前草稿快照至撤销栈（在执行用户编辑操作前调用）
    pub fn push_undo_snapshot(&mut self) {
        if self.undo_stack.len() >= MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(DraftSnapshot {
            config: self.draft_config.clone(),
            custom_entries: self.draft_custom_entries.clone(),
            menu_layout: self.draft_menu_layout.clone(),
        });
    }

    /// 撤销最近一次草稿操作（Ctrl+Z）
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo_stack.pop() else {
            return false;
        };
        self.draft_config = prev.config;
        self.draft_custom_entries = prev.custom_entries;
        self.draft_menu_layout = prev.menu_layout;
        true
    }

    /// 是否存在可撤销的草稿操作
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 获取当前界面的扁平条目单选模型列表（结合别名、过滤、自定义项与布局编排）
    pub fn get_entry_items(&self) -> Vec<BootEntryItem> {
        let mut items: Vec<BootEntryItem> = self
            .collect_unfiltered_items()
            .into_iter()
            .filter(|item| !self.draft_menu_layout.hidden.contains(&item.full_path))
            .map(|mut item| {
                if let Some(sub_override) = self
                    .draft_menu_layout
                    .submenu_overrides
                    .get(&item.full_path)
                {
                    if let Some(title_override) =
                        self.draft_menu_layout.title_overrides.get(&item.full_path)
                        && !title_override.trim().is_empty()
                    {
                        item.display_title = title_override.trim().to_string();
                    }
                    item.submenu_name = if sub_override.trim().is_empty() {
                        None
                    } else {
                        Some(sub_override.trim().to_string())
                    };
                }
                if let Some(title_override) =
                    self.draft_menu_layout.title_overrides.get(&item.full_path)
                    && !title_override.trim().is_empty()
                {
                    item.display_title = title_override.trim().to_string();
                }
                item
            })
            .collect();

        if !self.draft_menu_layout.order.is_empty() {
            items.sort_by_key(|item| {
                self.draft_menu_layout
                    .order
                    .iter()
                    .position(|k| k == &item.full_path)
                    .unwrap_or(usize::MAX)
            });
        }

        items
    }

    /// 生成待保存的 Diff 差异报告
    pub fn compute_diff(&self) -> DiffReport {
        let orig_str = self.original_config.serialize();
        let draft_str = self.draft_config.serialize();
        generate_unified_diff(&orig_str, &draft_str)
    }

    /// 设置默认启动项（标题、层级路径或纯数字语义索引）
    ///
    /// # Errors
    /// 当取值非法（空标题、换行注入等）时返回 [`BootKeyError`]。
    pub fn set_default_entry(&mut self, full_path: &str) -> Result<(), BootKeyError> {
        let entry = DefaultEntry::parse(full_path)?;
        cfg_set_default_entry(&mut self.draft_config, &entry)
    }

    /// 按菜单逻辑序号设置默认启动项（原生 GRUB_DEFAULT 语义索引）
    ///
    /// # Errors
    /// 仅当配置层校验失败时返回 [`BootKeyError`]。
    pub fn set_default_entry_by_index(&mut self, index: u32) -> Result<(), BootKeyError> {
        cfg_set_default_entry(&mut self.draft_config, &default_entry_from_index(index))
    }

    /// 将完整路径解析为菜单逻辑序号后写入语义索引
    ///
    /// 菜单扁平顺序与 GRUB 生成顺序一致；若无法解析路径则回退为标题写入。
    ///
    /// # Errors
    /// 当标题形态非法时返回 [`BootKeyError`]。
    pub fn set_default_entry_semantic(&mut self, full_path: &str) -> Result<(), BootKeyError> {
        if let Some(index) = self.resolve_menu_index(full_path) {
            return self.set_default_entry_by_index(index);
        }
        cfg_set_default_entry(
            &mut self.draft_config,
            &default_entry_from_title(full_path)?,
        )
    }

    /// 解析条目完整路径对应的菜单逻辑序号（零堆分配树遍历）
    pub fn resolve_menu_index(&self, full_path: &str) -> Option<u32> {
        let mut idx = 0u32;
        find_entry_index_in_nodes(&self.menu_nodes, full_path, &mut idx)
    }

    /// 获取当前设置的默认启动项原始值
    pub fn get_default_entry(&self) -> Option<&str> {
        self.draft_config.get("GRUB_DEFAULT")
    }

    /// 获取当前设置的默认启动项语义形态
    ///
    /// # Errors
    /// 当已配置值非法时返回 [`BootKeyError`]。
    pub fn get_default_entry_typed(&self) -> Result<Option<DefaultEntry>, BootKeyError> {
        get_default_entry(&self.draft_config)
    }

    /// 检查当前系统配置是否启用了 saved 快速引导模式
    pub fn is_saved_default_enabled(&self) -> bool {
        self.draft_config.get("GRUB_DEFAULT") == Some("saved")
    }

    /// 启用 saved 快速引导模式
    pub fn enable_saved_default_mode(&mut self) {
        let _ = cfg_set_default_entry(&mut self.draft_config, &DefaultEntry::Saved);
        self.draft_config.set("GRUB_SAVEDEFAULT", "true");
    }

    /// 设置倒计时秒数（-1 表示无限等待）
    ///
    /// # Errors
    /// 当秒数超出合法范围时返回 [`BootKeyError`]。
    pub fn set_timeout(&mut self, timeout_seconds: i32) -> Result<(), BootKeyError> {
        let timeout = TimeoutSeconds::try_new(timeout_seconds)?;
        cfg_set_timeout(&mut self.draft_config, timeout)
    }

    /// 获取倒计时秒数（配置非法或缺失时回退为 5）
    pub fn get_timeout(&self) -> i32 {
        get_timeout(&self.draft_config)
            .map(|t| t.as_i32())
            .unwrap_or(5)
    }

    /// 获取倒计时秒数语义值
    ///
    /// # Errors
    /// 当已配置值非法时返回 [`BootKeyError`]。
    pub fn get_timeout_typed(&self) -> Result<TimeoutSeconds, BootKeyError> {
        get_timeout(&self.draft_config)
    }

    /// 设置倒计时显示风格
    ///
    /// # Errors
    /// 当风格枚举序列化异常时返回 [`BootKeyError`]。
    pub fn set_timeout_style(&mut self, style: TimeoutStyle) -> Result<(), BootKeyError> {
        cfg_set_timeout_style(&mut self.draft_config, style)
    }

    /// 获取倒计时显示风格（配置非法时回退为 menu）
    pub fn get_timeout_style(&self) -> TimeoutStyle {
        get_timeout_style(&self.draft_config).unwrap_or(TimeoutStyle::Menu)
    }

    /// 获取倒计时显示风格原始字符串
    pub fn get_timeout_style_raw(&self) -> &str {
        self.draft_config
            .get("GRUB_TIMEOUT_STYLE")
            .unwrap_or("menu")
    }

    /// 设置基础内核参数（GRUB_CMDLINE_LINUX）
    ///
    /// # Errors
    /// 当参数串含非法令牌时返回 [`CmdlineError`]。
    pub fn set_cmdline_linux(&mut self, cmdline: &str) -> Result<(), CmdlineError> {
        let parsed = KernelCmdline::parse(cmdline)?;
        cfg_set_cmdline_linux(&mut self.draft_config, &parsed)
    }

    /// 获取基础内核参数
    pub fn get_cmdline_linux(&self) -> &str {
        self.draft_config.get("GRUB_CMDLINE_LINUX").unwrap_or("")
    }

    /// 获取基础内核参数类型化视图
    ///
    /// # Errors
    /// 当已配置值非法时返回 [`CmdlineError`]。
    pub fn get_cmdline_linux_typed(&self) -> Result<KernelCmdline, CmdlineError> {
        get_cmdline_linux(&self.draft_config)
    }

    /// 设置全局内核参数（GRUB_CMDLINE_LINUX_DEFAULT）
    ///
    /// # Errors
    /// 当参数串含非法令牌时返回 [`CmdlineError`]。
    pub fn set_cmdline_default(&mut self, cmdline: &str) -> Result<(), CmdlineError> {
        let parsed = KernelCmdline::parse(cmdline)?;
        cfg_set_cmdline_default(&mut self.draft_config, &parsed)
    }

    /// 获取全局内核参数
    pub fn get_cmdline_default(&self) -> &str {
        self.draft_config
            .get("GRUB_CMDLINE_LINUX_DEFAULT")
            .unwrap_or("")
    }

    /// 获取全局内核参数类型化视图
    ///
    /// # Errors
    /// 当已配置值非法时返回 [`CmdlineError`]。
    pub fn get_cmdline_default_typed(&self) -> Result<KernelCmdline, CmdlineError> {
        get_cmdline_default(&self.draft_config)
    }

    /// 启停全局内核参数中的常见开关（quiet / splash / nomodeset 等）
    ///
    /// # Errors
    /// 当已配置值非法或开关名非法时返回 [`CmdlineError`]。
    pub fn set_cmdline_default_flag(
        &mut self,
        flag: &str,
        enabled: bool,
    ) -> Result<(), CmdlineError> {
        let mut cmdline = get_cmdline_default(&self.draft_config)?;
        if enabled {
            cmdline.enable_flag(flag)?;
        } else {
            cmdline.disable_flag(flag);
        }
        cfg_set_cmdline_default(&mut self.draft_config, &cmdline)
    }

    /// 选中特定引导项（联动右侧检查器）
    pub fn select_entry(&mut self, full_path: &str) {
        self.selected_entry_path = Some(full_path.to_string());
    }

    /// 获取当前选中的引导项详情（零堆分配树查找）
    pub fn get_selected_entry(&self) -> Option<&BootEntry> {
        let target_path = self.selected_entry_path.as_deref()?;
        find_entry_by_path(&self.menu_nodes, target_path)
    }
}

fn find_entry_index_in_nodes(nodes: &[MenuNode], target: &str, counter: &mut u32) -> Option<u32> {
    for node in nodes {
        match node {
            MenuNode::Entry(e) => {
                if e.full_path == target || e.title == target {
                    return Some(*counter);
                }
                *counter = counter.saturating_add(1);
            }
            MenuNode::Submenu { children, .. } => {
                if let Some(found) = find_entry_index_in_nodes(children, target, counter) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn find_entry_by_path<'a>(nodes: &'a [MenuNode], target_path: &str) -> Option<&'a BootEntry> {
    for node in nodes {
        match node {
            MenuNode::Entry(e) if e.full_path == target_path => return Some(e),
            MenuNode::Entry(_) => {}
            MenuNode::Submenu { children, .. } => {
                if let Some(found) = find_entry_by_path(children, target_path) {
                    return Some(found);
                }
            }
        }
    }
    None
}

pub(crate) fn any_entry_matches(nodes: &[MenuNode], target: &str) -> bool {
    for node in nodes {
        match node {
            MenuNode::Entry(e) if e.full_path == target || e.title == target => return true,
            MenuNode::Entry(_) => {}
            MenuNode::Submenu { children, .. } if any_entry_matches(children, target) => {
                return true;
            }
            MenuNode::Submenu { .. } => {}
        }
    }
    false
}
