use grub_boot_reader::{BootEntry, MenuNode};

/// 供前端界面渲染的扁平启动项条目模型
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootEntryItem {
    /// 条目完整唯一定位路径（如 "gnulinux-advanced-...>gnulinux-6.8.0-..." 或直接标题）
    pub full_path: String,
    /// 界面显示的人类可读标题
    pub title: String,
    /// 是否为当前选中的默认启动项
    pub is_default: bool,
    /// 是否为急救或恢复模式
    pub is_recovery: bool,
    /// 探测到的内核版本号（若可提取）
    pub kernel_version: Option<String>,
}

impl BootEntryItem {
    /// 从底层 BootEntry 转换为前端条目视图模型
    pub fn from_entry(entry: &BootEntry, current_default: Option<&str>) -> Self {
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

        // 从标题或参数中粗略提取内核版本
        let kernel_version = entry
            .title
            .split_whitespace()
            .find(|w| w.starts_with("6.") || w.starts_with("5.") || w.starts_with("4."))
            .map(|s| s.to_string());

        Self {
            full_path: entry.full_path.clone(),
            title: entry.title.clone(),
            is_default,
            is_recovery,
            kernel_version,
        }
    }
}

/// 将多层级的 MenuNode 树扁平化转换为便于单选列表展示的条目列表
///
/// # 设计原理
/// - **实现初衷**：在基础交互中，普通用户更习惯以单层列表单选默认项，避免深层折叠带来的寻找困难。
/// - **核心优势**：自动穿透 Submenu 递归提取所有实际可启动的内核条目，并标记当前默认状态。
pub fn flatten_boot_entries(
    nodes: &[MenuNode],
    current_default: Option<&str>,
) -> Vec<BootEntryItem> {
    let mut raw_entries = Vec::with_capacity(16);
    for node in nodes {
        raw_entries.extend(node.collect_entries());
    }

    let default_idx = current_default.and_then(|d| d.parse::<usize>().ok());
    let mut entries = Vec::with_capacity(raw_entries.len());

    for (idx, raw) in raw_entries.into_iter().enumerate() {
        let mut item = BootEntryItem::from_entry(raw, current_default);
        if let Some(target_idx) = default_idx
            && target_idx == idx
        {
            item.is_default = true;
        }
        entries.push(item);
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_entry_item_conversion() {
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

        let item =
            BootEntryItem::from_entry(&entry, Some("gnulinux-advanced>gnulinux-6.8.0-recovery"));
        assert!(item.is_default);
        assert!(item.is_recovery);
        assert_eq!(item.kernel_version, Some("6.8.0-40-generic".to_string()));
    }

    #[test]
    fn test_flatten_boot_entries() {
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
            title: "Windows Boot Manager".to_string(),
            id: Some("entry2".to_string()),
            full_path: "entry2".to_string(),
            classes: vec!["windows".to_string()],
            kernel_path: None,
            initrd_path: None,
            root_uuid: None,
            cmdline_params: None,
            chainloader_path: None,
        };

        let nodes = vec![
            MenuNode::Entry(entry1),
            MenuNode::Submenu {
                title: "高级选项".to_string(),
                id: Some("sub".to_string()),
                full_path: "sub".to_string(),
                children: vec![MenuNode::Entry(entry2)],
            },
        ];

        let flattened = flatten_boot_entries(&nodes, Some("entry2"));
        assert_eq!(flattened.len(), 2);
        assert_eq!(flattened[0].title, "Ubuntu 24.04");
        assert!(!flattened[0].is_default);
        assert_eq!(flattened[1].title, "Windows Boot Manager");
        assert!(flattened[1].is_default);
    }
}
