use crate::app::HelmsmanWindow;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _};
use gpui_kit::*;

/// 渲染左侧引导菜单的快捷调整工具栏（对齐参考工具栏中的移除、排序与子菜单归组操作）
pub fn render_entry_action_bar(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let selected = win.state.get_selected_entry();
    let selected_path = selected.map(|e| e.full_path.clone());

    // 在受管自定义条目列表中查找当前选中项的位置与子菜单状态
    let custom_match = selected.and_then(|entry| {
        let entry_id = entry.id.as_deref()?;
        win.state
            .draft_custom_entries
            .iter()
            .enumerate()
            .find(|(_, c)| c.id == entry_id)
            .map(|(idx, c)| (idx, c.id.clone(), !c.submenu_title.is_empty()))
    });

    let custom_len = win.state.draft_custom_entries.len();
    let (can_remove, can_up, can_down, can_to_root, can_to_sub, custom_id) = match custom_match {
        Some((idx, id, in_submenu)) => (
            true,
            idx > 0,
            idx + 1 < custom_len,
            in_submenu,
            !in_submenu,
            id,
        ),
        None => (false, false, false, false, false, String::new()),
    };

    let id_remove = custom_id.clone();
    let id_up = custom_id.clone();
    let id_down = custom_id.clone();
    let id_root = custom_id.clone();
    let id_sub = custom_id;
    let default_path = selected_path.clone().unwrap_or_default();

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
                        .xsmall()
                        .icon(IconName::Delete)
                        .tooltip("移除选中的自定义引导条目（系统条目受保护不可删除）")
                        .disabled(!can_remove)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.remove_custom_entry(&id_remove) {
                                this.toast = Some((
                                    true,
                                    format!("已移除自定义条目 '{id_remove}'（审核保存后生效）"),
                                ));
                            }
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.0)).h(px(14.0)).bg(cx.theme().border))
                .child(
                    Button::new("toolbar-move-up")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ArrowUp)
                        .tooltip("将自定义条目上移一位")
                        .disabled(!can_up)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.move_custom_entry_up(&id_up) {
                                this.toast = Some((true, "已将自定义条目上移一位".to_string()));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("toolbar-move-down")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ArrowDown)
                        .tooltip("将自定义条目下移一位")
                        .disabled(!can_down)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.move_custom_entry_down(&id_down) {
                                this.toast = Some((true, "已将自定义条目下移一位".to_string()));
                            }
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.0)).h(px(14.0)).bg(cx.theme().border))
                .child(
                    Button::new("toolbar-to-root")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronLeft)
                        .tooltip("移出子菜单至主菜单根层级")
                        .disabled(!can_to_root)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.state.move_custom_entry_to_root(&id_root) {
                                this.toast = Some((true, "已移回主菜单根层级".to_string()));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("toolbar-to-submenu")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronRight)
                        .tooltip("归组移入“自定义引导项”子菜单")
                        .disabled(!can_to_sub)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this
                                .state
                                .move_custom_entry_to_submenu(&id_sub, "自定义引导项")
                            {
                                this.toast =
                                    Some((true, "已归组移入“自定义引导项”子菜单".to_string()));
                            }
                            cx.notify();
                        })),
                ),
        )
        .child(
            Button::new("toolbar-set-default")
                .outline()
                .xsmall()
                .icon(IconName::Check)
                .label("设为默认")
                .disabled(selected_path.is_none())
                .on_click(cx.listener(move |this, _, _, cx| {
                    match this.state.set_default_entry_semantic(&default_path) {
                        Ok(()) => {
                            this.toast =
                                Some((true, format!("已将 '{default_path}' 设为默认启动项草稿")));
                        }
                        Err(e) => {
                            this.toast = Some((false, e.to_string()));
                        }
                    }
                    cx.notify();
                })),
        )
}
