use crate::entry_view::{BootEntryItem, flatten_boot_entries_with_options};
use crate::theme::{ColorPalette, FontScale, SystemColorScheme, ThemeMode};
use grub_boot_reader::{BootEntry, CustomBootEntry, MenuNode, parse_grub_cfg};
use grub_config_parser::{
    BootKeyError, CmdlineError, DefaultEntry, DisplayConfigError, GfxMode, GrubColor,
    GrubConfigFile, KernelCmdline, MenuVisibility, TimeoutSeconds, TimeoutStyle,
    default_entry_from_index, default_entry_from_title, display_keys, get_cmdline_default,
    get_cmdline_linux, get_default_entry, get_timeout, get_timeout_style, parse_grub_config,
    set_cmdline_default as cfg_set_cmdline_default, set_cmdline_linux as cfg_set_cmdline_linux,
    set_default_entry as cfg_set_default_entry, set_timeout as cfg_set_timeout,
    set_timeout_style as cfg_set_timeout_style,
};
use grub_transaction_engine::{DiffReport, generate_unified_diff};
use std::collections::HashMap;

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
    /// 条目友好别名映射表 (entry_id -> alias)
    pub aliases: HashMap<String, String>,
    /// 当前在列表中高亮选中的条目路径（用于右侧属性检查器）
    pub selected_entry_path: Option<String>,
    /// 界面视觉主题偏好
    pub theme_mode: ThemeMode,
    /// 界面字体缩放级别
    pub font_scale: FontScale,
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
            aliases: HashMap::new(),
            selected_entry_path: first_entry,
            theme_mode: ThemeMode::default(),
            font_scale: FontScale::default(),
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

    /// 检查是否有未保存的更改（包括常规配置与自定义引导项）
    pub fn has_unsaved_changes(&self) -> bool {
        self.original_config != self.draft_config
            || self.original_custom_entries != self.draft_custom_entries
    }

    /// 检查自定义引导项是否有草稿变更
    pub fn has_custom_entry_changes(&self) -> bool {
        self.original_custom_entries != self.draft_custom_entries
    }

    /// 放弃所有未保存修改，一键重置草稿为原始基准配置
    pub fn reset_draft(&mut self) {
        self.draft_config = self.original_config.clone();
        self.draft_custom_entries = self.original_custom_entries.clone();
    }

    /// 获取当前界面的扁平条目单选模型列表（结合别名、过滤与自定义项）
    pub fn get_entry_items(&self) -> Vec<BootEntryItem> {
        let filter_recovery = self.is_recovery_disabled();
        let mut items = flatten_boot_entries_with_options(
            &self.menu_nodes,
            self.get_default_entry(),
            &self.aliases,
            filter_recovery,
        );

        // 追加已启用的自定义条目
        for custom in &self.draft_custom_entries {
            if custom.enabled {
                items.push(BootEntryItem::from_custom_entry(
                    custom,
                    self.get_default_entry(),
                ));
            }
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

    /// 解析条目完整路径对应的菜单逻辑序号
    pub fn resolve_menu_index(&self, full_path: &str) -> Option<u32> {
        let mut flat = Vec::with_capacity(16);
        for node in &self.menu_nodes {
            flat.extend(node.collect_entries());
        }
        flat.iter()
            .position(|e| e.full_path == full_path || e.title == full_path)
            .map(|idx| idx as u32)
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
        // 与 GRUB 缺省语义一致：未配置时展示 menu
        match self.draft_config.get("GRUB_TIMEOUT_STYLE") {
            Some(raw) => raw,
            None => "menu",
        }
    }

    /// 设置基础内核参数（GRUB_CMDLINE_LINUX）
    ///
    /// 写回前自动去重并校验令牌，保留键原有引号与行尾注释。
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
    /// 写回前自动去重并校验令牌，保留键原有引号与行尾注释。
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

    /// 切换 os-prober 探测
    pub fn set_os_prober_enabled(&mut self, enabled: bool) {
        let mut vis = MenuVisibility::from_config(&self.draft_config);
        vis.os_prober_disabled = !enabled;
        vis.apply_to(&mut self.draft_config);
    }

    /// 检查是否禁用了恢复模式条目
    pub fn is_recovery_disabled(&self) -> bool {
        MenuVisibility::from_config(&self.draft_config).recovery_disabled
    }

    /// 设置是否禁用恢复模式条目（零侵入隐藏恢复模式内核）
    pub fn set_recovery_disabled(&mut self, disabled: bool) {
        let mut vis = MenuVisibility::from_config(&self.draft_config);
        vis.recovery_disabled = disabled;
        vis.apply_to(&mut self.draft_config);
    }

    /// 检查是否禁用了二级子菜单折叠（拉平菜单）
    pub fn is_submenu_disabled(&self) -> bool {
        MenuVisibility::from_config(&self.draft_config).submenu_disabled
    }

    /// 设置是否禁用二级子菜单（零侵入控制菜单折叠与平铺）
    pub fn set_submenu_disabled(&mut self, disabled: bool) {
        let mut vis = MenuVisibility::from_config(&self.draft_config);
        vis.submenu_disabled = disabled;
        vis.apply_to(&mut self.draft_config);
    }

    /// 读取完整类级可见性策略
    pub fn get_menu_visibility(&self) -> MenuVisibility {
        MenuVisibility::from_config(&self.draft_config)
    }

    /// 写入完整类级可见性策略（类级隐藏；单项隐藏不在本接口范围）
    pub fn set_menu_visibility(&mut self, visibility: MenuVisibility) {
        visibility.apply_to(&mut self.draft_config);
    }

    /// 添加自定义引导项草稿
    pub fn add_custom_entry(&mut self, entry: CustomBootEntry) {
        self.draft_custom_entries.push(entry);
    }

    /// 移除自定义引导项草稿
    ///
    /// # 语义说明
    /// 仅作用于 Helmsman 受管自定义项草稿；系统内核/Memtest 等发行版生成条目
    /// **不支持删除**，如需不显示请使用类级隐藏（[`MenuVisibility`]）。
    pub fn remove_custom_entry(&mut self, id: &str) -> bool {
        let initial_len = self.draft_custom_entries.len();
        self.draft_custom_entries.retain(|e| e.id != id);
        self.draft_custom_entries.len() < initial_len
    }

    /// 判断给定条目路径/标题是否属于系统生成条目（不可删除，仅可隐藏）
    pub fn is_system_entry(&self, full_path: &str) -> bool {
        let is_custom = self
            .draft_custom_entries
            .iter()
            .any(|e| e.id == full_path || e.title == full_path);
        if is_custom {
            return false;
        }
        self.menu_nodes
            .iter()
            .flat_map(|n| n.collect_entries())
            .any(|e| e.full_path == full_path || e.title == full_path)
    }

    /// 更新自定义引导项草稿
    pub fn update_custom_entry(&mut self, entry: CustomBootEntry) -> bool {
        for item in &mut self.draft_custom_entries {
            if item.id == entry.id {
                *item = entry;
                return true;
            }
        }
        false
    }

    /// 设置条目友好别名
    pub fn set_entry_alias(&mut self, id_or_title: &str, alias: &str) {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            self.aliases.remove(id_or_title);
        } else {
            self.aliases
                .insert(id_or_title.to_string(), trimmed.to_string());
        }
    }

    /// 获取条目友好别名
    pub fn get_entry_alias(&self, id_or_title: &str) -> Option<&str> {
        self.aliases.get(id_or_title).map(|s| s.as_str())
    }

    /// 设置菜单分辨率（GRUB_GFXMODE）；空串或 auto 跟随固件
    ///
    /// # Errors
    /// 当模式格式非法时返回 [`DisplayConfigError`]。
    pub fn set_gfxmode(&mut self, mode: &str) -> Result<(), DisplayConfigError> {
        let trimmed = mode.trim();
        if trimmed.is_empty() {
            return display_keys::set_gfxmode(&mut self.draft_config, None);
        }
        let parsed = GfxMode::parse(trimmed)?;
        display_keys::set_gfxmode(&mut self.draft_config, Some(&parsed))
    }

    /// 获取菜单分辨率原始值
    pub fn get_gfxmode(&self) -> Option<&str> {
        self.draft_config.get("GRUB_GFXMODE")
    }

    /// 设置内核帧缓冲保持策略（GRUB_GFXPAYLOAD）
    ///
    /// # Errors
    /// 当模式格式非法时返回 [`DisplayConfigError`]。
    pub fn set_gfxpayload(&mut self, mode: &str) -> Result<(), DisplayConfigError> {
        let trimmed = mode.trim();
        if trimmed.is_empty() {
            return display_keys::set_gfxpayload(&mut self.draft_config, None);
        }
        let parsed = GfxMode::parse(trimmed)?;
        display_keys::set_gfxpayload(&mut self.draft_config, Some(&parsed))
    }

    /// 获取内核帧缓冲保持策略原始值
    pub fn get_gfxpayload(&self) -> Option<&str> {
        self.draft_config.get("GRUB_GFXPAYLOAD")
    }

    /// 列出常见安全分辨率（探测失败时的回退候选）
    pub fn common_gfx_modes() -> &'static [&'static str] {
        GfxMode::COMMON_MODES
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
            entries.extend(node.collect_entries());
        }
        entries.into_iter().find(|e| e.full_path == target_path)
    }

    /// 设置界面视觉主题模式
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
    }

    /// 获取当前界面视觉主题模式
    pub fn get_theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    /// 设置界面字体缩放级别
    pub fn set_font_scale(&mut self, scale: FontScale) {
        self.font_scale = scale;
    }

    /// 获取当前界面字体缩放级别
    pub fn get_font_scale(&self) -> FontScale {
        self.font_scale
    }

    /// 获取当前生效的语义化调色板
    pub fn current_palette(&self, system_preference: SystemColorScheme) -> ColorPalette {
        let resolved = self.theme_mode.resolve(system_preference);
        ColorPalette::for_theme(resolved)
    }

    /// 设置 GRUB 开机主题描述文件绝对路径
    pub fn set_grub_theme_path(&mut self, path: Option<&str>) {
        match path {
            Some(p) if !p.trim().is_empty() => self.draft_config.set("GRUB_THEME", p.trim()),
            _ => {
                self.draft_config.remove("GRUB_THEME");
            }
        }
    }

    /// 应用解压安装后的主题目录（自动拼接 theme.txt 路径）
    pub fn apply_installed_theme(&mut self, theme_dir: &std::path::Path) {
        let theme_txt = theme_dir.join("theme.txt");
        self.set_grub_theme_path(Some(&theme_txt.to_string_lossy()));
    }

    /// 获取当前配置的 GRUB 开机主题路径
    pub fn get_grub_theme_path(&self) -> Option<&str> {
        self.draft_config.get("GRUB_THEME")
    }

    /// 设置 GRUB 开机背景壁纸路径
    ///
    /// # Errors
    /// 当路径含危险字符时返回 [`DisplayConfigError`]。
    pub fn set_grub_background_path(
        &mut self,
        path: Option<&str>,
    ) -> Result<(), DisplayConfigError> {
        display_keys::set_background(&mut self.draft_config, path)
    }

    /// 获取当前配置的 GRUB 开机背景壁纸路径
    pub fn get_grub_background_path(&self) -> Option<&str> {
        self.draft_config.get("GRUB_BACKGROUND")
    }

    /// 设置 GRUB 终端控制台文本前景色与背景色
    ///
    /// # Errors
    /// 当颜色格式非法时返回 [`DisplayConfigError`]。
    pub fn set_grub_colors(
        &mut self,
        normal: Option<&str>,
        highlight: Option<&str>,
    ) -> Result<(), DisplayConfigError> {
        let normal_color = match normal.map(str::trim) {
            Some(n) if !n.is_empty() => Some(GrubColor::parse(n)?),
            _ => None,
        };
        let highlight_color = match highlight.map(str::trim) {
            Some(h) if !h.is_empty() => Some(GrubColor::parse(h)?),
            _ => None,
        };
        display_keys::set_colors(
            &mut self.draft_config,
            normal_color.as_ref(),
            highlight_color.as_ref(),
        )
    }

    /// 获取 GRUB 终端常规颜色配置
    pub fn get_grub_color_normal(&self) -> Option<&str> {
        self.draft_config.get("GRUB_COLOR_NORMAL")
    }

    /// 获取 GRUB 终端高亮选中颜色配置
    pub fn get_grub_color_highlight(&self) -> Option<&str> {
        self.draft_config.get("GRUB_COLOR_HIGHLIGHT")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_custom_entries_and_aliases() {
        let default_grub = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n";
        let grub_cfg = "menuentry 'Ubuntu, with Linux 6.8' --id 'gnulinux-6.8' {}\nmenuentry 'Ubuntu (recovery)' --id 'gnulinux-rec' {}\n";

        let mut state = AppState::new_from_content(default_grub, grub_cfg);

        // 1. 设置别名
        state.set_entry_alias("gnulinux-6.8", "Ubuntu 6.8 (生产)");
        assert_eq!(
            state.get_entry_alias("gnulinux-6.8"),
            Some("Ubuntu 6.8 (生产)")
        );

        // 2. 查看条目渲染（应包含别名）
        let items = state.get_entry_items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].display_title, "Ubuntu 6.8 (生产)");

        // 3. 启用恢复模式过滤（原生零侵入隐藏）
        state.set_recovery_disabled(true);
        assert!(state.is_recovery_disabled());
        let filtered_items = state.get_entry_items();
        assert_eq!(filtered_items.len(), 1);
        assert_eq!(filtered_items[0].id.as_deref(), Some("gnulinux-6.8"));

        // 4. 添加自定义条目草稿
        let custom_entry = CustomBootEntry::new_iso_boot(
            "iso_live",
            "Fedora Workstation Live",
            "/boot/iso/fedora.iso",
            "UUID-7788",
            "",
        );
        state.add_custom_entry(custom_entry);
        assert!(state.has_custom_entry_changes());
        assert!(state.has_unsaved_changes());

        // 验证当前条目列表包含自定义项
        let items_with_custom = state.get_entry_items();
        assert_eq!(items_with_custom.len(), 2);
        assert!(items_with_custom.iter().any(|i| i.is_custom));

        // 5. 移除自定义项
        assert!(state.remove_custom_entry("iso_live"));
        assert!(!state.has_custom_entry_changes());
    }
}
