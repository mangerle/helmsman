use crate::app::HelmsmanWindow;
use crate::views::{page_header, scroll_page_container, section_card};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 渲染“系统救援与脚本”页面（当前包含 `/etc/grub.d/` 原生脚本管理）
pub fn render_system_tools(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    scroll_page_container()
        .child(page_header(
            "系统救援与脚本管理",
            "管理 /etc/grub.d/ 原生引导脚本启用状态与源码预览",
            Some(
                Tag::new()
                    .with_variant(TagVariant::Warning)
                    .small()
                    .child("高危系统工具")
                    .into_any_element(),
            ),
            cx,
        ))
        .child(render_grub_scripts_section(win, cx))
}

fn render_grub_scripts_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let selected_name = win
        .selected_script_name
        .as_deref()
        .or_else(|| win.backend.grub_scripts.first().map(|s| s.name.as_str()));
    let selected_script =
        selected_name.and_then(|name| win.backend.grub_scripts.iter().find(|s| s.name == name));

    section_card("/etc/grub.d/ 原生引导脚本", cx)
        .child(
            Label::new("通过开关控制脚本可执行权限（chmod +x / -x），不篡改发行版脚本正文")
                .text_xs()
                .text_color(cx.theme().muted_foreground),
        )
        .child(div().flex().flex_col().gap_1p5().children(
            win.backend.grub_scripts.iter().enumerate().map(|(idx, s)| {
                let is_selected = selected_name == Some(s.name.as_str());
                let name_select = s.name.clone();
                let name_toggle = s.name.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(if is_selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border.opacity(0.6)
                    })
                    .bg(if is_selected {
                        cx.theme().primary.opacity(0.08)
                    } else {
                        cx.theme().background
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Label::new(s.name.clone()).text_xs().font_medium())
                            .child(
                                Tag::new()
                                    .with_variant(if s.enabled {
                                        TagVariant::Success
                                    } else {
                                        TagVariant::Secondary
                                    })
                                    .xsmall()
                                    .child(if s.enabled { "已启用" } else { "已禁用" }),
                            )
                            .when(s.is_helmsman_managed, |el| {
                                el.child(
                                    Tag::new()
                                        .with_variant(TagVariant::Info)
                                        .xsmall()
                                        .child("Helmsman 托管"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new(("btn-view-script", idx))
                                    .ghost()
                                    .xsmall()
                                    .label("查看源码")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.selected_script_name = Some(name_select.clone());
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new(("sw-script-enabled", idx))
                                    .checked(s.enabled)
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        this.toggle_grub_script_action(&name_toggle, *checked, cx);
                                    })),
                            ),
                    )
            }),
        ))
        .when_some(selected_script, |card, script| {
            card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mt_2()
                    .child(
                        Label::new(format!(
                            "脚本源码预览：{} ({} 字节)",
                            script.name, script.size_bytes
                        ))
                        .text_xs()
                        .font_semibold(),
                    )
                    .child(
                        div()
                            .max_h(px(200.0))
                            .overflow_y_scrollbar()
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().secondary.opacity(0.4))
                            .child(Label::new(script.content.clone()).text_xs()),
                    ),
            )
        })
}
