use crate::app::HelmsmanWindow;
use crate::backend;
use gpui_kit::*;

impl HelmsmanWindow {
    /// 调用特权服务转换系统字体为 GRUB `.pf2` 并写入草稿 `GRUB_FONT`
    pub fn convert_grub_font_action(&mut self, cx: &mut Context<Self>) {
        let src_path = self
            .extra_inputs
            .font_src_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let size_str = self
            .extra_inputs
            .font_size_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let out_name = self
            .extra_inputs
            .font_out_input
            .read(cx)
            .value()
            .trim()
            .to_string();

        if src_path.is_empty() {
            self.toast = Some((false, "请先选择或输入系统字体文件路径".to_string()));
            cx.notify();
            return;
        }
        let font_size: u16 = match size_str.parse() {
            Ok(v) if (8..=72).contains(&v) => v,
            _ => {
                self.toast = Some((false, "字号必须为 8~72 之间的整数".to_string()));
                cx.notify();
                return;
            }
        };

        if self.backend.is_demo_mode {
            let simulated = format!("/boot/grub/fonts/{out_name}.pf2");
            let _ = self.state.set_grub_font_path(Some(&simulated));
            self.toast = Some((true, format!("演示模式：已设置 GRUB_FONT={simulated}")));
            cx.notify();
            return;
        }

        match backend::convert_grub_font(&src_path, font_size, &out_name) {
            Ok(pf2_path) => {
                if let Err(e) = self.state.set_grub_font_path(Some(&pf2_path)) {
                    self.toast = Some((false, e.to_string()));
                } else {
                    self.toast = Some((
                        true,
                        format!("已生成字体 {pf2_path} 并填入 GRUB_FONT（审核保存后生效）"),
                    ));
                }
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }

    /// 切换 `/etc/grub.d/` 原生脚本的可执行权限（启用/禁用）
    pub fn toggle_grub_script_action(
        &mut self,
        script_name: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.backend.is_demo_mode {
            if let Some(script) = self
                .backend
                .grub_scripts
                .iter_mut()
                .find(|s| s.name == script_name)
            {
                script.enabled = enabled;
            }
            self.toast = Some((
                true,
                format!(
                    "演示模式：已{}脚本 {script_name}",
                    if enabled { "启用" } else { "禁用" }
                ),
            ));
            cx.notify();
            return;
        }

        match backend::set_grub_script_enabled(script_name, enabled) {
            Ok(()) => {
                if let Ok(refreshed) = backend::list_grub_scripts() {
                    self.backend.grub_scripts = refreshed;
                }
                self.toast = Some((
                    true,
                    format!(
                        "已{}原生脚本 {script_name}（点击顶部“审核并保存”重建菜单后生效）",
                        if enabled { "启用" } else { "禁用" }
                    ),
                ));
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }

    /// 执行 `grub-install` 重装引导程序到 MBR 或 EFI 分区
    pub fn perform_reinstall_grub(&mut self, cx: &mut Context<Self>) {
        let mode = if self.reinstall_mode_uefi {
            "uefi"
        } else {
            "bios"
        };
        let device_path = self
            .extra_inputs
            .install_device_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let efi_directory = self
            .extra_inputs
            .install_efi_input
            .read(cx)
            .value()
            .trim()
            .to_string();

        if self.backend.is_demo_mode {
            self.toast = Some((true, format!("演示模式：已模拟执行 grub-install ({mode})")));
            cx.notify();
            return;
        }

        match backend::reinstall_grub(mode, &device_path, &efi_directory) {
            Ok(msg) => {
                let summary = if msg.trim().is_empty() {
                    "GRUB 引导程序重装成功".to_string()
                } else {
                    format!("GRUB 引导程序重装完成: {msg}")
                };
                self.toast = Some((true, summary));
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }

    /// 切换目标系统挂载根目录（Live USB 修复模式）并重新加载配置
    pub fn switch_target_root(&mut self, target_root: &str, cx: &mut Context<Self>) {
        if self.backend.is_demo_mode {
            self.toast = Some((false, "演示模式下无法切换真实宿主机挂载根目录".to_string()));
            cx.notify();
            return;
        }

        match backend::set_target_root(target_root) {
            Ok(status) => {
                let new_root = status.target_root.clone();
                self.reload_from_backend(cx);
                self.toast = Some((true, format!("已切换目标引导环境至: {new_root}")));
            }
            Err(e) => {
                self.toast = Some((false, e));
            }
        }
        cx.notify();
    }
}
