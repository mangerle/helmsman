use crate::app::HelmsmanWindow;
use crate::backend::FEATURED_THEMES;
use crate::views::{kv_row, section_card};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use grub_config_parser::TimeoutStyle;
use helmsman_ui::AppState;

/// 常见高可读性 GRUB 终端配色方案预设
const COLOR_PRESETS: &[(&str, &str, &str)] = &[
    ("经典白/黑", "white/black", "black/light-gray"),
    ("极客绿/黑", "light-green/black", "black/green"),
    ("科技青/深蓝", "light-cyan/blue", "white/cyan"),
];

/// 渲染“外观与引导参数”页面
pub fn render_appearance(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .p_4()
        .gap_3()
        .overflow_hidden()
        .child(render_timeout_section(win, cx))
        .child(render_resolution_and_colors_section(win, cx))
        .child(render_themes_section(win, cx))
        .child(render_online_themes_section(win, cx))
}

fn render_timeout_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let timeout = win.state.get_timeout();
    let style = win.state.get_timeout_style();

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
        )
}

fn render_resolution_and_colors_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let gfxmode = win.state.get_gfxmode().unwrap_or("auto");
    let grub_bg = win.state.get_grub_background_path().unwrap_or("未设置背景");
    let color_normal = win.state.get_grub_color_normal().unwrap_or("默认");
    let color_highlight = win.state.get_grub_color_highlight().unwrap_or("默认");

    section_card("分辨率、开机壁纸与终端颜色", cx)
        .child(kv_row("当前分辨率 (GRUB_GFXMODE)", gfxmode, cx))
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
        )
        .child(kv_row("开机壁纸 (GRUB_BACKGROUND)", grub_bg, cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("btn-pick-bg")
                        .outline()
                        .xsmall()
                        .label("选择壁纸图片...")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pick_grub_background(cx);
                        })),
                )
                .child(
                    Button::new("btn-clear-bg")
                        .ghost()
                        .xsmall()
                        .label("清除壁纸")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = this.state.set_grub_background_path(None);
                            cx.notify();
                        })),
                ),
        )
        .child(kv_row(
            "终端配色 (常规 / 高亮)",
            &format!("{color_normal}  /  {color_highlight}"),
            cx,
        ))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .children(
                    COLOR_PRESETS
                        .iter()
                        .enumerate()
                        .map(|(idx, (title, norm, hi))| {
                            let is_active = color_normal == *norm && color_highlight == *hi;
                            let btn = Button::new(("color-preset", idx)).xsmall().label(*title);
                            let btn = if is_active {
                                btn.primary()
                            } else {
                                btn.outline()
                            };
                            btn.on_click(cx.listener(move |this, _, _, cx| {
                                if let Err(e) = this.state.set_grub_colors(Some(norm), Some(hi)) {
                                    this.toast = Some((false, e.to_string()));
                                }
                                cx.notify();
                            }))
                        }),
                )
                .child(
                    Button::new("btn-clear-colors")
                        .ghost()
                        .xsmall()
                        .label("恢复默认颜色")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = this.state.set_grub_colors(None, None);
                            cx.notify();
                        })),
                ),
        )
}

fn render_themes_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let grub_theme = win.state.get_grub_theme_path().unwrap_or("未启用主题");
    let busy = win.is_installing_theme;

    section_card("已安装 GRUB 主题管理", cx)
        .child(kv_row("当前 GRUB_THEME", grub_theme, cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("btn-import-theme-archive")
                        .primary()
                        .xsmall()
                        .disabled(busy)
                        .label(if busy {
                            "正在安装中..."
                        } else {
                            "从本地压缩包导入 (.zip/.tar.gz)..."
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pick_and_install_theme_archive(cx);
                        })),
                )
                .child(
                    Button::new("btn-clear-theme")
                        .ghost()
                        .xsmall()
                        .label("清除主题（恢复纯文本菜单）")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.set_grub_theme_path(None);
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .when(win.backend.themes.is_empty(), |el| {
                    el.child(
                        Label::new("系统主题目录下暂无已安装主题，可从本地导入或下方在线下载。")
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    )
                })
                .children(win.backend.themes.iter().enumerate().map(|(idx, theme)| {
                    let theme_path = format!("{}/theme.txt", theme.path);
                    let name_for_remove = theme.name.clone();
                    let is_applied = grub_theme == theme_path;
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(if is_applied {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
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
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    Button::new(("apply-theme", idx))
                                        .outline()
                                        .xsmall()
                                        .label(if is_applied {
                                            "已选定"
                                        } else {
                                            "应用此主题"
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.state.set_grub_theme_path(Some(&theme_path));
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new(("remove-theme", idx))
                                        .danger()
                                        .xsmall()
                                        .disabled(busy)
                                        .label("卸载")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.uninstall_theme(name_for_remove.clone(), cx);
                                        })),
                                ),
                        )
                })),
        )
}

fn render_online_themes_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let busy = win.is_installing_theme;

    section_card("在线下载 GRUB 主题 (HTTPS 安全直链)", cx)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().child(Input::new(&win.theme_url_input)))
                .child(
                    Button::new("btn-download-custom-url")
                        .primary()
                        .small()
                        .disabled(busy)
                        .label("下载并安装")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let url = this.theme_url_input.read(cx).value().to_string();
                            this.download_and_install_online_theme(url, String::new(), cx);
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .children(FEATURED_THEMES.iter().enumerate().map(|(idx, item)| {
                    let url = item.url.to_string();
                    let name = item.name.to_string();
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
                                .flex_col()
                                .gap_0p5()
                                .child(Label::new(item.name).text_xs())
                                .child(
                                    Label::new(item.description)
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                ),
                        )
                        .child(
                            Button::new(("download-featured", idx))
                                .outline()
                                .xsmall()
                                .disabled(busy)
                                .label("一键下载安装")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.download_and_install_online_theme(
                                        url.clone(),
                                        name.clone(),
                                        cx,
                                    );
                                })),
                        )
                })),
        )
}
