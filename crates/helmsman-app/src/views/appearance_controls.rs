use crate::app::HelmsmanWindow;
use gpui_kit::component::IndexPath;
use gpui_kit::component::input::{InputEvent, InputState, NumberInputEvent, StepAction};
use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectItem, SelectState};
use gpui_kit::*;
use grub_config_parser::TimeoutStyle;
use helmsman_ui::AppState;

/// 下拉选择器通用键值项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub label: SharedString,
    pub value: String,
}

impl SelectOption {
    pub fn new(label: impl Into<SharedString>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

impl SelectItem for SelectOption {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

/// 外观设置页面的步进输入与下拉选择控件状态集合
pub struct AppearanceControls {
    /// 菜单等待倒计时步进输入框状态
    pub timeout_input: Entity<InputState>,
    /// 菜单显示风格下拉选择状态
    pub timeout_style_select: Entity<SelectState<SearchableVec<SelectOption>>>,
    /// 菜单分辨率下拉选择状态
    pub gfxmode_select: Entity<SelectState<SearchableVec<SelectOption>>>,
    /// 菜单分辨率手动输入框状态
    pub gfxmode_input: Entity<InputState>,
    /// 终端颜色配色下拉选择状态
    pub color_preset_select: Entity<SelectState<SearchableVec<SelectOption>>>,
}

impl AppearanceControls {
    /// 初始化全部外观页交互控件并绑定状态同步事件
    pub fn new(state: &AppState, window: &mut Window, cx: &mut Context<HelmsmanWindow>) -> Self {
        let timeout_input = Self::build_timeout_input(state, window, cx);
        let timeout_style_select = Self::build_timeout_style_select(state, window, cx);
        let gfxmode_input = Self::build_gfxmode_input(state, window, cx);
        let gfxmode_select = Self::build_gfxmode_select(state, &gfxmode_input, window, cx);
        let color_preset_select = Self::build_color_preset_select(state, window, cx);

        Self {
            timeout_input,
            timeout_style_select,
            gfxmode_select,
            gfxmode_input,
            color_preset_select,
        }
    }

    fn build_timeout_input(
        state: &AppState,
        window: &mut Window,
        cx: &mut Context<HelmsmanWindow>,
    ) -> Entity<InputState> {
        let init_val = state.get_timeout();
        let entity = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("秒 (-1 为无限)");
            input.set_value(init_val.to_string(), window, cx);
            input
        });

        cx.subscribe_in(
            &entity,
            window,
            |this, input_entity, ev: &NumberInputEvent, window, cx| {
                let NumberInputEvent::Step(action) = ev;
                let current = this.state.get_timeout();
                let next = match action {
                    StepAction::Increment => {
                        if current < 0 {
                            1
                        } else {
                            (current + 1).min(3600)
                        }
                    }
                    StepAction::Decrement => (current - 1).max(-1),
                };
                if let Err(e) = this.state.set_timeout(next) {
                    this.toast = Some((false, e.to_string()));
                } else {
                    input_entity.update(cx, |s, cx| s.set_value(next.to_string(), window, cx));
                }
                cx.notify();
            },
        )
        .detach();

        cx.subscribe(
            &entity,
            |this, input_entity, ev: &InputEvent, cx| match ev {
                InputEvent::Change | InputEvent::PressEnter { .. } => {
                    let raw = input_entity.read(cx).value().trim().to_string();
                    if let Ok(val) = raw.parse::<i32>() {
                        if let Err(e) = this.state.set_timeout(val) {
                            this.toast = Some((false, e.to_string()));
                        }
                        cx.notify();
                    }
                }
                _ => {}
            },
        )
        .detach();

        entity
    }

    fn build_timeout_style_select(
        state: &AppState,
        window: &mut Window,
        cx: &mut Context<HelmsmanWindow>,
    ) -> Entity<SelectState<SearchableVec<SelectOption>>> {
        let options = vec![
            SelectOption::new("展示完整菜单 (menu)", "menu"),
            SelectOption::new("静默隐藏等待 (hidden)", "hidden"),
            SelectOption::new("仅显示倒计时 (countdown)", "countdown"),
        ];
        let initial_idx = match state.get_timeout_style() {
            TimeoutStyle::Menu => 0,
            TimeoutStyle::Hidden => 1,
            TimeoutStyle::Countdown => 2,
        };
        let entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(options),
                Some(IndexPath::new(initial_idx)),
                window,
                cx,
            )
        });

        cx.subscribe_in(
            &entity,
            window,
            |this, _, ev: &SelectEvent<SearchableVec<SelectOption>>, _, cx| {
                let SelectEvent::Confirm(Some(val)) = ev else {
                    return;
                };
                let style = match val.as_str() {
                    "hidden" => TimeoutStyle::Hidden,
                    "countdown" => TimeoutStyle::Countdown,
                    _ => TimeoutStyle::Menu,
                };
                if let Err(e) = this.state.set_timeout_style(style) {
                    this.toast = Some((false, e.to_string()));
                }
                cx.notify();
            },
        )
        .detach();

        entity
    }

    fn build_gfxmode_input(
        state: &AppState,
        window: &mut Window,
        cx: &mut Context<HelmsmanWindow>,
    ) -> Entity<InputState> {
        let current = state.get_gfxmode().unwrap_or("auto").to_string();
        let entity = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("如 1920x1080 或 auto");
            input.set_value(current, window, cx);
            input
        });

        cx.subscribe(&entity, |this, input_entity, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                let val = input_entity.read(cx).value().trim().to_string();
                match this.state.set_gfxmode(&val) {
                    Ok(()) => {
                        this.toast = Some((true, format!("已设置菜单分辨率为 {val}")));
                    }
                    Err(e) => {
                        this.toast = Some((false, e.to_string()));
                    }
                }
                cx.notify();
            }
        })
        .detach();

        entity
    }

    fn build_gfxmode_select(
        state: &AppState,
        gfxmode_input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<HelmsmanWindow>,
    ) -> Entity<SelectState<SearchableVec<SelectOption>>> {
        let mut options = vec![SelectOption::new("自动探测 (auto)", "auto")];
        for mode in AppState::common_gfx_modes() {
            options.push(SelectOption::new(*mode, *mode));
        }
        let current = state.get_gfxmode().unwrap_or("auto");
        let initial_idx = options.iter().position(|o| o.value == current);

        let entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(options),
                initial_idx.map(IndexPath::new),
                window,
                cx,
            )
        });

        let input_ref = gfxmode_input.clone();
        cx.subscribe_in(
            &entity,
            window,
            move |this, _, ev: &SelectEvent<SearchableVec<SelectOption>>, window, cx| {
                let SelectEvent::Confirm(Some(val)) = ev else {
                    return;
                };
                if let Err(e) = this.state.set_gfxmode(val) {
                    this.toast = Some((false, e.to_string()));
                } else {
                    input_ref.update(cx, |s, cx| s.set_value(val.clone(), window, cx));
                }
                cx.notify();
            },
        )
        .detach();

        entity
    }

    fn build_color_preset_select(
        state: &AppState,
        window: &mut Window,
        cx: &mut Context<HelmsmanWindow>,
    ) -> Entity<SelectState<SearchableVec<SelectOption>>> {
        let options = vec![
            SelectOption::new("系统默认配色", "default"),
            SelectOption::new("经典白/黑 (white/black · black/light-gray)", "classic"),
            SelectOption::new("极客绿/黑 (light-green/black · black/green)", "geek"),
            SelectOption::new("科技青/深蓝 (light-cyan/blue · white/cyan)", "tech"),
        ];
        let norm = state.get_grub_color_normal().unwrap_or("");
        let hi = state.get_grub_color_highlight().unwrap_or("");
        let initial_idx = match (norm, hi) {
            ("white/black", "black/light-gray") => 1,
            ("light-green/black", "black/green") => 2,
            ("light-cyan/blue", "white/cyan") => 3,
            _ => 0,
        };

        let entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(options),
                Some(IndexPath::new(initial_idx)),
                window,
                cx,
            )
        });

        cx.subscribe_in(
            &entity,
            window,
            |this, _, ev: &SelectEvent<SearchableVec<SelectOption>>, _, cx| {
                let SelectEvent::Confirm(Some(val)) = ev else {
                    return;
                };
                let res = match val.as_str() {
                    "classic" => this
                        .state
                        .set_grub_colors(Some("white/black"), Some("black/light-gray")),
                    "geek" => this
                        .state
                        .set_grub_colors(Some("light-green/black"), Some("black/green")),
                    "tech" => this
                        .state
                        .set_grub_colors(Some("light-cyan/blue"), Some("white/cyan")),
                    _ => this.state.set_grub_colors(None, None),
                };
                if let Err(e) = res {
                    this.toast = Some((false, e.to_string()));
                }
                cx.notify();
            },
        )
        .detach();

        entity
    }
}
