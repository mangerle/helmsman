use crate::app::{ActiveModal, HelmsmanWindow};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use helmsman_ui::{
    DiffLineType, DiffViewModel, ImpactAnalyzer, Language, RiskLevel, SafetyValidator,
};

/// 渲染当前激活的模态弹窗（Diff 审核弹窗 / GRUB 菜单实时预览弹窗）
pub fn render_active_modal(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui_kit::black().opacity(0.55))
        .child(match win.active_modal {
            ActiveModal::ApplyDiffReview => render_apply_diff_modal(win, cx).into_any_element(),
            ActiveModal::MenuPreview => render_menu_preview_modal(win, cx).into_any_element(),
            ActiveModal::None => div().into_any_element(),
        })
}

fn render_apply_diff_modal(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let diff_report = win.state.compute_diff();
    let parsed_diff = DiffViewModel::from_unified_diff(&diff_report.diff_text);
    let warnings = SafetyValidator::validate(&win.state.draft_config, Language::ZhCn);
    let impacts = ImpactAnalyzer::analyze(
        &win.state.original_config,
        &win.state.draft_config,
        Language::ZhCn,
    );

    let has_danger = SafetyValidator::max_risk_level(&warnings) == RiskLevel::DangerBlocked;

    div()
        .flex()
        .flex_col()
        .w(px(720.0))
        .max_h(px(580.0))
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .p_5()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    Label::new("变更影响分析与 Diff 审核")
                        .text_base()
                        .font_semibold(),
                )
                .child(
                    Tag::new()
                        .with_variant(if has_danger {
                            TagVariant::Danger
                        } else {
                            TagVariant::Info
                        })
                        .small()
                        .child(format!(
                            "+{} / -{} 行",
                            parsed_diff.added_count, parsed_diff.removed_count
                        )),
                ),
        )
        .when(!warnings.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2p5()
                    .rounded_md()
                    .border_1()
                    .border_color(if has_danger {
                        cx.theme().danger
                    } else {
                        cx.theme().warning
                    })
                    .children(warnings.iter().map(|w| {
                        Label::new(format!("[{}] {}: {}", w.level, w.title, w.message)).text_xs()
                    })),
            )
        })
        .when(!impacts.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2p5()
                    .rounded_md()
                    .bg(cx.theme().secondary.opacity(0.6))
                    .children(impacts.items.iter().map(|imp| {
                        Label::new(format!("• {}: {}", imp.title, imp.explanation)).text_xs()
                    })),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(160.0))
                .max_h(px(300.0))
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary.opacity(0.3))
                .gap_0p5()
                .overflow_y_scrollbar()
                .when(parsed_diff.lines.is_empty(), |el| {
                    el.child(
                        Label::new(if win.state.has_custom_entry_changes() {
                            "仅包含自定义引导条目变更（/etc/grub.d/41_helmsman_custom）。"
                        } else {
                            "无文本行级变更。"
                        })
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                    )
                })
                .children(parsed_diff.lines.iter().take(200).map(|line| {
                    let prefix = match line.line_type {
                        DiffLineType::Added => "+ ",
                        DiffLineType::Removed => "- ",
                        DiffLineType::Context => "  ",
                        DiffLineType::HunkHeader => "@@ ",
                    };
                    let color = match line.line_type {
                        DiffLineType::Added => cx.theme().success,
                        DiffLineType::Removed => cx.theme().danger,
                        DiffLineType::Context | DiffLineType::HunkHeader => {
                            cx.theme().muted_foreground
                        }
                    };
                    Label::new(format!("{prefix}{}", line.content))
                        .text_xs()
                        .text_color(color)
                })),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .pt_2()
                .child(
                    Button::new("btn-cancel-apply")
                        .ghost()
                        .small()
                        .label("取消")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.active_modal = ActiveModal::None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("btn-confirm-apply")
                        .primary()
                        .small()
                        .label("确认授权并写入系统")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.commit_changes(cx);
                        })),
                ),
        )
}

fn render_menu_preview_modal(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let Some(preview) = win.preview_state.as_ref() else {
        return div();
    };

    let countdown_text = if preview.is_silent_mode {
        format!("静默启动倒计时: {} 秒", preview.remaining_seconds)
    } else if preview.timeout_seconds < 0 {
        "等待用户选择（无自动倒计时）".to_string()
    } else if preview.is_countdown_active {
        format!("{} 秒后自动启动高亮项", preview.remaining_seconds)
    } else {
        "倒计时已暂停".to_string()
    };

    div()
        .flex()
        .flex_col()
        .w(px(680.0))
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border)
        .bg(gpui_kit::rgb(0x11111b))
        .text_color(gpui_kit::rgb(0xcdd6f4))
        .p_5()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    Label::new("GRUB 开机菜单模拟预览")
                        .text_base()
                        .font_semibold(),
                )
                .child(Label::new(countdown_text).text_xs()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(gpui_kit::rgb(0x45475a))
                .gap_1()
                .children(preview.entries.iter().enumerate().map(|(idx, item)| {
                    let selected = idx == preview.selected_index;
                    let bg_color: Hsla = if selected {
                        gpui_kit::rgb(0x89b4fa).into()
                    } else {
                        gpui_kit::transparent_black()
                    };
                    let fg_color: Hsla = if selected {
                        gpui_kit::rgb(0x11111b).into()
                    } else {
                        gpui_kit::rgb(0xcdd6f4).into()
                    };
                    div()
                        .id(("preview-item", idx))
                        .px_3()
                        .py_1p5()
                        .rounded_sm()
                        .bg(bg_color)
                        .text_color(fg_color)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.preview_state.as_mut() {
                                p.selected_index = idx;
                                p.is_countdown_active = false;
                            }
                            cx.notify();
                        }))
                        .child(Label::new(item.title.clone()).text_xs())
                })),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            Button::new("btn-preview-prev")
                                .outline()
                                .small()
                                .icon(IconName::ArrowUp)
                                .label("上一项")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(p) = this.preview_state.as_mut()
                                        && !p.entries.is_empty()
                                    {
                                        p.selected_index = p.selected_index.saturating_sub(1);
                                        p.is_countdown_active = false;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("btn-preview-next")
                                .outline()
                                .small()
                                .icon(IconName::ArrowDown)
                                .label("下一项")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(p) = this.preview_state.as_mut()
                                        && !p.entries.is_empty()
                                    {
                                        let max_idx = p.entries.len().saturating_sub(1);
                                        p.selected_index = (p.selected_index + 1).min(max_idx);
                                        p.is_countdown_active = false;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    Button::new("btn-close-preview")
                        .secondary()
                        .small()
                        .label("关闭预览")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.active_modal = ActiveModal::None;
                            this.preview_state = None;
                            cx.notify();
                        })),
                ),
        )
}
