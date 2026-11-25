use crate::app::HelmsmanWindow;
use crate::views::setting_row;
use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _};
use gpui_kit::*;

/// 渲染左侧引导菜单的快捷调整工具栏（支持所有可见的系统/自定义条目）
pub fn render_entry_action_bar(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let items = win.state.get_entry_items();
    let selected_path = win.state.get_selected_entry().map(|e| e.full_path.clone());
    let selected_idx = selected_path
        .as_deref()
        .and_then(|p| items.iter().position(|item| item.full_path == p));

    let has_selected = selected_path.is_some();
    let can_up = selected_idx.is_some_and(|i| i > 0);
    let can_down = selected_idx.is_some_and(|i| i + 1 < items.len());
    let in_submenu = selected_path
        .as_deref()
        .is_some_and(|p| win.state.is_entry_in_submenu(p));

    let path_remove = selected_path.clone().unwrap_or_default();
    let path_up = selected_path.clone().unwrap_or_default();
    let path_down = selected_path.clone().unwrap_or_default();
    let path_root = selected_path.clone().unwrap_or_default();
    let path_sub = selected_path.clone().unwrap_or_default();
    let path_default = selected_path.unwrap_or_default();

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_1()
        .pt_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new("toolbar-remove-entry")
                        .ghost()
                        .small()
                        .icon(AssetIcon::Trash)
                        .tooltip("移除（隐藏）当前选中的引导条目，可在下方列表恢复")
                        .disabled(!has_selected)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.hide_entry(&path_remove) {
                                this.toast = Some((
                                    true,
                                    format!(
                                        "已移除条目 '{path_remove}'（可在下方“已移除的条目”一键恢复）"
                                    ),
                                ));
                            }
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.0)).h(px(16.0)).bg(cx.theme().border))
                .child(
                    Button::new("toolbar-move-up")
                        .ghost()
                        .small()
                        .icon(AssetIcon::ArrowUp)
                        .tooltip("将当前选中条目上移一位")
                        .disabled(!can_up)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.move_entry_up(&path_up) {
                                this.toast = Some((true, "已将引导条目上移一位".to_string()));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("toolbar-move-down")
                        .ghost()
                        .small()
                        .icon(AssetIcon::ArrowDown)
                        .tooltip("将当前选中条目下移一位")
                        .disabled(!can_down)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.move_entry_down(&path_down) {
                                this.toast = Some((true, "已将引导条目下移一位".to_string()));
                            }
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.0)).h(px(16.0)).bg(cx.theme().border))
                .child(
                    Button::new("toolbar-to-root")
                        .ghost()
                        .small()
                        .icon(AssetIcon::ListIndentDecrease)
                        .tooltip("移出子菜单至主菜单根层级")
                        .disabled(!has_selected || !in_submenu)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.move_entry_to_root(&path_root) {
                                this.toast = Some((true, "已移回主菜单根层级".to_string()));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("toolbar-to-submenu")
                        .ghost()
                        .small()
                        .icon(AssetIcon::ListIndentIncrease)
                        .tooltip("归组移入“更多系统与工具”子菜单")
                        .disabled(!has_selected || in_submenu)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this
                                .state
                                .move_entry_to_submenu(&path_sub, "更多系统与工具")
                            {
                                this.toast =
                                    Some((true, "已归组移入“更多系统与工具”子菜单".to_string()));
                            }
                            cx.notify();
                        })),
                ),
        )
        .child(
            Button::new("toolbar-set-default")
                .outline()
                .small()
                .icon(AssetIcon::Check)
                .label("设为默认")
                .disabled(!has_selected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    match this.state.set_default_entry_semantic(&path_default) {
                        Ok(()) => {
                            this.toast = Some((
                                true,
                                format!("已将 '{path_default}' 设为默认启动项草稿"),
                            ));
                        }
                        Err(e) => {
                            this.toast = Some((false, e.to_string()));
                        }
                    }
                    cx.notify();
                })),
        )
}

/// 渲染“已移除的条目”暂存恢复面板（对齐参考图右侧的恢复能力）
pub fn render_hidden_entries_panel(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let hidden = win.state.list_hidden_entries();
    if hidden.is_empty() {
        return div().into_any_element();
    }

    div()
        .flex()
        .flex_col()
        .gap_1p5()
        .px_3()
        .py_2p5()
        .border_t_1()
        .border_color(cx.theme().border.opacity(0.65))
        .bg(cx.theme().secondary.opacity(0.35))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    Label::new("已移除的条目")
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    Tag::new()
                        .with_variant(TagVariant::Secondary)
                        .xsmall()
                        .child(format!("{}", hidden.len())),
                ),
        )
        .children(hidden.into_iter().enumerate().map(|(idx, item)| {
            let restore_path = item.full_path.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(cx.theme().background.opacity(0.7))
                .child(
                    Label::new(item.display_title)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    Button::new(("restore-entry", idx))
                        .ghost()
                        .xsmall()
                        .icon(AssetIcon::Undo2)
                        .label("恢复")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.restore_entry(&restore_path) {
                                this.toast =
                                    Some((true, format!("已恢复引导条目 '{restore_path}'")));
                            }
                            cx.notify();
                        })),
                )
        }))
        .into_any_element()
}

/// 渲染受管自定义条目的 GRUB 菜单显示名称编辑行（实际写入 41_helmsman_custom）
pub fn render_custom_title_row(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    setting_row(
        "GRUB 菜单显示名称",
        "修改实际开机引导菜单中展示的条目名称（仅对自定义条目生效）",
        div()
            .flex()
            .items_center()
            .gap_1p5()
            .w(px(280.0))
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&win.custom_title_input).small()),
            )
            .child(
                Button::new("btn-save-custom-title")
                    .outline()
                    .xsmall()
                    .label("更新名称")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_selected_custom_title(cx);
                    })),
            ),
        cx,
    )
}
