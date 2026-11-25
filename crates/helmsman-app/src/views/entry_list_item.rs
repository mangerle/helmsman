use crate::app::HelmsmanWindow;
use gpui_kit::component::label::Label;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use helmsman_ui::BootEntryItem;

/// 拖拽引导条目时传递的轻量载荷
#[derive(Clone)]
pub struct DragBootEntry {
    pub full_path: String,
    pub display_title: String,
}

/// 拖拽过程中的浮动视觉预览卡片
struct DragEntryPreview(String);

impl Render for DragEntryPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1p5()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().primary)
            .bg(cx.theme().background.opacity(0.95))
            .shadow_md()
            .child(Label::new(self.0.clone()).text_xs().font_semibold())
    }
}

/// 渲染单条引导菜单列表项（支持拖拽排序、右键上下文菜单与子菜单层级缩进）
pub fn render_boot_entry_row(
    win: &HelmsmanWindow,
    idx: usize,
    item: &BootEntryItem,
    selected_path: Option<&str>,
    indented: bool,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let is_selected = selected_path == Some(item.full_path.as_str());
    let full_path = item.full_path.clone();
    let display_title = item.display_title.clone();
    let is_custom = item.is_custom;
    let in_submenu = win.state.is_entry_in_submenu(&full_path);

    let drag_payload = DragBootEntry {
        full_path: full_path.clone(),
        display_title: display_title.clone(),
    };
    let drop_target = full_path.clone();
    let click_path = full_path.clone();

    let view = cx.entity().clone();
    let ctx_path = full_path.clone();
    let primary_border = cx.theme().primary;

    let row = div()
        .id(("boot-entry-row", idx))
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .p_2p5()
        .rounded_xl()
        .border_1()
        .border_color(if is_selected {
            cx.theme().primary
        } else {
            cx.theme().border.opacity(0.6)
        })
        .bg(if is_selected {
            cx.theme().primary.opacity(0.14)
        } else {
            cx.theme().background.opacity(0.75)
        })
        .hover(|s| {
            s.bg(if is_selected {
                cx.theme().primary.opacity(0.18)
            } else {
                cx.theme().secondary.opacity(0.5)
            })
        })
        .cursor_pointer()
        .on_drag(drag_payload, |drag, _, _, cx| {
            cx.new(|_| DragEntryPreview(drag.display_title.clone()))
        })
        .drag_over::<DragBootEntry>(move |style, _, _, _| {
            style.border_2().border_color(primary_border)
        })
        .on_drop(cx.listener(move |this, dragged: &DragBootEntry, _, cx| {
            if dragged.full_path != drop_target {
                this.state.push_undo_snapshot();
                if this.state.move_entry_to(&dragged.full_path, &drop_target) {
                    this.toast = Some((true, "已通过拖拽调整引导条目顺序".to_string()));
                }
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.select_boot_entry(&click_path, window, cx);
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2p5()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .w(px(5.0))
                        .h(px(26.0))
                        .rounded_full()
                        .bg(if item.is_default {
                            cx.theme().primary
                        } else if is_selected {
                            cx.theme().primary.opacity(0.45)
                        } else {
                            cx.theme().border
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .gap_0p5()
                        .child(Label::new(display_title).text_xs().font_semibold())
                        .child(
                            Label::new(match &item.submenu_name {
                                Some(sub) if !indented => format!("子菜单: {sub}"),
                                _ => item.full_path.clone(),
                            })
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .when(item.is_default, |el| {
                    el.child(
                        Tag::new()
                            .with_variant(TagVariant::Primary)
                            .xsmall()
                            .child("默认"),
                    )
                })
                .when(is_custom, |el| {
                    el.child(
                        Tag::new()
                            .with_variant(TagVariant::Info)
                            .xsmall()
                            .child("自定义"),
                    )
                }),
        )
        .context_menu(move |menu, _window, _cx| {
            let p_def = ctx_path.clone();
            let p_up = ctx_path.clone();
            let p_down = ctx_path.clone();
            let p_sub = ctx_path.clone();
            let p_hide = ctx_path.clone();
            let v1 = view.clone();
            let v2 = view.clone();
            let v3 = view.clone();
            let v4 = view.clone();
            let v5 = view.clone();

            menu.item(
                PopupMenuItem::new("设为默认启动项")
                    .icon(IconName::Check)
                    .on_click(move |_, _, cx| {
                        v1.update(cx, |this, cx| {
                            this.state.push_undo_snapshot();
                            match this.state.set_default_entry_semantic(&p_def) {
                                Ok(()) => {
                                    this.toast =
                                        Some((true, format!("已将 '{p_def}' 设为默认启动项草稿")));
                                }
                                Err(e) => {
                                    this.toast = Some((false, e.to_string()));
                                }
                            }
                            cx.notify();
                        });
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new("上移一位")
                    .icon(IconName::ArrowUp)
                    .on_click(move |_, _, cx| {
                        v2.update(cx, |this, cx| {
                            this.state.push_undo_snapshot();
                            let _ = this.state.move_entry_up(&p_up);
                            cx.notify();
                        });
                    }),
            )
            .item(
                PopupMenuItem::new("下移一位")
                    .icon(IconName::ArrowDown)
                    .on_click(move |_, _, cx| {
                        v3.update(cx, |this, cx| {
                            this.state.push_undo_snapshot();
                            let _ = this.state.move_entry_down(&p_down);
                            cx.notify();
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(if in_submenu {
                    "移出子菜单至根层级"
                } else {
                    "归组至“更多系统与工具”"
                })
                .icon(if in_submenu {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                })
                .on_click(move |_, _, cx| {
                    v4.update(cx, |this, cx| {
                        this.state.push_undo_snapshot();
                        if in_submenu {
                            let _ = this.state.move_entry_to_root(&p_sub);
                        } else {
                            let _ = this.state.move_entry_to_submenu(&p_sub, "更多系统与工具");
                        }
                        cx.notify();
                    });
                }),
            )
            .separator()
            .item(
                PopupMenuItem::new("移除（隐藏）此条目")
                    .icon(IconName::EyeOff)
                    .on_click(move |_, _, cx| {
                        v5.update(cx, |this, cx| {
                            this.state.push_undo_snapshot();
                            if this.state.hide_entry(&p_hide) {
                                this.toast =
                                    Some((true, format!("已移除条目 '{p_hide}'（可在下方恢复）")));
                            }
                            cx.notify();
                        });
                    }),
            )
        });

    if indented {
        div().pl_4().child(row).into_any_element()
    } else {
        row.into_any_element()
    }
}
