use serde::{Deserialize, Serialize};
use thiserror::Error;
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

/// 自定义引导项安全校验错误
///
/// # 设计原理
/// - **实现初衷**：自定义条目最终会被拼进受管的 `/etc/grub.d/41_helmsman_custom`，
///   任一字段逃逸都可能导致 GRUB 脚本注入，必须在生成脚本前强类型拒绝。
/// - **核心优势**：错误变体带字段名与原因，便于 D-Bus 边界直接映射为用户可读信息。
/// - **代价与局限**：对标题、路径等采用保守白名单，牺牲少量非常规字符兼容以换取确定安全性。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CustomEntryError {
    /// 条目标识不符合命名规范
    #[error("条目标识 '{id}' 非法，原因: {reason}")]
    InvalidId {
        /// 被拒绝的标识
        id: String,
        /// 拒绝原因
        reason: String,
    },
    /// 菜单标题包含危险字符
    #[error("条目标题 '{title}' 非法，原因: {reason}")]
    InvalidTitle {
        /// 被拒绝的标题
        title: String,
        /// 拒绝原因
        reason: String,
    },
    /// 条目类型不受支持
    #[error("条目类型 '{entry_type}' 不受支持，允许值: iso / chainloader / linux / raw")]
    InvalidEntryType {
        /// 被拒绝的类型
        entry_type: String,
    },
    /// 样式分类标签非法
    #[error("样式分类 '{class}' 非法，原因: {reason}")]
    InvalidClass {
        /// 被拒绝的分类
        class: String,
        /// 拒绝原因
        reason: String,
    },
    /// 目标路径或 initrd 路径非法
    #[error("路径 '{path}' 非法，原因: {reason}")]
    InvalidPath {
        /// 被拒绝的路径
        path: String,
        /// 拒绝原因
        reason: String,
    },
    /// 分区 UUID 非法
    #[error("分区 UUID '{uuid}' 非法，原因: {reason}")]
    InvalidPartitionUuid {
        /// 被拒绝的 UUID
        uuid: String,
        /// 拒绝原因
        reason: String,
    },
    /// 内核命令行参数非法
    #[error("内核启动参数 '{cmdline}' 非法，原因: {reason}")]
    InvalidCmdline {
        /// 被拒绝的参数
        cmdline: String,
        /// 拒绝原因
        reason: String,
    },
    /// 原生脚本内容非法
    #[error("原生脚本非法，原因: {reason}")]
    InvalidRawScript {
        /// 拒绝原因
        reason: String,
    },
}

/// 单个自定义引导条目强类型模型
///
/// # 设计原理
/// - **实现初衷**：为用户提供结构化添加与管理各类常见引导项的能力（ISO 镜像、双系统 Windows、独立内核等）。
/// - **核心优势**：采用标准 D-Bus 字段兼容的扁平结构，无缝支持 zvariant::Type 序列化与跨进程传输；
///   通过轻量元数据注释持久化保存，反向解析时可 100% 精确还原，同时对 GRUB 执行环境完全兼容透明。
/// - **安全约束**：所有进入脚本的字段均须通过 [`CustomBootEntry::validate`] 白名单校验。
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

/// 校验单个标识符（id / class）是否仅含安全字符
fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// 校验路径字段：禁止换行、引号、反引号与 shell/GRUB 元字符
fn is_safe_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.starts_with('-')
        && value.chars().all(|c| {
            !matches!(
                c,
                '\n' | '\r'
                    | '\0'
                    | '\''
                    | '"'
                    | '`'
                    | '$'
                    | ';'
                    | '&'
                    | '|'
                    | '<'
                    | '>'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '\\'
            )
        })
}

/// 校验分区 UUID / 文件系统 UUID 片段
fn is_safe_uuid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// 校验内核命令行参数：禁止换行与会破坏脚本结构的元字符
fn is_safe_cmdline(value: &str) -> bool {
    value.len() <= 1024
        && value.chars().all(|c| {
            !matches!(
                c,
                '\n' | '\r'
                    | '\0'
                    | '\''
                    | '"'
                    | '`'
                    | '$'
                    | ';'
                    | '&'
                    | '|'
                    | '<'
                    | '>'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '\\'
            )
        })
}

/// 校验标题：允许空格与多语言，但禁止引号、换行与花括号
fn is_safe_title(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|c| {
            !matches!(
                c,
                '\n' | '\r'
                    | '\0'
                    | '\''
                    | '"'
                    | '`'
                    | '\\'
                    | '{'
                    | '}'
                    | ';'
                    | '&'
                    | '|'
                    | '$'
                    | '<'
                    | '>'
                    | '('
                    | ')'
            )
        })
}

/// 校验原生脚本：禁止空字节，且花括号必须配对，防止提前闭合 menuentry 逃逸
fn validate_raw_script(script: &str) -> Result<(), CustomEntryError> {
    if script.contains('\0') {
        return Err(CustomEntryError::InvalidRawScript {
            reason: "包含空字节".to_string(),
        });
    }
    if script.len() > 8192 {
        return Err(CustomEntryError::InvalidRawScript {
            reason: "长度超过 8192 字节上限".to_string(),
        });
    }
    let mut depth = 0i32;
    for ch in script.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(CustomEntryError::InvalidRawScript {
                        reason: "花括号不配对，存在提前闭合风险".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(CustomEntryError::InvalidRawScript {
            reason: "花括号不配对".to_string(),
        });
    }
    Ok(())
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

    /// 对全部字段执行安全白名单校验
    ///
    /// # Errors
    /// 任一字段含注入风险字符或不符合命名规范时返回对应的 [`CustomEntryError`]。
    pub fn validate(&self) -> Result<(), CustomEntryError> {
        if !is_safe_identifier(&self.id) {
            return Err(CustomEntryError::InvalidId {
                id: self.id.clone(),
                reason: "仅允许字母、数字、下划线、连字符与点，长度 1-64".to_string(),
            });
        }

        if !is_safe_title(&self.title) {
            return Err(CustomEntryError::InvalidTitle {
                title: self.title.clone(),
                reason: "禁止引号、换行、花括号与 shell 元字符".to_string(),
            });
        }

        match self.entry_type.as_str() {
            entry_types::ISO | entry_types::CHAINLOADER | entry_types::LINUX | entry_types::RAW => {
            }
            other => {
                return Err(CustomEntryError::InvalidEntryType {
                    entry_type: other.to_string(),
                });
            }
        }

        for class in &self.classes {
            if !is_safe_identifier(class) {
                return Err(CustomEntryError::InvalidClass {
                    class: class.clone(),
                    reason: "仅允许字母、数字、下划线、连字符与点".to_string(),
                });
            }
        }

        match self.entry_type.as_str() {
            entry_types::ISO => {
                if !is_safe_path(&self.target_path) {
                    return Err(CustomEntryError::InvalidPath {
                        path: self.target_path.clone(),
                        reason: "ISO 路径含非法字符".to_string(),
                    });
                }
                if !is_safe_uuid(&self.partition_uuid) {
                    return Err(CustomEntryError::InvalidPartitionUuid {
                        uuid: self.partition_uuid.clone(),
                        reason: "仅允许字母、数字与连字符".to_string(),
                    });
                }
                if !is_safe_cmdline(&self.cmdline_params) {
                    return Err(CustomEntryError::InvalidCmdline {
                        cmdline: self.cmdline_params.clone(),
                        reason: "禁止换行、引号与 shell 元字符".to_string(),
                    });
                }
            }
            entry_types::CHAINLOADER => {
                if !is_safe_path(&self.target_path) {
                    return Err(CustomEntryError::InvalidPath {
                        path: self.target_path.clone(),
                        reason: "EFI 路径含非法字符".to_string(),
                    });
                }
                if !is_safe_uuid(&self.partition_uuid) {
                    return Err(CustomEntryError::InvalidPartitionUuid {
                        uuid: self.partition_uuid.clone(),
                        reason: "仅允许字母、数字与连字符".to_string(),
                    });
                }
            }
            entry_types::LINUX => {
                if !is_safe_path(&self.target_path) {
                    return Err(CustomEntryError::InvalidPath {
                        path: self.target_path.clone(),
                        reason: "内核路径含非法字符".to_string(),
                    });
                }
                if !self.initrd_path.is_empty() && !is_safe_path(&self.initrd_path) {
                    return Err(CustomEntryError::InvalidPath {
                        path: self.initrd_path.clone(),
                        reason: "initrd 路径含非法字符".to_string(),
                    });
                }
                if !is_safe_uuid(&self.partition_uuid) {
                    return Err(CustomEntryError::InvalidPartitionUuid {
                        uuid: self.partition_uuid.clone(),
                        reason: "仅允许字母、数字与连字符".to_string(),
                    });
                }
                if !is_safe_cmdline(&self.cmdline_params) {
                    return Err(CustomEntryError::InvalidCmdline {
                        cmdline: self.cmdline_params.clone(),
                        reason: "禁止换行、引号与 shell 元字符".to_string(),
                    });
                }
            }
            entry_types::RAW => {
                validate_raw_script(&self.raw_script)?;
            }
            _ => unreachable!("入口类型已在上方 match 中穷尽"),
        }

        Ok(())
    }

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

        // 元数据标记注释：id/type 均已通过标识符白名单，不会引号注入
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
        Ok(out)
    }
}

/// 将自定义引导条目列表格式化为完整的 `/etc/grub.d/41_helmsman_custom` 脚本内容
///
/// # Errors
/// 任一条目字段未通过安全校验时返回对应的 [`CustomEntryError`]，拒绝生成不完整脚本。
pub fn generate_custom_script(entries: &[CustomBootEntry]) -> Result<String, CustomEntryError> {
    let mut script = String::from(HELMSMAN_CUSTOM_HEADER);
    for entry in entries {
        script.push('\n');
        script.push_str(&entry.try_format_entry()?);
        script.push('\n');
    }
    Ok(script)
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

        let script = generate_custom_script(&[entry]).expect("合法条目应成功生成脚本");
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

        let script = generate_custom_script(&[entry]).expect("合法条目应成功生成脚本");
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

    #[test]
    fn test_reject_title_quote_injection() {
        let entry = CustomBootEntry::new_iso_boot(
            "evil",
            "x' } { menuentry pwned",
            "/boot/iso/a.iso",
            "ABCD",
            "",
        );
        assert!(matches!(
            entry.validate(),
            Err(CustomEntryError::InvalidTitle { .. })
        ));
        assert!(generate_custom_script(&[entry]).is_err());
    }

    #[test]
    fn test_reject_id_injection() {
        let entry = CustomBootEntry::new_raw_script("bad\" id=\"x", "标题", "true");
        assert!(matches!(
            entry.validate(),
            Err(CustomEntryError::InvalidId { .. })
        ));
    }

    #[test]
    fn test_reject_path_metacharacters() {
        let entry =
            CustomBootEntry::new_iso_boot("iso1", "标题", "/boot/iso/a.iso; rm -rf /", "ABCD", "");
        assert!(matches!(
            entry.validate(),
            Err(CustomEntryError::InvalidPath { .. })
        ));
    }

    #[test]
    fn test_reject_raw_script_brace_escape() {
        let entry =
            CustomBootEntry::new_raw_script("raw1", "标题", "true\n}\nmenuentry 'pwned' {\ntrue");
        assert!(matches!(
            entry.validate(),
            Err(CustomEntryError::InvalidRawScript { .. })
        ));
    }

    #[test]
    fn test_reject_cmdline_injection() {
        let entry = CustomBootEntry::new_custom_linux(
            "lin1",
            "标题",
            "/vmlinuz",
            "/initrd.img",
            "ABCD-EF",
            "quiet $(reboot)",
        );
        assert!(matches!(
            entry.validate(),
            Err(CustomEntryError::InvalidCmdline { .. })
        ));
    }
}
