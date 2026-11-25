use crate::app::HelmsmanWindow;
use crate::backend::FEATURED_THEMES;
use crate::views::{kv_row, page_header, scroll_page_container, section_card, setting_row};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, StyledExt as _,
};
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

/// 渲染“外观与引导参数”页面（带滚动容器与现代卡片排版）
pub fn render_appearance(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    scroll_page_container()
        .child(page_header(
            "主题与视觉外观",
            "自定义 GRUB 引导倒计时、显示分辨率、壁纸配色以及在线下载/本地导入 GRUB 主题",
            Some(
                Button::new("btn-page-preview")
                    .outline()
                    .small()
                    .icon(IconName::Eye)
                    .label("预览开机效果")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_preview_modal(cx);
                    }))
                    .into_any_element(),
            ),
            cx,
        ))
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
        .child(setting_row(
            "菜单等待倒计时 (GRUB_TIMEOUT)",
            &format!("当前设置: {timeout} 秒（-1 表示无限等待）"),
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .children([0, 3, 5, 10, -1].into_iter().map(|sec| {
                    let label = if sec < 0 {
                        "无限".to_string()
                    } else {
                        format!("{sec}s")
                    };
                    let btn = Button::new(("timeout-preset", sec as usize))
                        .xsmall()
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
            cx,
        ))
        .child(setting_row(
            "菜单显示风格 (GRUB_TIMEOUT_STYLE)",
            "控制开机时是直接展示完整菜单、倒计时还是静默等待",
            div().flex().items_center().gap_1p5().children(
                [
                    (TimeoutStyle::Menu, "menu"),
                    (TimeoutStyle::Hidden, "hidden"),
                    (TimeoutStyle::Countdown, "countdown"),
                ]
                .into_iter()
                .enumerate()
                .map(|(idx, (s, title))| {
                    let btn = Button::new(("timeout-style", idx)).xsmall().label(title);
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
            cx,
        ))
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
        .child(setting_row(
            "菜单分辨率 (GRUB_GFXMODE)",
            &format!("当前模式: {gfxmode}"),
            div().flex().flex_wrap().justify_end().gap_1p5().children(
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
            cx,
        ))
        .child(setting_row(
            "开机背景壁纸 (GRUB_BACKGROUND)",
            grub_bg,
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    Button::new("btn-pick-bg")
                        .outline()
                        .xsmall()
                        .icon(IconName::Folder)
                        .label("选择图片...")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pick_grub_background(cx);
                        })),
                )
                .child(
                    Button::new("btn-clear-bg")
                        .ghost()
                        .xsmall()
                        .label("清除")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = this.state.set_grub_background_path(None);
                            cx.notify();
                        })),
                ),
            cx,
        ))
        .child(setting_row(
            "终端颜色配色 (常规 / 高亮)",
            &format!("{color_normal}  /  {color_highlight}"),
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1p5()
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
                        .label("恢复默认")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = this.state.set_grub_colors(None, None);
                            cx.notify();
                        })),
                ),
            cx,
        ))
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
                        .small()
                        .icon(IconName::Folder)
                        .disabled(busy)
                        .label(if busy {
                            "正在安装中..."
                        } else {
                            "导入本地主题包 (.zip / .tar.gz)..."
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pick_and_install_theme_archive(cx);
                        })),
                )
                .child(
                    Button::new("btn-clear-theme")
                        .ghost()
                        .small()
                        .label("清除主题（纯文本菜单）")
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
                .gap_2()
                .when(win.backend.themes.is_empty(), |el| {
                    el.child(
                        Label::new(
                            "系统主题目录下暂无已安装主题，可点击上方按钮导入或从下方在线下载。",
                        )
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
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(if is_applied {
                            cx.theme().primary
                        } else {
                            cx.theme().border.opacity(0.6)
                        })
                        .bg(cx.theme().background.opacity(0.75))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(Label::new(theme.name.clone()).text_xs().font_semibold())
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
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Delete)
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
                        .icon(IconName::ArrowDown)
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
                .gap_2()
                .children(FEATURED_THEMES.iter().enumerate().map(|(idx, item)| {
                    let url = item.url.to_string();
                    let name = item.name.to_string();
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().border.opacity(0.6))
                        .bg(cx.theme().background.opacity(0.6))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(Label::new(item.name).text_xs().font_semibold())
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
                                .icon(IconName::ArrowDown)
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
