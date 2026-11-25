use super::model::{CustomBootEntry, entry_types};

/// 解析 `/etc/grub.d/41_helmsman_custom` 脚本为结构化自定义引导项列表
pub fn parse_custom_script(content: &str) -> Vec<CustomBootEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut current_submenu: Option<String> = None;

    while i < lines.len() {
        let line = lines[i].trim();

        if let Some(rest) = line.strip_prefix("submenu ") {
            let title = extract_title_from_menuentry(rest)
                .or_else(|| extract_title_from_menuentry(line))
                .unwrap_or_default();
            current_submenu = if title.is_empty() { None } else { Some(title) };
            i += 1;
            continue;
        }
        if line == "}" && current_submenu.is_some() {
            current_submenu = None;
            i += 1;
            continue;
        }

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
            let meta_submenu = extract_meta_attr(meta_str, "submenu");
            let submenu_title = meta_submenu
                .or_else(|| current_submenu.clone())
                .unwrap_or_default();

            i += 1;
            let mut block_lines = Vec::new();
            while i < lines.len() {
                let cur_trimmed = lines[i].trim();
                let uncommented = cur_trimmed.strip_prefix("# ").unwrap_or(cur_trimmed);

                block_lines.push(uncommented);
                if uncommented == "}"
                    || (uncommented.starts_with('}') && uncommented.ends_with('}'))
                {
                    break;
                }
                i += 1;
            }

            if let Some(mut entry) = parse_single_block(&id, &entry_type, enabled, &block_lines) {
                entry.submenu_title = submenu_title;
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
        submenu_title: String::new(),
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
