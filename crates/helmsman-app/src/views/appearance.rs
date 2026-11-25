use crate::app::HelmsmanWindow;
use crate::backend::FEATURED_THEMES;
use crate::views::{kv_row, page_header, scroll_page_container, section_card, setting_row};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::label::Label;
use gpui_kit::component::select::Select;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, StyledExt as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 渲染“外观与引导参数”页面（带滚动容器与现代卡片排版）
pub fn render_appearance(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    scroll_page_container()
        .child(page_header(
            "主题与视觉外观",
            "自定义 GRUB 显示分辨率、开机壁纸配色以及在线下载/本地导入 GRUB 主题",
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
        .child(render_resolution_and_colors_section(win, cx))
        .child(render_themes_section(win, cx))
        .child(render_online_themes_section(win, cx))
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
            &format!("当前模式: {gfxmode}（支持预设下拉或手动输入后应用）"),
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .w(px(155.0))
                        .child(Select::new(&win.appearance_controls.gfxmode_select).small()),
                )
                .child(
                    div()
                        .w(px(135.0))
                        .child(Input::new(&win.appearance_controls.gfxmode_input).small()),
                )
                .child(
                    Button::new("btn-apply-gfxmode")
                        .outline()
                        .small()
                        .label("应用")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let val = this
                                .appearance_controls
                                .gfxmode_input
                                .read(cx)
                                .value()
                                .trim()
                                .to_string();
                            match this.state.set_gfxmode(&val) {
                                Ok(()) => {
                                    this.toast = Some((true, format!("已设置菜单分辨率为 {val}")));
                                }
                                Err(e) => {
                                    this.toast = Some((false, e.to_string()));
                                }
                            }
                            cx.notify();
                        })),
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
            "终端颜色配色预设 (常规 / 高亮)",
            &format!("{color_normal}  /  {color_highlight}"),
            div()
                .w(px(280.0))
                .child(Select::new(&win.appearance_controls.color_preset_select).small()),
            cx,
        ))
        .child(setting_row(
            "常规文字与背景色 (GRUB_COLOR_NORMAL)",
            "自由搭配未选中项的前景色与背景色（标准 16 色）",
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div().w(px(135.0)).child(
                        Select::new(&win.appearance_controls.color_normal_fg_select).small(),
                    ),
                )
                .child(Label::new("/").text_xs())
                .child(
                    div().w(px(135.0)).child(
                        Select::new(&win.appearance_controls.color_normal_bg_select).small(),
                    ),
                ),
            cx,
        ))
        .child(setting_row(
            "高亮文字与背景色 (GRUB_COLOR_HIGHLIGHT)",
            "自由搭配当前选中高亮项的前景色与背景色（标准 16 色）",
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div().w(px(135.0)).child(
                        Select::new(&win.appearance_controls.color_highlight_fg_select).small(),
                    ),
                )
                .child(Label::new("/").text_xs())
                .child(div().w(px(135.0)).child(
                    Select::new(&win.appearance_controls.color_highlight_bg_select).small(),
                )),
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
