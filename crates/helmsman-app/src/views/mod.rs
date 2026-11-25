pub mod appearance;
pub mod boot_entries;
pub mod modals;
pub mod nav_rail;
pub mod settings_about;
pub mod snapshots;
pub mod topbar;

use gpui_kit::component::label::Label;
use gpui_kit::component::scroll::{Scrollable, ScrollableElement as _};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::*;

/// 现代页面统一头部（大标题 + 副标题 + 可选右侧操作）
pub fn page_header<V>(
    title: &str,
    subtitle: &str,
    right: Option<AnyElement>,
    cx: &Context<V>,
) -> Div {
    let mut row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .pb_4()
        .mb_2()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.6))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Label::new(title.to_string())
                        .text_xl()
                        .font_bold()
                        .text_color(cx.theme().foreground),
                )
                .child(
                    Label::new(subtitle.to_string())
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                ),
        );

    if let Some(el) = right {
        row = row.child(el);
    }
    row
}

/// 可垂直滚动的主内容壳层
pub fn scroll_page_container() -> Scrollable<Div> {
    div()
        .flex()
        .flex_col()
        .size_full()
        .p_6()
        .gap_4()
        .overflow_y_scrollbar()
}

/// 现代圆角分组卡片容器
pub fn section_card<V>(title: &str, cx: &Context<V>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border.opacity(0.75))
        .bg(cx.theme().secondary.opacity(0.35))
        .child(
            Label::new(title.to_string())
                .text_sm()
                .font_semibold()
                .text_color(cx.theme().foreground),
        )
}

/// 现代左右对齐设置行（左侧标题+说明，右侧操作控件）
pub fn setting_row<V>(
    title: &str,
    description: &str,
    control: impl IntoElement,
    cx: &Context<V>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border.opacity(0.4))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .flex_1()
                .child(
                    Label::new(title.to_string())
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().foreground),
                )
                .child(
                    Label::new(description.to_string())
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .child(control)
}

/// 通用键值对展示行
pub fn kv_row<V>(label: &str, value: &str, cx: &Context<V>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .py_1p5()
        .border_t_1()
        .border_color(cx.theme().border.opacity(0.35))
        .child(
            Label::new(label.to_string())
                .text_xs()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            Label::new(value.to_string())
                .text_xs()
                .font_medium()
                .text_color(cx.theme().foreground),
        )
}
