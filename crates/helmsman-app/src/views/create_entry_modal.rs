use crate::app::{ActiveModal, HelmsmanWindow};
use crate::views::setting_row;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use grub_boot_reader::{CustomBootEntry, generate_custom_script};

/// 新建自定义引导条目的类型选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardEntryKind {
    Iso,
    Chainloader,
    Linux,
}

/// 新建自定义引导条目向导状态
pub struct CreateEntryWizard {
    pub kind: WizardEntryKind,
    pub title_input: Entity<InputState>,
    pub target_input: Entity<InputState>,
    pub initrd_input: Entity<InputState>,
    pub uuid_input: Entity<InputState>,
    pub cmdline_input: Entity<InputState>,
    pub submenu_input: Entity<InputState>,
}

impl CreateEntryWizard {
    /// 初始化向导表单状态
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            kind: WizardEntryKind::Iso,
            title_input: cx.new(|cx| InputState::new(window, cx).placeholder("如 Ubuntu Live ISO")),
            target_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("/boot/iso/live.iso")),
            initrd_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("/boot/initrd.img-...")),
            uuid_input: cx.new(|cx| InputState::new(window, cx).placeholder("0000-0000")),
            cmdline_input: cx.new(|cx| InputState::new(window, cx).placeholder("quiet splash")),
            submenu_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("留空置于主菜单根层级")),
        }
    }

    /// 构造用于预览或创建的 [`CustomBootEntry`]
    pub fn build_entry(&self, id: String, cx: &App) -> CustomBootEntry {
        let title = self.title_input.read(cx).value().trim().to_string();
        let target = self.target_input.read(cx).value().trim().to_string();
        let initrd = self.initrd_input.read(cx).value().trim().to_string();
        let uuid = self.uuid_input.read(cx).value().trim().to_string();
        let cmdline = self.cmdline_input.read(cx).value().trim().to_string();
        let submenu = self.submenu_input.read(cx).value().trim().to_string();

        let effective_title = if title.is_empty() {
            "未命名自定义引导项".to_string()
        } else {
            title
        };
        let mut entry = match self.kind {
            WizardEntryKind::Iso => {
                CustomBootEntry::new_iso_boot(id, effective_title, target, uuid, cmdline)
            }
            WizardEntryKind::Chainloader => {
                CustomBootEntry::new_chainloader(id, effective_title, target, uuid)
            }
            WizardEntryKind::Linux => CustomBootEntry::new_custom_linux(
                id,
                effective_title,
                target,
                initrd,
                uuid,
                cmdline,
            ),
        };
        entry.submenu_title = submenu;
        entry
    }
}

/// 渲染“新建自定义引导条目”交互向导弹窗（支持实时脚本预览）
pub fn render_create_entry_modal(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let kind = win.create_wizard.kind;
    let preview_entry = win.create_wizard.build_entry("preview_id".to_string(), cx);
    let script_preview =
        generate_custom_script(&[preview_entry]).unwrap_or_else(|e| format!("# 参数校验提示: {e}"));

    div()
        .flex()
        .flex_col()
        .w(px(680.0))
        .max_h(px(620.0))
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
                    Label::new("新建自定义引导条目向导")
                        .text_base()
                        .font_semibold(),
                )
                .child(
                    div().flex().items_center().gap_1p5().children(
                        [
                            (WizardEntryKind::Iso, "ISO 镜像"),
                            (WizardEntryKind::Chainloader, "EFI 链式加载"),
                            (WizardEntryKind::Linux, "Linux 内核"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(idx, (k, label))| {
                            let btn = Button::new(("wizard-kind", idx)).xsmall().label(label);
                            let btn = if kind == k {
                                btn.primary()
                            } else {
                                btn.outline()
                            };
                            btn.on_click(cx.listener(move |this, _, _, cx| {
                                this.create_wizard.kind = k;
                                cx.notify();
                            }))
                        }),
                    ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(setting_row(
                    "菜单显示标题",
                    "开机时在 GRUB 菜单中显示的名称",
                    div()
                        .w(px(340.0))
                        .child(Input::new(&win.create_wizard.title_input).small()),
                    cx,
                ))
                .child(setting_row(
                    match kind {
                        WizardEntryKind::Iso => "ISO 镜像路径",
                        WizardEntryKind::Chainloader => "EFI 引导文件路径",
                        WizardEntryKind::Linux => "Linux 内核路径 (vmlinuz)",
                    },
                    "支持根目录相对路径，如 /boot/iso/live.iso",
                    div()
                        .w(px(340.0))
                        .child(Input::new(&win.create_wizard.target_input).small()),
                    cx,
                ))
                .when(kind == WizardEntryKind::Linux, |el| {
                    el.child(setting_row(
                        "Initrd 内存盘路径",
                        "如 /boot/initrd.img-6.8.0-generic",
                        div()
                            .w(px(340.0))
                            .child(Input::new(&win.create_wizard.initrd_input).small()),
                        cx,
                    ))
                })
                .child(setting_row(
                    "目标分区 UUID",
                    "可通过 blkid 或 lsblk -f 查询",
                    div()
                        .w(px(340.0))
                        .child(Input::new(&win.create_wizard.uuid_input).small()),
                    cx,
                ))
                .when(kind != WizardEntryKind::Chainloader, |el| {
                    el.child(setting_row(
                        "附加内核参数",
                        "如 quiet splash",
                        div()
                            .w(px(340.0))
                            .child(Input::new(&win.create_wizard.cmdline_input).small()),
                        cx,
                    ))
                })
                .child(setting_row(
                    "归属子菜单（可选）",
                    "留空置于主菜单根层级",
                    div()
                        .w(px(340.0))
                        .child(Input::new(&win.create_wizard.submenu_input).small()),
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Label::new("生成脚本实时预览 (41_helmsman_custom)")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .max_h(px(120.0))
                        .p_2p5()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().secondary.opacity(0.35))
                        .overflow_y_scrollbar()
                        .child(Label::new(script_preview).text_xs()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("btn-wizard-cancel")
                        .outline()
                        .small()
                        .label("取消")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.active_modal = ActiveModal::None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("btn-wizard-confirm")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label("创建并加入草稿")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_create_wizard(window, cx);
                        })),
                ),
        )
}
