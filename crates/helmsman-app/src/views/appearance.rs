use crate::app::HelmsmanWindow;
use crate::views::{kv_row, section_card};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use grub_config_parser::TimeoutStyle;
use helmsman_ui::AppState;

/// 渲染“外观与引导参数”页面
pub fn render_appearance(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let timeout = win.state.get_timeout();
    let style = win.state.get_timeout_style();
    let gfxmode = win.state.get_gfxmode().unwrap_or("auto");
    let grub_theme = win.state.get_grub_theme_path().unwrap_or("未启用主题");
    let grub_bg = win.state.get_grub_background_path().unwrap_or("未设置背景");

    div()
        .flex()
        .flex_col()
        .size_full()
        .p_4()
        .gap_4()
        .child(
            section_card("等待倒计时与菜单显示策略", cx)
                .child(kv_row(
                    "当前倒计时 (GRUB_TIMEOUT)",
                    &format!("{timeout} 秒"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .children([0, 3, 5, 10, -1].into_iter().map(|sec| {
                            let label = if sec < 0 {
                                "无限等待 (-1)".to_string()
                            } else {
                                format!("{sec} 秒")
                            };
                            let btn = Button::new(("timeout-preset", sec as usize))
                                .small()
                                .label(label);
                            let btn = if timeout == sec {
                                btn.primary()
                            } else {
                                btn.outline()
                            };
                            btn.on_click(cx.listener(move |this, _, _, cx| {
                                if let Err(e) = this.state.set_timeout(sec) {
                                    this.toast = Some((false, e.to_string()));
                                }
                                cx.notify();
                            }))
                        })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pt_1()
                        .child(
                            Label::new("显示风格:")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .children(
                            [
                                (TimeoutStyle::Menu, "menu (显示菜单)"),
                                (TimeoutStyle::Hidden, "hidden (隐藏等待)"),
                                (TimeoutStyle::Countdown, "countdown (仅倒计时)"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(idx, (s, title))| {
                                let btn = Button::new(("timeout-style", idx)).small().label(title);
                                let btn = if style == s {
                                    btn.primary()
                                } else {
                                    btn.outline()
                                };
                                btn.on_click(cx.listener(move |this, _, _, cx| {
                                    if let Err(e) = this.state.set_timeout_style(s) {
                                        this.toast = Some((false, e.to_string()));
                                    }
                                    cx.notify();
                                }))
                            }),
                        ),
                ),
        )
        .child(
            section_card("显示分辨率 (GRUB_GFXMODE)", cx)
                .child(kv_row("当前分辨率模式", gfxmode, cx))
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        AppState::common_gfx_modes()
                            .iter()
                            .enumerate()
                            .map(|(idx, mode)| {
                                let mode_str = (*mode).to_string();
                                let btn = Button::new(("gfxmode", idx))
                                    .xsmall()
                                    .label(mode_str.clone());
                                let btn = if gfxmode == *mode {
                                    btn.primary()
                                } else {
                                    btn.outline()
                                };
                                btn.on_click(cx.listener(move |this, _, _, cx| {
                                    if let Err(e) = this.state.set_gfxmode(&mode_str) {
                                        this.toast = Some((false, e.to_string()));
                                    }
                                    cx.notify();
                                }))
                            }),
                    ),
                ),
        )
        .child(
            section_card("GRUB 引导主题与背景壁纸", cx)
                .child(kv_row("当前 GRUB_THEME", grub_theme, cx))
                .child(kv_row("当前 GRUB_BACKGROUND", grub_bg, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .when(win.backend.themes.is_empty(), |el| {
                            el.child(
                                Label::new("系统主题目录下尚未检测到已安装主题（可清除当前主题回退经典文本菜单）。")
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground),
                            )
                        })
                        .children(win.backend.themes.iter().enumerate().map(|(idx, theme)| {
                            let theme_path = format!("{}/theme.txt", theme.path);
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .p_2()
                                .rounded_md()
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(Label::new(theme.name.clone()).text_xs())
                                        .child(
                                            Tag::new()
                                                .with_variant(if theme.has_descriptor {
                                                    TagVariant::Success
                                                } else {
                                                    TagVariant::Warning
                                                })
                                                .xsmall()
                                                .child(if theme.has_descriptor {
                                                    "含 theme.txt"
                                                } else {
                                                    "缺少描述文件"
                                                }),
                                        ),
                                )
                                .child(
                                    Button::new(("apply-theme", idx))
                                        .outline()
                                        .xsmall()
                                        .label("应用此主题")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.state.set_grub_theme_path(Some(&theme_path));
                                            cx.notify();
                                        })),
                                )
                        })),
                )
                .child(
                    div().pt_1().child(
                        Button::new("btn-clear-theme")
                            .ghost()
                            .xsmall()
                            .label("清除 GRUB_THEME（恢复纯文本菜单）")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.set_grub_theme_path(None);
                                cx.notify();
                            })),
                    ),
                ),
        )
}
