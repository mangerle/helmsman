use crate::backend::{self, BackendSnapshot};
use crate::views;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::label::Label;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::theme::{Theme, ThemeMode as KitThemeMode};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use helmsman_ui::{
    AppState, DiffViewModel, MenuPreviewModel, ResolvedTheme, SystemColorScheme, ThemeMode,
};
use helmsman_updater::UpdateInfo;
use tracing::{info, warn};

/// 应用自更新状态（基于 shipup）
#[derive(Debug, Clone)]
pub enum AppUpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(Box<UpdateInfo>),
    Installing(String),
    ReadyToRestart(String),
    Error(String),
}

/// 主导航标签页
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    BootEntries,
    Appearance,
    Snapshots,
    Settings,
    About,
}

/// 当前打开的模态窗口类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    ApplyDiffReview,
    MenuPreview,
}

/// Helmsman 桌面主窗口实体
pub struct HelmsmanWindow {
    /// 无头应用状态机（持有基准配置与草稿配置）
    pub state: AppState,
    /// 守护进程连接与只读系统数据快照
    pub backend: BackendSnapshot,
    /// 当前激活导航页
    pub active_tab: ActiveTab,
    /// 左侧导航栏是否折叠
    pub nav_collapsed: bool,
    /// 当前弹窗状态
    pub active_modal: ActiveModal,
    /// 引导菜单模拟预览状态
    pub preview_state: Option<MenuPreviewModel>,
    /// 底部/顶部临时反馈提示 (是否成功, 文本)
    pub toast: Option<(bool, String)>,
    /// 自定义在线主题 HTTPS URL 输入框状态
    pub theme_url_input: Entity<InputState>,
    /// 是否正在执行主题下载或安装
    pub is_installing_theme: bool,
    /// 应用自身更新状态
    pub update_status: AppUpdateStatus,
}

impl HelmsmanWindow {
    /// 从后端快照构建窗口状态
    pub fn new(backend: BackendSnapshot, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = AppState::new_from_content(&backend.default_config, &backend.grub_cfg)
            .with_custom_entries(backend.custom_entries.clone())
            .with_aliases(backend.aliases.clone());
        let theme_url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入主题归档 HTTPS URL (.zip / .tar.gz / .tar)")
        });

        Self {
            state,
            backend,
            active_tab: ActiveTab::BootEntries,
            nav_collapsed: false,
            active_modal: ActiveModal::None,
            preview_state: None,
            toast: None,
            theme_url_input,
            is_installing_theme: false,
            update_status: AppUpdateStatus::Idle,
        }
    }

    /// 同步 `helmsman_ui::ThemeMode` 到 `gpui-component` 全局主题
    pub fn sync_kit_theme(&self, window: &mut Window, cx: &mut App) {
        let resolved = self
            .state
            .get_theme_mode()
            .resolve(SystemColorScheme::PreferDark);
        let kit_mode = match resolved {
            ResolvedTheme::Light => KitThemeMode::Light,
            ResolvedTheme::Dark | ResolvedTheme::HighContrast => KitThemeMode::Dark,
        };
        Theme::change(kit_mode, Some(window), cx);
    }

    /// 切换界面主题模式
    pub fn set_ui_theme(&mut self, mode: ThemeMode, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_theme_mode(mode);
        self.sync_kit_theme(window, cx);
        cx.notify();
    }

    /// 重新从守护进程加载全量配置与快照
    pub fn reload_from_backend(&mut self, cx: &mut Context<Self>) {
        let snapshot = BackendSnapshot::load();
        self.state = AppState::new_from_content(&snapshot.default_config, &snapshot.grub_cfg)
            .with_custom_entries(snapshot.custom_entries.clone())
            .with_aliases(snapshot.aliases.clone());
        self.backend = snapshot;
        self.toast = Some((true, "已重新同步最新引导配置与系统状态".to_string()));
        cx.notify();
    }

    /// 打开保存前 Diff 审核弹窗
    pub fn open_apply_modal(&mut self, cx: &mut Context<Self>) {
        if !self.state.has_unsaved_changes() {
            self.toast = Some((true, "当前草稿没有任何待提交的更改".to_string()));
            cx.notify();
            return;
        }
        self.active_modal = ActiveModal::ApplyDiffReview;
        cx.notify();
    }

    /// 打开 GRUB 启动菜单模拟预览弹窗
    pub fn open_preview_modal(&mut self, cx: &mut Context<Self>) {
        let preview = MenuPreviewModel::from_app_state(&self.state);
        self.preview_state = Some(preview);
        self.active_modal = ActiveModal::MenuPreview;
        cx.notify();
    }

    /// 确认提交草稿修改至特权守护进程
    pub fn commit_changes(&mut self, cx: &mut Context<Self>) {
        if self.backend.is_demo_mode {
            self.state.original_config = self.state.draft_config.clone();
            self.state.original_custom_entries = self.state.draft_custom_entries.clone();
            self.active_modal = ActiveModal::None;
            self.toast = Some((
                true,
                "演示模式：已更新内存基准配置（未写入真实系统磁盘）".to_string(),
            ));
            cx.notify();
            return;
        }

        let mut ok_msgs = Vec::new();
        if self.state.original_config != self.state.draft_config {
            let new_cfg = self.state.draft_config.serialize();
            match backend::apply_config_changes(&new_cfg, "通过 Helmsman 桌面端更新引导配置")
            {
                Ok(res) => {
                    info!("配置提交成功，快照 ID: {}", res.snapshot_id);
                    ok_msgs.push(format!("配置已保存 (快照 {})", res.snapshot_id));
                }
                Err(e) => {
                    warn!("提交配置变更失败: {e}");
                    self.toast = Some((false, e));
                    cx.notify();
                    return;
                }
            }
        }

        if self.state.has_custom_entry_changes() {
            match backend::apply_custom_entries(
                &self.state.draft_custom_entries,
                "通过 Helmsman 桌面端更新自定义引导项",
            ) {
                Ok(res) => {
                    info!("自定义条目提交成功，快照 ID: {}", res.snapshot_id);
                    ok_msgs.push("自定义引导项已同步".to_string());
                }
                Err(e) => {
                    warn!("提交自定义引导项失败: {e}");
                    self.toast = Some((false, e));
                    cx.notify();
                    return;
                }
            }
        }

        self.active_modal = ActiveModal::None;
        self.reload_from_backend(cx);
        if !ok_msgs.is_empty() {
            self.toast = Some((true, ok_msgs.join("；")));
        }
        cx.notify();
    }

    /// 回滚至指定快照
    pub fn perform_rollback(&mut self, snapshot_id: &str, cx: &mut Context<Self>) {
        if self.backend.is_demo_mode {
            self.toast = Some((false, "演示模式下无法执行磁盘快照回滚".to_string()));
            cx.notify();
            return;
        }
        match backend::rollback_snapshot(snapshot_id) {
            Ok(()) => {
                self.reload_from_backend(cx);
                self.toast = Some((true, format!("已成功回滚至快照 {snapshot_id}")));
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }

    /// 删除指定快照
    pub fn perform_delete_snapshot(&mut self, snapshot_id: &str, cx: &mut Context<Self>) {
        if self.backend.is_demo_mode {
            return;
        }
        match backend::delete_snapshot(snapshot_id) {
            Ok(()) => {
                self.backend.snapshots.retain(|s| s.id != snapshot_id);
                self.toast = Some((true, format!("已删除快照 {snapshot_id}")));
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }

    /// 渲染顶部工具栏
    fn render_topbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_changes = self.state.has_unsaved_changes();
        let diff_summary = if has_changes {
            let report = self.state.compute_diff();
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
            .px_4()
            .py_2p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Label::new("Helmsman")
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground),
                    )
                    .child(
                        Tag::new()
                            .with_variant(if self.backend.is_demo_mode {
                                TagVariant::Warning
                            } else {
                                TagVariant::Success
                            })
                            .small()
                            .child(self.backend.status_note.clone()),
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
                            .label("刷新")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload_from_backend(cx);
                            })),
                    )
                    .child(
                        Button::new("btn-preview")
                            .outline()
                            .small()
                            .label("菜单预览")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_preview_modal(cx);
                            })),
                    )
                    .when(has_changes, |el| {
                        el.child(
                            Button::new("btn-reset")
                                .ghost()
                                .small()
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
                            .disabled(!has_changes)
                            .label("审核并保存")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_apply_modal(cx);
                            })),
                    ),
            )
    }
}

impl Render for HelmsmanWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_topbar(cx))
            .when_some(self.toast.clone(), |el, (ok, msg)| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_1p5()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(if ok {
                            cx.theme().secondary
                        } else {
                            cx.theme().danger.opacity(0.15)
                        })
                        .child(Label::new(msg).text_xs())
                        .child(
                            Button::new("btn-dismiss-toast")
                                .ghost()
                                .xsmall()
                                .label("关闭")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toast = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(views::nav_rail::render_nav_rail(self, cx))
                    .child(
                        div().flex_1().min_w_0().min_h_0().overflow_hidden().child(
                            match self.active_tab {
                                ActiveTab::BootEntries => {
                                    views::boot_entries::render_boot_entries(self, cx)
                                        .into_any_element()
                                }
                                ActiveTab::Appearance => {
                                    views::appearance::render_appearance(self, cx)
                                        .into_any_element()
                                }
                                ActiveTab::Snapshots => {
                                    views::snapshots::render_snapshots(self, cx).into_any_element()
                                }
                                ActiveTab::Settings => {
                                    views::settings_about::render_settings(self, cx)
                                        .into_any_element()
                                }
                                ActiveTab::About => {
                                    views::settings_about::render_about(self, cx).into_any_element()
                                }
                            },
                        ),
                    ),
            )
            .when(self.active_modal != ActiveModal::None, |el| {
                el.child(views::modals::render_active_modal(self, cx))
            })
    }
}
