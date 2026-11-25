pub mod appearance;
pub mod boot_entries;
pub mod modals;
pub mod nav_rail;
pub mod settings_about;
pub mod snapshots;

use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::*;

/// 通用信息卡片容器
pub fn section_card<V>(title: &str, cx: &Context<V>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.45))
        .child(
            Label::new(title.to_string())
                .text_sm()
                .font_semibold()
                .text_color(cx.theme().foreground),
        )
}

/// 通用键值对展示行
pub fn kv_row<V>(label: &str, value: &str, cx: &Context<V>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .py_1()
        .child(
            Label::new(label.to_string())
                .text_xs()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            Label::new(value.to_string())
                .text_xs()
                .text_color(cx.theme().foreground),
        )
}
