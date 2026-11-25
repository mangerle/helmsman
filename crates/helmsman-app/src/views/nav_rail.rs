use crate::app::{ActiveTab, HelmsmanWindow};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::*;

fn nav_item(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    target: ActiveTab,
    active: ActiveTab,
    collapsed: bool,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let is_active = active == target;
    let text_color = if is_active {
        cx.theme().primary
    } else {
        cx.theme().muted_foreground
    };

    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2p5()
        .w_full()
        .px_2p5()
        .py_2()
        .rounded_lg()
        .cursor_pointer()
        .bg(if is_active {
            cx.theme().primary.opacity(0.14)
        } else {
            gpui_kit::transparent_black()
        })
        .hover(|s| {
            s.bg(if is_active {
                cx.theme().primary.opacity(0.18)
            } else {
                cx.theme().secondary.opacity(0.6)
            })
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = target;
            cx.notify();
        }))
        .child(div().text_color(text_color).child(icon))
        .children((!collapsed).then(|| {
            Label::new(label)
                .text_xs()
                .font_medium()
                .text_color(if is_active {
                    cx.theme().foreground
                } else {
                    cx.theme().muted_foreground
                })
        }))
}

/// 渲染左侧现代导航栏（带矢量图标与折叠切换）
pub fn render_nav_rail(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let collapsed = win.nav_collapsed;
    let width = if collapsed { px(56.0) } else { px(176.0) };

    div()
        .flex()
        .flex_col()
        .justify_between()
        .w(width)
        .h_full()
        .px_2()
        .py_3()
        .border_r_1()
        .border_color(cx.theme().border.opacity(0.75))
        .bg(cx.theme().secondary.opacity(0.45))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_1()
                        .pb_2()
                        .children((!collapsed).then(|| {
                            Label::new("导航")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                        }))
                        .child(
                            Button::new("btn-toggle-nav")
                                .ghost()
                                .xsmall()
                                .icon(IconName::PanelLeft)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.nav_collapsed = !this.nav_collapsed;
                                    cx.notify();
                                })),
                        ),
                )
                .child(nav_item(
                    "nav-boot",
                    "启动项管理",
                    IconName::LayoutDashboard,
                    ActiveTab::BootEntries,
                    win.active_tab,
                    collapsed,
                    cx,
                ))
                .child(nav_item(
                    "nav-appearance",
                    "主题与外观",
                    IconName::Palette,
                    ActiveTab::Appearance,
                    win.active_tab,
                    collapsed,
                    cx,
                ))
                .child(nav_item(
                    "nav-snapshots",
                    "快照与回滚",
                    IconName::Folder,
                    ActiveTab::Snapshots,
                    win.active_tab,
                    collapsed,
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pt_2()
                .border_t_1()
                .border_color(cx.theme().border.opacity(0.5))
                .child(nav_item(
                    "nav-settings",
                    "偏好设置",
                    IconName::Settings,
                    ActiveTab::Settings,
                    win.active_tab,
                    collapsed,
                    cx,
                ))
                .child(nav_item(
                    "nav-about",
                    "更新与关于",
                    IconName::Info,
                    ActiveTab::About,
                    win.active_tab,
                    collapsed,
                    cx,
                )),
        )
}
