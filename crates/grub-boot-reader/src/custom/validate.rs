use super::model::{CustomBootEntry, CustomEntryError, entry_types};
use std::collections::HashSet;

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

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

fn is_safe_uuid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

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

        for class in &self.classes {
            if !is_safe_identifier(class) {
                return Err(CustomEntryError::InvalidClass {
                    class: class.clone(),
                    reason: "仅允许字母、数字、下划线、连字符与点".to_string(),
                });
            }
        }

        if !self.submenu_title.is_empty() && !is_safe_title(&self.submenu_title) {
            return Err(CustomEntryError::InvalidTitle {
                title: self.submenu_title.clone(),
                reason: "子菜单标题禁止引号、换行、花括号与 shell 元字符".to_string(),
            });
        }

        self.validate_type_specific_fields()
    }

    fn validate_type_specific_fields(&self) -> Result<(), CustomEntryError> {
        match self.entry_type.as_str() {
            entry_types::ISO => {
                check_path(&self.target_path, "ISO 路径含非法字符")?;
                check_uuid(&self.partition_uuid)?;
                check_cmdline(&self.cmdline_params)
            }
            entry_types::CHAINLOADER => {
                check_path(&self.target_path, "EFI 路径含非法字符")?;
                check_uuid(&self.partition_uuid)
            }
            entry_types::LINUX => {
                check_path(&self.target_path, "内核路径含非法字符")?;
                if !self.initrd_path.is_empty() {
                    check_path(&self.initrd_path, "initrd 路径含非法字符")?;
                }
                check_uuid(&self.partition_uuid)?;
                check_cmdline(&self.cmdline_params)
            }
            entry_types::RAW => validate_raw_script(&self.raw_script),
            other => Err(CustomEntryError::InvalidEntryType {
                entry_type: other.to_string(),
            }),
        }
    }
}

fn check_path(path: &str, reason: &str) -> Result<(), CustomEntryError> {
    if is_safe_path(path) {
        Ok(())
    } else {
        Err(CustomEntryError::InvalidPath {
            path: path.to_string(),
            reason: reason.to_string(),
        })
    }
}

fn check_uuid(uuid: &str) -> Result<(), CustomEntryError> {
    if is_safe_uuid(uuid) {
        Ok(())
    } else {
        Err(CustomEntryError::InvalidPartitionUuid {
            uuid: uuid.to_string(),
            reason: "仅允许字母、数字与连字符".to_string(),
        })
    }
}

fn check_cmdline(cmdline: &str) -> Result<(), CustomEntryError> {
    if is_safe_cmdline(cmdline) {
        Ok(())
    } else {
        Err(CustomEntryError::InvalidCmdline {
            cmdline: cmdline.to_string(),
            reason: "禁止换行、引号与 shell 元字符".to_string(),
        })
    }
}

/// 校验条目列表级冲突（ID 重复、标题重复）
///
/// # Errors
/// 当存在重复 ID 或重复标题时返回 [`CustomEntryError::Conflict`]。
pub fn validate_entry_list(entries: &[CustomBootEntry]) -> Result<(), CustomEntryError> {
    let mut seen_ids = HashSet::with_capacity(entries.len());
    let mut seen_titles = HashSet::with_capacity(entries.len());

    for entry in entries {
        entry.validate()?;
        if !seen_ids.insert(entry.id.as_str()) {
            return Err(CustomEntryError::Conflict {
                reason: format!("条目 ID '{}' 重复", entry.id),
            });
        }
        if !seen_titles.insert(entry.title.as_str()) {
            return Err(CustomEntryError::Conflict {
                reason: format!(
                    "条目标题 '{}' 重复，将导致默认项按标题解析不确定",
                    entry.title
                ),
            });
        }
    }
    Ok(())
}

/// 校验生成脚本的结构完整性（头部、花括号配对、menuentry 数量）
///
/// # Errors
/// 当头部缺失、花括号不配对或启用条目缺少 menuentry 时返回 [`CustomEntryError::InvalidScriptStructure`]。
pub fn validate_custom_script_structure(script: &str) -> Result<(), CustomEntryError> {
    if !script.starts_with("#!/bin/sh") {
        return Err(CustomEntryError::InvalidScriptStructure {
            reason: "缺少 #!/bin/sh 受管脚本头".to_string(),
        });
    }

    let mut depth = 0i32;
    for ch in script.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(CustomEntryError::InvalidScriptStructure {
                        reason: "花括号不配对，存在提前闭合的 menuentry".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(CustomEntryError::InvalidScriptStructure {
            reason: "花括号不配对，存在未闭合的 menuentry".to_string(),
        });
    }

    let menuentry_count = script
        .lines()
        .filter(|l| l.trim_start().starts_with("menuentry "))
        .count();
    let enabled_meta_count = script
        .lines()
        .filter(|l| l.contains("[helmsman-meta ") && l.contains("enabled=\"true\""))
        .count();
    if menuentry_count != enabled_meta_count {
        return Err(CustomEntryError::InvalidScriptStructure {
            reason: format!(
                "启用条目数 ({}) 与 menuentry 数 ({}) 不一致",
                enabled_meta_count, menuentry_count
            ),
        });
    }

    Ok(())
}
