use grub_boot_reader::{BootEntry, CustomBootEntry, MenuNode};
use std::collections::HashMap;

/// 供前端界面渲染的扁平启动项条目模型
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootEntryItem {
    /// 唯一标识 ID（若有）
    pub id: Option<String>,
    /// 条目完整唯一定位路径（如 "gnulinux-advanced-...>gnulinux-6.8.0-..." 或直接标题）
    pub full_path: String,
    /// 原始人类可读标题
    pub title: String,
    /// 最终界面展示标题（若设置了别名则优先展示别名）
    pub display_title: String,
    /// 是否为当前选中的默认启动项
    pub is_default: bool,
    /// 是否为急救或恢复模式
    pub is_recovery: bool,
    /// 是否为受管的自定义引导条目（如 41_helmsman_custom）
    pub is_custom: bool,
    /// 探测到的内核版本号（若可提取）
    pub kernel_version: Option<String>,
}

impl BootEntryItem {
    /// 从底层 BootEntry 转换为前端条目视图模型（带可选别名注入）
    pub fn from_entry_with_alias(
        entry: &BootEntry,
        current_default: Option<&str>,
        aliases: &HashMap<String, String>,
    ) -> Self {
        let is_default = match current_default {
            Some("saved") => false,
            Some(target) => {
                entry.full_path == target
                    || entry.title == target
                    || entry.id.as_deref() == Some(target)
            }
            None => false,
        };

        let title_lower = entry.title.to_ascii_lowercase();
        let is_recovery = title_lower.contains("recovery")
            || title_lower.contains("rescue")
            || title_lower.contains("恢复");

        let kernel_version = entry
            .title
            .split_whitespace()
            .find(|w| w.starts_with("6.") || w.starts_with("5.") || w.starts_with("4."))
            .map(|s| s.to_string());

        let display_title = if let Some(ref id) = entry.id
            && let Some(alias) = aliases.get(id)
        {
            alias.clone()
        } else if let Some(alias) = aliases.get(&entry.title) {
            alias.clone()
        } else {
            entry.title.clone()
        };

        Self {
            id: entry.id.clone(),
            full_path: entry.full_path.clone(),
            title: entry.title.clone(),
            display_title,
            is_default,
            is_recovery,
            is_custom: false,
            kernel_version,
        }
    }

    /// 从底层 BootEntry 转换为普通条目模型
    pub fn from_entry(entry: &BootEntry, current_default: Option<&str>) -> Self {
        let empty_aliases = HashMap::new();
        Self::from_entry_with_alias(entry, current_default, &empty_aliases)
    }

    /// 从受管自定义条目转换为前端视图模型
    pub fn from_custom_entry(custom: &CustomBootEntry, current_default: Option<&str>) -> Self {
        let is_default = match current_default {
            Some(target) => custom.id == target || custom.title == target,
            None => false,
        };

        Self {
            id: Some(custom.id.clone()),
            full_path: custom.title.clone(),
            title: custom.title.clone(),
            display_title: custom.title.clone(),
            is_default,
            is_recovery: false,
            is_custom: true,
            kernel_version: None,
        }
    }
}

/// 扁平化转换引导项树并附加别名映射与恢复模式过滤
pub fn flatten_boot_entries_with_options(
    nodes: &[MenuNode],
    current_default: Option<&str>,
    aliases: &HashMap<String, String>,
    filter_recovery: bool,
) -> Vec<BootEntryItem> {
    let mut raw_entries = Vec::with_capacity(16);
    for node in nodes {
        raw_entries.extend(node.collect_entries());
    }

    let default_idx = current_default.and_then(|d| d.parse::<usize>().ok());
    let mut entries = Vec::with_capacity(raw_entries.len());

    for (idx, raw) in raw_entries.into_iter().enumerate() {
        let mut item = BootEntryItem::from_entry_with_alias(raw, current_default, aliases);
        if let Some(target_idx) = default_idx
            && target_idx == idx
        {
            item.is_default = true;
        }

        if filter_recovery && item.is_recovery {
            continue;
        }

        entries.push(item);
    }

    entries
}

/// 基础扁平化转换（保持向后兼容）
pub fn flatten_boot_entries(
    nodes: &[MenuNode],
    current_default: Option<&str>,
) -> Vec<BootEntryItem> {
    let empty_aliases = HashMap::new();
    flatten_boot_entries_with_options(nodes, current_default, &empty_aliases, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_entry_item_alias_and_recovery() {
        let entry = BootEntry {
            title: "Ubuntu, with Linux 6.8.0-40-generic (recovery mode)".to_string(),
            id: Some("gnulinux-6.8.0-recovery".to_string()),
            full_path: "gnulinux-advanced>gnulinux-6.8.0-recovery".to_string(),
            classes: vec!["ubuntu".to_string(), "gnu-linux".to_string()],
            kernel_path: Some("/boot/vmlinuz-6.8.0-40-generic".to_string()),
            initrd_path: Some("/boot/initrd.img-6.8.0-40-generic".to_string()),
            root_uuid: None,
            cmdline_params: Some("root=UUID=123 ro single".to_string()),
            chainloader_path: None,
        };

        let mut aliases = HashMap::new();
        aliases.insert(
            "gnulinux-6.8.0-recovery".to_string(),
            "Ubuntu 6.8 (急救模式)".to_string(),
        );

        let item = BootEntryItem::from_entry_with_alias(&entry, None, &aliases);
        assert_eq!(
            item.title,
            "Ubuntu, with Linux 6.8.0-40-generic (recovery mode)"
        );
        assert_eq!(item.display_title, "Ubuntu 6.8 (急救模式)");
        assert!(item.is_recovery);
    }

    #[test]
    fn test_flatten_with_filter_recovery() {
        let entry1 = BootEntry {
            title: "Ubuntu 24.04".to_string(),
            id: Some("entry1".to_string()),
            full_path: "entry1".to_string(),
            classes: vec!["ubuntu".to_string()],
            kernel_path: None,
            initrd_path: None,
            root_uuid: None,
            cmdline_params: None,
            chainloader_path: None,
        };

        let entry2 = BootEntry {
            title: "Ubuntu (recovery mode)".to_string(),
            id: Some("entry2".to_string()),
            full_path: "entry2".to_string(),
            classes: vec!["ubuntu".to_string()],
            kernel_path: None,
            initrd_path: None,
            root_uuid: None,
            cmdline_params: None,
            chainloader_path: None,
        };

        let nodes = vec![MenuNode::Entry(entry1), MenuNode::Entry(entry2)];
        let empty_aliases = HashMap::new();

        // 不过滤时 2 项
        let all = flatten_boot_entries_with_options(&nodes, None, &empty_aliases, false);
        assert_eq!(all.len(), 2);

        // 过滤恢复模式时只剩 1 项
        let filtered = flatten_boot_entries_with_options(&nodes, None, &empty_aliases, true);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Ubuntu 24.04");
    }
}
