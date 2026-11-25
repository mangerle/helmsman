use crate::app::{ActiveTab, HelmsmanWindow};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::*;

fn nav_button(
    id: &'static str,
    label: &'static str,
    short_label: &'static str,
    target: ActiveTab,
    active: ActiveTab,
    collapsed: bool,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let is_active = active == target;
    let display = if collapsed { short_label } else { label };
    let btn = Button::new(id).small().label(display);
    let btn = if is_active {
        btn.primary()
    } else {
        btn.ghost()
    };
    btn.w_full().on_click(cx.listener(move |this, _, _, cx| {
        this.active_tab = target;
        cx.notify();
    }))
}

/// 渲染左侧主导航栏（支持展开与折叠）
pub fn render_nav_rail(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let collapsed = win.nav_collapsed;
    let width = if collapsed { px(68.0) } else { px(164.0) };

    div()
        .flex()
        .flex_col()
        .justify_between()
        .w(width)
        .h_full()
        .p_2()
        .border_r_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.6))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(
                    Button::new("btn-toggle-nav")
                        .ghost()
                        .xsmall()
                        .label(if collapsed { "展开 >>" } else { "<< 收起" })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.nav_collapsed = !this.nav_collapsed;
                            cx.notify();
                        })),
                )
                .child(nav_button(
                    "nav-boot",
                    "启动项",
                    "启动",
                    ActiveTab::BootEntries,
                    win.active_tab,
                    collapsed,
                    cx,
                ))
                .child(nav_button(
                    "nav-appearance",
                    "外观",
                    "外观",
                    ActiveTab::Appearance,
                    win.active_tab,
                    collapsed,
                    cx,
                ))
                .child(nav_button(
                    "nav-snapshots",
                    "快照",
                    "快照",
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
                .gap_1p5()
                .child(nav_button(
                    "nav-settings",
                    "设置",
                    "设置",
                    ActiveTab::Settings,
                    win.active_tab,
                    collapsed,
                    cx,
                ))
                .child(nav_button(
                    "nav-about",
                    "关于",
                    "关于",
                    ActiveTab::About,
                    win.active_tab,
                    collapsed,
                    cx,
                )),
        )
}
