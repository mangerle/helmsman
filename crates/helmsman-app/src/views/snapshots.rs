use crate::app::HelmsmanWindow;
use crate::views::{page_header, scroll_page_container, section_card};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 渲染“历史快照与回滚”页面
pub fn render_snapshots(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    scroll_page_container()
        .child(page_header(
            "配置快照与灾备回滚",
            "每次系统配置变更前均会自动创建原子快照，支持一键秒级还原并重建引导菜单",
            Some(
                Tag::new()
                    .with_variant(TagVariant::Secondary)
                    .small()
                    .child(format!("共 {} 条快照", win.backend.snapshots.len()))
                    .into_any_element(),
            ),
            cx,
        ))
        .child(
            section_card("历史配置快照列表", cx)
                .when(win.backend.snapshots.is_empty(), |el| {
                    el.child(
                        div().py_8().flex().justify_center().child(
                            Label::new("暂无可用快照记录。提交配置修改后将自动生成。")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        ),
                    )
                })
                .children(win.backend.snapshots.iter().enumerate().map(|(idx, snap)| {
                    let id_for_rollback = snap.id.clone();
                    let id_for_delete = snap.id.clone();
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_4()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().border.opacity(0.6))
                        .bg(cx.theme().background.opacity(0.75))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Label::new(snap.id.clone()).text_xs().font_semibold(),
                                        )
                                        .child(
                                            Tag::new()
                                                .with_variant(TagVariant::Secondary)
                                                .xsmall()
                                                .child(format!("TS: {}", snap.timestamp)),
                                        ),
                                )
                                .child(
                                    Label::new(format!("原因: {}", snap.reason))
                                        .text_xs()
                                        .text_color(cx.theme().foreground),
                                )
                                .child(
                                    Label::new(format!("目标文件: {}", snap.target_file))
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    Button::new(("snap-rollback", idx))
                                        .outline()
                                        .xsmall()
                                        .label("一键回滚")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.perform_rollback(&id_for_rollback, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("snap-delete", idx))
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Delete)
                                        .label("删除")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.perform_delete_snapshot(&id_for_delete, cx);
                                        })),
                                ),
                        )
                })),
        )
}
