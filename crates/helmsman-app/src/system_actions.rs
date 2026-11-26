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
}
