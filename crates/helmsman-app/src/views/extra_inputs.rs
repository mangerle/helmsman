use gpui_kit::component::input::InputState;
use gpui_kit::*;
use grub_boot_reader::CustomBootEntry;

/// 常规设置与自定义条目完整属性编辑的输入框状态集合
pub struct ExtraFormInputs {
    /// 高级环境变量名输入框
    pub env_key_input: Entity<InputState>,
    /// 高级环境变量值输入框
    pub env_val_input: Entity<InputState>,
    /// 自定义条目目标路径输入框（ISO / 内核 / EFI）
    pub custom_target_input: Entity<InputState>,
    /// 自定义条目分区 UUID 输入框
    pub custom_uuid_input: Entity<InputState>,
    /// 自定义条目内核启动参数输入框
    pub custom_cmdline_input: Entity<InputState>,
    /// 自定义条目子菜单名称输入框（留空表示主菜单根层级）
    pub custom_submenu_input: Entity<InputState>,
}

impl ExtraFormInputs {
    /// 初始化额外表单输入框状态
    pub fn new(
        initial_custom: Option<&CustomBootEntry>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let env_key_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("变量名，如 GRUB_INIT_TUNE"));
        let env_val_input = cx.new(|cx| InputState::new(window, cx).placeholder("变量值"));
        let custom_target_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("/boot/iso/live.iso"));
        let custom_uuid_input = cx.new(|cx| InputState::new(window, cx).placeholder("0000-0000"));
        let custom_cmdline_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("如 quiet splash"));
        let custom_submenu_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("子菜单名称（留空则置于主菜单根层级）")
        });

        let this = Self {
            env_key_input,
            env_val_input,
            custom_target_input,
            custom_uuid_input,
            custom_cmdline_input,
            custom_submenu_input,
        };
        this.sync_selected_custom(initial_custom, window, cx);
        this
    }

    /// 切换选中条目时同步刷新自定义属性输入框
    pub fn sync_selected_custom(
        &self,
        custom: Option<&CustomBootEntry>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (target, uuid, cmdline, submenu) = match custom {
            Some(c) => (
                c.target_path.clone(),
                c.partition_uuid.clone(),
                c.cmdline_params.clone(),
                c.submenu_title.clone(),
            ),
            None => (String::new(), String::new(), String::new(), String::new()),
        };
        self.custom_target_input
            .update(cx, |s, cx| s.set_value(target, window, cx));
        self.custom_uuid_input
            .update(cx, |s, cx| s.set_value(uuid, window, cx));
        self.custom_cmdline_input
            .update(cx, |s, cx| s.set_value(cmdline, window, cx));
        self.custom_submenu_input
            .update(cx, |s, cx| s.set_value(submenu, window, cx));
    }
}
