use super::model::{CustomBootEntry, CustomEntryError, HELMSMAN_CUSTOM_HEADER, entry_types};
use super::validate::{validate_custom_script_structure, validate_entry_list};

impl CustomBootEntry {
    /// 生成当前条目的 GRUB 语法脚本块
    ///
    /// # Errors
    /// 字段未通过 [`Self::validate`] 时返回 [`CustomEntryError`]。
    pub fn try_format_entry(&self) -> Result<String, CustomEntryError> {
        self.validate()?;

        let mut out = String::with_capacity(256);
        let class_args = if self.classes.is_empty() {
            String::new()
        } else {
            self.classes
                .iter()
                .map(|c| format!("--class {}", c))
                .collect::<Vec<_>>()
                .join(" ")
        };

        if self.submenu_title.is_empty() {
            out.push_str(&format!(
                "# [helmsman-meta id=\"{}\" type=\"{}\" enabled=\"{}\"]\n",
                self.id, self.entry_type, self.enabled
            ));
        } else {
            out.push_str(&format!(
                "# [helmsman-meta id=\"{}\" type=\"{}\" enabled=\"{}\" submenu=\"{}\"]\n",
                self.id, self.entry_type, self.enabled, self.submenu_title
            ));
        }

        let entry_body = match self.entry_type.as_str() {
            entry_types::ISO => {
                let extra = if self.cmdline_params.trim().is_empty() {
                    String::new()
                } else {
                    format!(" {}", self.cmdline_params.trim())
                };
                format!(
                    "    rmmod tpm\n    set isofile=\"{}\"\n    search --no-floppy --fs-uuid --set=root {}\n    loopback loop ($root)$isofile\n    linux (loop)/casper/vmlinuz boot=casper iso-scan/filename=$isofile quiet splash{}\n    initrd (loop)/casper/initrd\n",
                    self.target_path, self.partition_uuid, extra
                )
            }
            entry_types::CHAINLOADER => {
                format!(
                    "    insmod part_gpt\n    insmod fat\n    search --no-floppy --fs-uuid --set=root {}\n    chainloader {}\n",
                    self.partition_uuid, self.target_path
                )
            }
            entry_types::LINUX => {
                let initrd_line = if self.initrd_path.trim().is_empty() {
                    String::new()
                } else {
                    format!("    initrd {}\n", self.initrd_path)
                };
                format!(
                    "    search --no-floppy --fs-uuid --set=root {}\n    linux {} root=UUID={} {}\n{}",
                    self.partition_uuid,
                    self.target_path,
                    self.partition_uuid,
                    self.cmdline_params,
                    initrd_line
                )
            }
            _ => {
                format!("    {}\n", self.raw_script.trim())
            }
        };

        let menuentry_line = if class_args.is_empty() {
            format!("menuentry '{}' {{\n", self.title)
        } else {
            format!("menuentry '{}' {} {{\n", self.title, class_args)
        };

        let full_block = format!("{}{}}}", menuentry_line, entry_body);

        if self.enabled {
            out.push_str(&full_block);
        } else {
            for line in full_block.lines() {
                out.push_str(&format!("# {}\n", line));
            }
            if out.ends_with('\n') {
                out.pop();
            }
        }
        Ok(out)
    }
}

/// 将自定义引导条目列表格式化为完整的 `/etc/grub.d/41_helmsman_custom` 脚本内容
///
/// 根层级条目按原序输出；同名子菜单内的条目聚合为 `submenu` 块。
///
/// # Errors
/// 任一条目字段未通过安全校验，或列表存在 ID/标题冲突时返回对应的 [`CustomEntryError`]。
pub fn generate_custom_script(entries: &[CustomBootEntry]) -> Result<String, CustomEntryError> {
    validate_entry_list(entries)?;
    let mut script = String::from(HELMSMAN_CUSTOM_HEADER);

    for entry in entries.iter().filter(|e| e.submenu_title.is_empty()) {
        script.push('\n');
        script.push_str(&entry.try_format_entry()?);
        script.push('\n');
    }

    let mut submenu_order: Vec<&str> = Vec::with_capacity(4);
    for entry in entries {
        if !entry.submenu_title.is_empty() && !submenu_order.contains(&entry.submenu_title.as_str())
        {
            submenu_order.push(&entry.submenu_title);
        }
    }

    for submenu_title in submenu_order {
        script.push('\n');
        script.push_str(&format!("submenu '{}' {{\n", submenu_title));
        for entry in entries.iter().filter(|e| e.submenu_title == submenu_title) {
            script.push_str(&entry.try_format_entry()?);
            script.push('\n');
        }
        script.push_str("}\n");
    }

    validate_custom_script_structure(&script)?;
    Ok(script)
}
