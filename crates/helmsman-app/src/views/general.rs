use crate::app::HelmsmanWindow;
use crate::views::{page_header, scroll_page_container, section_card, setting_row};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, NumberInput};
use gpui_kit::component::label::Label;
use gpui_kit::component::select::Select;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _};
use gpui_kit::*;
use grub_config_parser::well_known_flags;

/// 渲染“常规引导设置”页面（集中管理默认启动项、菜单可见性与倒计时、内核参数）
pub fn render_general(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let has_saved_default = win.state.is_saved_default_enabled();
    let timeout = win.state.get_timeout();
    let cmdline_default = win.state.get_cmdline_default();
    let has_quiet = cmdline_default.split_whitespace().any(|t| t == "quiet");
    let has_splash = cmdline_default.split_whitespace().any(|t| t == "splash");
    let has_nomodeset = cmdline_default.split_whitespace().any(|t| t == "nomodeset");
    let recovery_disabled = win.state.is_recovery_disabled();
    let submenu_disabled = win.state.is_submenu_disabled();
    let os_prober_disabled = win.state.get_menu_visibility().os_prober_disabled;

    scroll_page_container()
        .child(page_header(
            "常规引导设置",
            "集中配置默认启动项、菜单可见性与等待时间，以及全局内核启动参数",
            Some(
                Tag::new()
                    .with_variant(TagVariant::Info)
                    .small()
                    .child("全局策略")
                    .into_any_element(),
            ),
            cx,
        ))
        .child(
            section_card("默认启动项 (GRUB_DEFAULT)", cx)
                .child(setting_row(
                    "记忆上一个启动项 (GRUB_DEFAULT=saved)",
                    "开启后自动选中上一次开机所选条目；关闭则使用下方预定义默认项",
                    Switch::new("sw-general-saved-default")
                        .checked(has_saved_default)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            if *checked {
                                this.state.enable_saved_default_mode();
                                this.toast =
                                    Some((true, "已切换为“记忆上一个启动项”模式".to_string()));
                            } else {
                                let _ = this.state.set_default_entry_by_index(0);
                                this.toast =
                                    Some((true, "已切换为“预定义默认项 (0)”模式".to_string()));
                            }
                            cx.notify();
                        })),
                    cx,
                ))
                .child(setting_row(
                    "预定义默认启动项",
                    if has_saved_default {
                        "当前已启用“记忆上次启动项”，关闭上方开关后可指定固定默认条目"
                    } else {
                        "从下拉列表直接选择每次开机默认高亮并自动引导的条目"
                    },
                    div().w(px(280.0)).child(
                        Select::new(&win.appearance_controls.default_entry_select)
                            .small()
                            .disabled(has_saved_default),
                    ),
                    cx,
                )),
        )
        .child(
            section_card("菜单可见性与等待倒计时", cx)
                .child(setting_row(
                    "菜单显示风格 (GRUB_TIMEOUT_STYLE)",
                    "控制开机时是直接展示完整菜单、倒计时还是静默等待",
                    div()
                        .w(px(230.0))
                        .child(Select::new(&win.appearance_controls.timeout_style_select).small()),
                    cx,
                ))
                .child(setting_row(
                    "自动引导等待倒计时 (GRUB_TIMEOUT)",
                    &format!("当前设置: {timeout} 秒（-1 表示无限等待）"),
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().w(px(140.0)).child(
                            NumberInput::new(&win.appearance_controls.timeout_input).small(),
                        ))
                        .child({
                            let btn = Button::new("btn-general-timeout-inf")
                                .small()
                                .label("无限等待");
                            let btn = if timeout < 0 {
                                btn.primary()
                            } else {
                                btn.outline()
                            };
                            btn.on_click(cx.listener(|this, _, window, cx| {
                                let next = if this.state.get_timeout() < 0 { 5 } else { -1 };
                                if let Err(e) = this.state.set_timeout(next) {
                                    this.toast = Some((false, e.to_string()));
                                } else {
                                    this.appearance_controls.timeout_input.update(cx, |s, cx| {
                                        s.set_value(next.to_string(), window, cx);
                                    });
                                }
                                cx.notify();
                            }))
                        }),
                    cx,
                ))
                .child(setting_row(
                    "查找其它操作系统 (os-prober)",
                    "自动扫描其他分区上的 Windows 或其他 Linux 发行版",
                    Switch::new("sw-general-osprober")
                        .checked(!os_prober_disabled)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.state.set_os_prober_enabled(*checked);
                            cx.notify();
                        })),
                    cx,
                ))
                .child(setting_row(
                    "拉平二级子菜单 (GRUB_DISABLE_SUBMENU)",
                    "将高级选项中的旧内核直接平铺在主菜单列表",
                    Switch::new("sw-general-submenu")
                        .checked(submenu_disabled)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.state.set_submenu_disabled(*checked);
                            cx.notify();
                        })),
                    cx,
                )),
        )
        .child(
            section_card("全局内核启动参数 (GRUB_CMDLINE_LINUX_DEFAULT)", cx)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&win.cmdline_input).small()))
                        .child(
                            Button::new("btn-general-apply-cmdline")
                                .outline()
                                .small()
                                .label("应用参数")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.apply_cmdline_input(cx);
                                })),
                        ),
                )
                .child(setting_row(
                    "安静启动 (quiet)",
                    "隐藏启动过程中的大部分内核日志输出",
                    Switch::new("sw-general-quiet")
                        .checked(has_quiet)
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.toggle_cmdline_flag(well_known_flags::QUIET, *checked, window, cx);
                        })),
                    cx,
                ))
                .child(setting_row(
                    "开机画面 (splash)",
                    "配合 Plymouth 展示发行版图形启动动画",
                    Switch::new("sw-general-splash")
                        .checked(has_splash)
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.toggle_cmdline_flag(
                                well_known_flags::SPLASH,
                                *checked,
                                window,
                                cx,
                            );
                        })),
                    cx,
                ))
                .child(setting_row(
                    "禁用内核模式设置 (nomodeset)",
                    "遇到显卡黑屏或驱动兼容性故障时开启排障",
                    Switch::new("sw-general-nomodeset")
                        .checked(has_nomodeset)
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.toggle_cmdline_flag(
                                well_known_flags::NOMODESET,
                                *checked,
                                window,
                                cx,
                            );
                        })),
                    cx,
                ))
                .child(setting_row(
                    "隐藏恢复模式 (GRUB_DISABLE_RECOVERY)",
                    "不在引导菜单展示 recovery mode 条目，保持菜单简洁",
                    Switch::new("sw-general-recovery")
                        .checked(recovery_disabled)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.state.set_recovery_disabled(*checked);
                            cx.notify();
                        })),
                    cx,
                )),
        )
        .child(render_advanced_env_section(win, cx))
}

fn render_advanced_env_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let vars = win.state.list_raw_config_vars();

    section_card("高级环境变量 (/etc/default/grub 原始键值)", cx)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(220.0))
                        .child(Input::new(&win.extra_inputs.env_key_input).small()),
                )
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&win.extra_inputs.env_val_input).small()),
                )
                .child(
                    Button::new("btn-save-raw-var")
                        .outline()
                        .small()
                        .icon(IconName::Plus)
                        .label("添加 / 更新")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let key = this
                                .extra_inputs
                                .env_key_input
                                .read(cx)
                                .value()
                                .trim()
                                .to_string();
                            let val = this
                                .extra_inputs
                                .env_val_input
                                .read(cx)
                                .value()
                                .trim()
                                .to_string();
                            match this.state.set_raw_config_var(&key, &val) {
                                Ok(()) => {
                                    this.toast = Some((
                                        true,
                                        format!("已更新变量 {key}={val}（审核保存后生效）"),
                                    ));
                                }
                                Err(e) => {
                                    this.toast = Some((false, e));
                                }
                            }
                            cx.notify();
                        })),
                ),
        )
        .children(vars.into_iter().enumerate().map(|(idx, (k, v))| {
            let k_edit = k.clone();
            let v_edit = v.clone();
            let k_del = k.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .py_1p5()
                .border_t_1()
                .border_color(cx.theme().border.opacity(0.35))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .flex_1()
                        .child(
                            Tag::new()
                                .with_variant(TagVariant::Secondary)
                                .xsmall()
                                .child(k),
                        )
                        .child(
                            Label::new(v)
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new(("edit-raw-var", idx))
                                .ghost()
                                .xsmall()
                                .label("编辑")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.extra_inputs.env_key_input.update(cx, |s, cx| {
                                        s.set_value(k_edit.clone(), window, cx)
                                    });
                                    this.extra_inputs.env_val_input.update(cx, |s, cx| {
                                        s.set_value(v_edit.clone(), window, cx)
                                    });
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(("del-raw-var", idx))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Delete)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.state.remove_raw_config_var(&k_del) {
                                        this.toast = Some((
                                            true,
                                            format!("已移除变量 {k_del}（审核保存后生效）"),
                                        ));
                                    }
                                    cx.notify();
                                })),
                        ),
                )
        }))
}
