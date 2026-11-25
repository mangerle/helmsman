use crate::app::HelmsmanWindow;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, StyledExt as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use helmsman_ui::DiffViewModel;

/// 渲染顶部工具栏
pub fn render_topbar(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let has_changes = win.state.has_unsaved_changes();
    let diff_summary = if has_changes {
        let report = win.state.compute_diff();
        let parsed = DiffViewModel::from_unified_diff(&report.diff_text);
        format!(
            "+{} / -{} 行待提交",
            parsed.added_count, parsed.removed_count
        )
    } else {
        "草稿与基准一致".to_string()
    };

    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(54.0))
        .px_4()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.75))
        .bg(cx.theme().secondary.opacity(0.75))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(30.0))
                        .rounded_lg()
                        .bg(cx.theme().primary.opacity(0.16))
                        .text_color(cx.theme().primary)
                        .child(IconName::Cpu),
                )
                .child(
                    Label::new("Helmsman")
                        .text_base()
                        .font_bold()
                        .text_color(cx.theme().foreground),
                )
                .child(
                    Tag::new()
                        .with_variant(if win.backend.is_demo_mode {
                            TagVariant::Warning
                        } else {
                            TagVariant::Success
                        })
                        .small()
                        .child(if win.backend.is_demo_mode {
                            "离线演示模式".to_string()
                        } else {
                            win.backend.status_note.clone()
                        }),
                )
                .child(
                    Label::new(diff_summary)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("btn-reload")
                        .ghost()
                        .small()
                        .label("重新同步")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reload_from_backend(cx);
                        })),
                )
                .child(
                    Button::new("btn-preview")
                        .outline()
                        .small()
                        .icon(IconName::Eye)
                        .label("开机预览")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_preview_modal(cx);
                        })),
                )
                .when(has_changes, |el| {
                    el.child(
                        Button::new("btn-reset")
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .label("放弃修改")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.reset_draft();
                                this.toast = Some((true, "已恢复至原始基准配置".to_string()));
                                cx.notify();
                            })),
                    )
                })
                .child(
                    Button::new("btn-apply")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .disabled(!has_changes)
                        .label("审核并保存")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_apply_modal(cx);
                        })),
                ),
        )
}
