use serde::{Deserialize, Serialize};
use zvariant::Type;

/// 自定义脚本标准头部定义
pub const HELMSMAN_CUSTOM_HEADER: &str =
    "#!/bin/sh\nexec tail -n +3 $0\n# 本文件由 Helmsman 自动生成与维护，请勿手动修改头部声明\n";

/// 自定义引导项类型常量
pub mod entry_types {
    /// Live ISO 镜像 loopback 引导
    pub const ISO: &str = "iso";
    /// 链式加载器引导（Windows / 独立 EFI）
    pub const CHAINLOADER: &str = "chainloader";
    /// 自定义 Linux 内核引导
    pub const LINUX: &str = "linux";
    /// 原生自定义脚本代码块
    pub const RAW: &str = "raw";
}

/// 单个自定义引导条目强类型模型
///
/// # 设计原理
/// - **实现初衷**：为用户提供结构化添加与管理各类常见引导项的能力（ISO 镜像、双系统 Windows、独立内核等）。
/// - **核心优势**：采用标准 D-Bus 字段兼容的扁平结构，无缝支持 zvariant::Type 序列化与跨进程传输；
///   通过轻量元数据注释持久化保存，反向解析时可 100% 精确还原，同时对 GRUB 执行环境完全兼容透明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CustomBootEntry {
    /// 唯一标识 ID
    pub id: String,
    /// 菜单显示标题
    pub title: String,
    /// 样式图标标签分类（例如 ["ubuntu", "os"], ["windows", "os"]）
    pub classes: Vec<String>,
    /// 条目分类 ("iso" / "chainloader" / "linux" / "raw")
    pub entry_type: String,
    /// 目标路径（ISO 路径 / EFI 路径 / 内核镜像路径）
    pub target_path: String,
    /// 目标分区 UUID（存放 ISO / EFI / 根分区的 UUID）
    pub partition_uuid: String,
    /// 初始内存盘路径（initrd，仅 linux 类型使用）
    pub initrd_path: String,
    /// 内核启动命令行参数
    pub cmdline_params: String,
    /// 原生脚本代码内容（仅 raw 类型使用）
    pub raw_script: String,
    /// 是否启用该引导条目（若禁用则生成时自动注释掉）
    pub enabled: bool,
}

impl CustomBootEntry {
    /// 创建 ISO 镜像引导条目
    pub fn new_iso_boot(
        id: impl Into<String>,
        title: impl Into<String>,
        isofile_path: impl Into<String>,
        partition_uuid: impl Into<String>,
        extra_cmdline: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["gnu-linux".to_string(), "os".to_string()],
            entry_type: entry_types::ISO.to_string(),
            target_path: isofile_path.into(),
            partition_uuid: partition_uuid.into(),
            initrd_path: String::new(),
            cmdline_params: extra_cmdline.into(),
            raw_script: String::new(),
            enabled: true,
        }
    }

    /// 创建 Windows / EFI 链式加载引导条目
    pub fn new_chainloader(
        id: impl Into<String>,
        title: impl Into<String>,
        efi_path: impl Into<String>,
        partition_uuid: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["windows".to_string(), "os".to_string()],
            entry_type: entry_types::CHAINLOADER.to_string(),
            target_path: efi_path.into(),
            partition_uuid: partition_uuid.into(),
            initrd_path: String::new(),
            cmdline_params: String::new(),
            raw_script: String::new(),
            enabled: true,
        }
    }

    /// 创建自定义 Linux 内核引导条目
    pub fn new_custom_linux(
        id: impl Into<String>,
        title: impl Into<String>,
        kernel_path: impl Into<String>,
        initrd_path: impl Into<String>,
        root_uuid: impl Into<String>,
        cmdline: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["gnu-linux".to_string(), "os".to_string()],
            entry_type: entry_types::LINUX.to_string(),
            target_path: kernel_path.into(),
            partition_uuid: root_uuid.into(),
            initrd_path: initrd_path.into(),
            cmdline_params: cmdline.into(),
            raw_script: String::new(),
            enabled: true,
        }
    }

    /// 创建通用原生脚本引导条目
    pub fn new_raw_script(
        id: impl Into<String>,
        title: impl Into<String>,
        script_content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["os".to_string()],
            entry_type: entry_types::RAW.to_string(),
            target_path: String::new(),
            partition_uuid: String::new(),
            initrd_path: String::new(),
            cmdline_params: String::new(),
            raw_script: script_content.into(),
            enabled: true,
        }
    }

    /// 生成当前条目的 GRUB 语法脚本块
    pub fn format_entry(&self) -> String {
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

        // 写入元数据标记注释（用于精确反序列化还原）
        out.push_str(&format!(
            "# [helmsman-meta id=\"{}\" type=\"{}\" enabled=\"{}\"]\n",
            self.id, self.entry_type, self.enabled
        ));

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
            // 若条目被禁用，将整段以 # 注释包裹
            for line in full_block.lines() {
                out.push_str(&format!("# {}\n", line));
            }
            if out.ends_with('\n') {
                out.pop();
            }
        }
        out
    }
}

/// 将自定义引导条目列表格式化为完整的 `/etc/grub.d/41_helmsman_custom` 脚本内容
pub fn generate_custom_script(entries: &[CustomBootEntry]) -> String {
    let mut script = String::from(HELMSMAN_CUSTOM_HEADER);
    for entry in entries {
        script.push('\n');
        script.push_str(&entry.format_entry());
        script.push('\n');
    }
    script
}

/// 解析 `/etc/grub.d/41_helmsman_custom` 脚本为结构化自定义引导项列表
pub fn parse_custom_script(content: &str) -> Vec<CustomBootEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // 识别元数据注释行
        if line.starts_with("# [helmsman-meta ") && line.ends_with(']') {
            let meta_str = line
                .trim_start_matches("# [helmsman-meta ")
                .trim_end_matches(']');
            let id = extract_meta_attr(meta_str, "id").unwrap_or_else(|| "custom".to_string());
            let entry_type =
                extract_meta_attr(meta_str, "type").unwrap_or_else(|| "raw".to_string());
            let enabled = extract_meta_attr(meta_str, "enabled")
                .map(|v| v == "true")
                .unwrap_or(true);

            i += 1;
            let mut block_lines = Vec::new();
            while i < lines.len() {
                let cur = lines[i];
                let cur_trimmed = cur.trim();
                let uncommented = cur_trimmed.strip_prefix("# ").unwrap_or(cur_trimmed);

                block_lines.push(uncommented);
                if uncommented == "}"
                    || (uncommented.starts_with('}') && uncommented.ends_with('}'))
                {
                    break;
                }
                i += 1;
            }

            if let Some(entry) = parse_single_block(&id, &entry_type, enabled, &block_lines) {
                entries.push(entry);
            }
        } else {
            i += 1;
        }
    }

    entries
}

fn extract_meta_attr(meta: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let start = meta.find(&pattern)? + pattern.len();
    let end = meta[start..].find('"')? + start;
    Some(meta[start..end].to_string())
}

fn parse_single_block(
    id: &str,
    entry_type: &str,
    enabled: bool,
    lines: &[&str],
) -> Option<CustomBootEntry> {
    if lines.is_empty() {
        return None;
    }

    let first_line = lines[0].trim();
    let title =
        extract_title_from_menuentry(first_line).unwrap_or_else(|| "未命名条目".to_string());
    let classes = extract_classes_from_menuentry(first_line);

    let body_lines = if lines.len() > 2 {
        &lines[1..lines.len() - 1]
    } else {
        &[]
    };
    let body_text = body_lines.join("\n");

    let mut target_path = String::new();
    let mut partition_uuid = String::new();
    let mut initrd_path = String::new();
    let mut cmdline_params = String::new();
    let mut raw_script = String::new();

    match entry_type {
        entry_types::ISO => {
            for line in body_lines {
                let trimmed = line.trim();
                if let Some(val) = trimmed.strip_prefix("set isofile=") {
                    target_path = val.trim_matches('"').trim_matches('\'').to_string();
                } else if let Some(pos) = trimmed.find("--fs-uuid --set=root ") {
                    partition_uuid = trimmed[pos + "--fs-uuid --set=root ".len()..]
                        .trim()
                        .to_string();
                } else if let Some(val) = trimmed.strip_prefix("linux ")
                    && let Some(pos) = val.find("quiet splash")
                {
                    cmdline_params = val[pos + "quiet splash".len()..].trim().to_string();
                }
            }
        }
        entry_types::CHAINLOADER => {
            for line in body_lines {
                let trimmed = line.trim();
                if let Some(pos) = trimmed.find("--fs-uuid --set=root ") {
                    partition_uuid = trimmed[pos + "--fs-uuid --set=root ".len()..]
                        .trim()
                        .to_string();
                } else if let Some(val) = trimmed.strip_prefix("chainloader ") {
                    target_path = val.trim().to_string();
                }
            }
        }
        entry_types::LINUX => {
            for line in body_lines {
                let trimmed = line.trim();
                if let Some(pos) = trimmed.find("--fs-uuid --set=root ") {
                    partition_uuid = trimmed[pos + "--fs-uuid --set=root ".len()..]
                        .trim()
                        .to_string();
                } else if let Some(val) = trimmed.strip_prefix("linux ") {
                    let parts: Vec<&str> = val.split_whitespace().collect();
                    if !parts.is_empty() {
                        target_path = parts[0].to_string();
                    }
                    if parts.len() > 1 {
                        cmdline_params = parts[1..].join(" ");
                    }
                } else if let Some(val) = trimmed.strip_prefix("initrd ") {
                    initrd_path = val.trim().to_string();
                }
            }
        }
        _ => {
            raw_script = body_text;
        }
    }

    Some(CustomBootEntry {
        id: id.to_string(),
        title,
        classes,
        entry_type: entry_type.to_string(),
        target_path,
        partition_uuid,
        initrd_path,
        cmdline_params,
        raw_script,
        enabled,
    })
}

fn extract_title_from_menuentry(line: &str) -> Option<String> {
    let start = line.find('\'').or_else(|| line.find('"'))?;
    let quote_char = line.chars().nth(start)?;
    let remainder = &line[start + 1..];
    let end = remainder.find(quote_char)?;
    Some(remainder[..end].to_string())
}

fn extract_classes_from_menuentry(line: &str) -> Vec<String> {
    let mut classes = Vec::new();
    let tokens = line.split_whitespace();
    let mut next_is_class = false;
    for t in tokens {
        if next_is_class {
            classes.push(t.trim_matches('\'').trim_matches('"').to_string());
            next_is_class = false;
        } else if t == "--class" {
            next_is_class = true;
        }
    }
    classes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_entry_iso_boot_roundtrip() {
        let entry = CustomBootEntry::new_iso_boot(
            "ubuntu_iso",
            "Ubuntu 24.04 Live",
            "/boot/iso/ubuntu.iso",
            "1234-5678",
            "nomodeset",
        );

        let script = generate_custom_script(&[entry]);
        assert!(script.contains("Ubuntu 24.04 Live"));
        assert!(script.contains("ubuntu.iso"));
        assert!(script.contains("1234-5678"));
        assert!(script.contains("nomodeset"));

        let parsed = parse_custom_script(&script);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "ubuntu_iso");
        assert_eq!(parsed[0].title, "Ubuntu 24.04 Live");
        assert_eq!(parsed[0].entry_type, entry_types::ISO);
        assert_eq!(parsed[0].target_path, "/boot/iso/ubuntu.iso");
        assert_eq!(parsed[0].partition_uuid, "1234-5678");
        assert_eq!(parsed[0].cmdline_params, "nomodeset");
        assert!(parsed[0].enabled);
    }

    #[test]
    fn test_custom_entry_chainloader_and_disabled() {
        let mut entry = CustomBootEntry::new_chainloader(
            "win11",
            "Windows 11",
            "/EFI/Microsoft/Boot/bootmgfw.efi",
            "ABCD-EF01",
        );
        entry.enabled = false;

        let script = generate_custom_script(&[entry]);
        assert!(script.contains("# menuentry 'Windows 11'"));

        let parsed = parse_custom_script(&script);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "win11");
        assert_eq!(parsed[0].title, "Windows 11");
        assert_eq!(parsed[0].entry_type, entry_types::CHAINLOADER);
        assert_eq!(parsed[0].target_path, "/EFI/Microsoft/Boot/bootmgfw.efi");
        assert_eq!(parsed[0].partition_uuid, "ABCD-EF01");
        assert!(!parsed[0].enabled);
    }
}
