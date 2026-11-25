use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 全量引导菜单布局编排规则
///
/// # 设计原理
/// - **实现初衷**：避免直接篡改 `/etc/grub.d/10_linux` 等发行版系统脚本，通过独立的声明式布局规则
///   控制所有引导条目（含系统内核、Windows 与自定义 ISO）的显示顺序、子菜单归组与隐藏状态。
/// - **核心优势**：非破坏性，系统内核升级后重新执行 `update-grub` 时仍可自动重放用户编排的布局，
///   且随时可清空或通过快照回滚还原。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuLayout {
    /// 条目全局显示顺序（存储条目的稳定路径 `full_path`）
    #[serde(default)]
    pub order: Vec<String>,
    /// 被用户隐藏（移除）的条目 `full_path` 列表
    #[serde(default)]
    pub hidden: Vec<String>,
    /// 子菜单归属覆写（`full_path` -> 目标子菜单标题；空字符串 `""` 表示强制移回根层级）
    #[serde(default)]
    pub submenu_overrides: HashMap<String, String>,
}

impl MenuLayout {
    /// 判断当前布局是否为空（没有任何编排覆写）
    pub fn is_empty(&self) -> bool {
        self.order.is_empty() && self.hidden.is_empty() && self.submenu_overrides.is_empty()
    }
}

/// 从 `grub.cfg` 中提取出的原始条目块
#[derive(Debug, Clone)]
struct RawEntryBlock {
    full_path: String,
    submenu_title: Option<String>,
    raw_block: String,
}

/// 将 [`MenuLayout`] 非破坏性地应用到 `grub.cfg` 文本，生成重排/归组/隐藏后的菜单内容
pub fn apply_menu_layout_to_grub_cfg(grub_cfg: &str, layout: &MenuLayout) -> String {
    if layout.is_empty() {
        return grub_cfg.to_string();
    }

    let mut prefix = String::with_capacity(grub_cfg.len() / 2);
    let mut suffix = String::new();
    let mut blocks = Vec::new();
    let mut seen_menu = false;

    let lines: Vec<&str> = grub_cfg.lines().collect();
    let mut idx = 0;

    while idx < lines.len() {
        let trimmed = lines[idx].trim_start();
        if is_menu_start(trimmed) {
            seen_menu = true;
            let end_idx = find_block_end(&lines, idx);
            let block_text = lines[idx..=end_idx].join("\n");
            if trimmed.starts_with("submenu ") {
                extract_submenu_entries(&lines[idx..=end_idx], &mut blocks);
            } else if let Some(title) = extract_quoted_title(trimmed) {
                blocks.push(RawEntryBlock {
                    full_path: title,
                    submenu_title: None,
                    raw_block: block_text,
                });
            }
            idx = end_idx + 1;
        } else {
            if !seen_menu {
                prefix.push_str(lines[idx]);
                prefix.push('\n');
            } else {
                suffix.push_str(lines[idx]);
                suffix.push('\n');
            }
            idx += 1;
        }
    }

    if blocks.is_empty() {
        return grub_cfg.to_string();
    }

    // 1. 过滤隐藏条目并应用子菜单归属覆写
    let mut visible: Vec<RawEntryBlock> = blocks
        .into_iter()
        .filter(|b| !layout.hidden.contains(&b.full_path))
        .map(|mut b| {
            if let Some(target_sub) = layout.submenu_overrides.get(&b.full_path) {
                b.submenu_title = if target_sub.trim().is_empty() {
                    None
                } else {
                    Some(target_sub.trim().to_string())
                };
            }
            b
        })
        .collect();

    // 2. 按 layout.order 进行稳定排序（未在 order 中的条目保持相对顺序附于末尾）
    if !layout.order.is_empty() {
        visible.sort_by_key(|b| {
            layout
                .order
                .iter()
                .position(|k| k == &b.full_path)
                .unwrap_or(usize::MAX)
        });
    }

    // 3. 重新组装根级条目与子菜单分组
    let rendered_menu = render_arranged_blocks(&visible);
    let mut out = String::with_capacity(grub_cfg.len());
    out.push_str(&prefix);
    out.push_str(&rendered_menu);
    if !suffix.is_empty() {
        out.push_str(&suffix);
    }
    out
}

fn is_menu_start(trimmed: &str) -> bool {
    (trimmed.starts_with("menuentry ") || trimmed.starts_with("submenu ")) && trimmed.contains('{')
}

fn find_block_end(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    for (offset, line) in lines[start..].iter().enumerate() {
        depth += brace_delta(line);
        if depth <= 0 {
            return start + offset;
        }
    }
    lines.len().saturating_sub(1)
}

fn brace_delta(line: &str) -> i32 {
    let mut delta = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escape = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '{' if !in_single && !in_double => delta += 1,
            '}' if !in_single && !in_double => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn extract_quoted_title(line: &str) -> Option<String> {
    let quote = if line.contains('\'') { '\'' } else { '"' };
    let first = line.find(quote)?;
    let rest = &line[first + 1..];
    let second = rest.find(quote)?;
    Some(rest[..second].to_string())
}

fn extract_submenu_entries(sub_lines: &[&str], out: &mut Vec<RawEntryBlock>) {
    let sub_title = sub_lines
        .first()
        .and_then(|l| extract_quoted_title(l.trim_start()))
        .unwrap_or_else(|| "子菜单".to_string());

    let mut idx = 1;
    let last = sub_lines.len().saturating_sub(1);
    while idx < last {
        let trimmed = sub_lines[idx].trim_start();
        if trimmed.starts_with("menuentry ") && trimmed.contains('{') {
            let end_idx = find_block_end(sub_lines, idx);
            let raw_block = sub_lines[idx..=end_idx]
                .iter()
                .map(|l| {
                    l.strip_prefix('\t')
                        .or_else(|| l.strip_prefix("    "))
                        .unwrap_or(l)
                })
                .collect::<Vec<_>>()
                .join("\n");
            if let Some(entry_title) = extract_quoted_title(trimmed) {
                out.push(RawEntryBlock {
                    full_path: format!("{sub_title}>{entry_title}"),
                    submenu_title: Some(sub_title.clone()),
                    raw_block,
                });
            }
            idx = end_idx + 1;
        } else {
            idx += 1;
        }
    }
}

fn render_arranged_blocks(blocks: &[RawEntryBlock]) -> String {
    let mut out = String::new();
    let mut emitted_submenus: Vec<String> = Vec::new();

    for block in blocks {
        match &block.submenu_title {
            None => {
                out.push_str(&block.raw_block);
                out.push('\n');
            }
            Some(sub_title) => {
                if emitted_submenus.contains(sub_title) {
                    continue;
                }
                emitted_submenus.push(sub_title.clone());
                let safe_title = sub_title.replace('\'', "");
                out.push_str(&format!("submenu '{safe_title}' {{\n"));
                for member in blocks
                    .iter()
                    .filter(|b| b.submenu_title.as_deref() == Some(sub_title.as_str()))
                {
                    for line in member.raw_block.lines() {
                        out.push('\t');
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                out.push_str("}\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CFG: &str = r#"# header
set default="0"
menuentry 'Ubuntu 24.04 LTS' {
    linux /vmlinuz
}
submenu 'Advanced options for Ubuntu' {
    menuentry 'Ubuntu, with Linux 6.8.0' {
        linux /vmlinuz-6.8.0
    }
}
menuentry 'Windows Boot Manager' {
    chainloader /efi/bootmgfw.efi
}
# footer
"#;

    #[test]
    fn test_layout_reorder_hide_and_submenu_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "Advanced options for Ubuntu>Ubuntu, with Linux 6.8.0".to_string(),
            String::new(),
        );
        let layout = MenuLayout {
            order: vec![
                "Windows Boot Manager".to_string(),
                "Advanced options for Ubuntu>Ubuntu, with Linux 6.8.0".to_string(),
            ],
            hidden: vec!["Ubuntu 24.04 LTS".to_string()],
            submenu_overrides: overrides,
        };

        let result = apply_menu_layout_to_grub_cfg(SAMPLE_CFG, &layout);
        assert!(!result.contains("Ubuntu 24.04 LTS"));
        assert!(!result.contains("submenu 'Advanced options for Ubuntu'"));
        let win_pos = result.find("Windows Boot Manager").unwrap_or(usize::MAX);
        let adv_pos = result
            .find("Ubuntu, with Linux 6.8.0")
            .unwrap_or(usize::MAX);
        assert!(win_pos < adv_pos);
        assert!(result.contains("# footer"));
    }
}
