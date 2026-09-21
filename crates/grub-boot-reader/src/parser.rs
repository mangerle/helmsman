use crate::model::{BootEntry, MenuNode};

/// 解析完整 /boot/grub/grub.cfg 文本并返回顶层菜单节点树
///
/// # 设计原理
/// - **实现初衷**：GRUB 配置文件采用类 Shell 语法且包含花括号作用域嵌套。
///   为了在不引入庞大 AST 解析器的前提下轻量提取条目与子菜单，采用基于栈的深度追踪状态机。
/// - **核心优势**：单遍扫描（Single-pass），内存开销低，即使几千行的复杂引导文件也能在微秒级解析完成。
/// - **代价与局限**：对语法错误的容忍度基于花括号平衡，若原文件存在未闭合的花括号可能影响层级判定。
pub fn parse_grub_cfg(content: &str) -> Vec<MenuNode> {
    let mut root_nodes: Vec<MenuNode> = Vec::new();
    let mut submenu_stack: Vec<SubmenuContext> = Vec::new();

    let mut current_depth = 0usize;
    let mut in_entry_depth: Option<usize> = None;
    let mut current_entry: Option<BootEntry> = None;

    for line in content.lines() {
        let clean_line = strip_comments(line);
        let trimmed = clean_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        handle_header_or_body(
            trimmed,
            current_depth,
            &mut in_entry_depth,
            &mut current_entry,
            &mut submenu_stack,
        );

        let open_braces = clean_line.chars().filter(|&c| c == '{').count();
        let close_braces = clean_line.chars().filter(|&c| c == '}').count();
        current_depth = current_depth
            .saturating_add(open_braces)
            .saturating_sub(close_braces);

        close_finished_scopes(
            current_depth,
            &mut in_entry_depth,
            &mut current_entry,
            &mut root_nodes,
            &mut submenu_stack,
        );
    }

    root_nodes
}

/// 处理行内容：识别 menuentry/submenu 声明头或在条目体内提取属性
fn handle_header_or_body(
    trimmed: &str,
    current_depth: usize,
    in_entry_depth: &mut Option<usize>,
    current_entry: &mut Option<BootEntry>,
    submenu_stack: &mut Vec<SubmenuContext>,
) {
    if in_entry_depth.is_none() {
        if trimmed.starts_with("menuentry ") {
            let (title, id, classes) = parse_entry_header(trimmed);
            let full_path = build_full_path(submenu_stack, &title);
            *current_entry = Some(BootEntry {
                title,
                id,
                full_path,
                classes,
                kernel_path: None,
                initrd_path: None,
                root_uuid: None,
                cmdline_params: None,
                chainloader_path: None,
            });
            *in_entry_depth = Some(current_depth);
        } else if trimmed.starts_with("submenu ") {
            let (title, id) = parse_submenu_header(trimmed);
            submenu_stack.push(SubmenuContext {
                title,
                id,
                start_depth: current_depth,
                children: Vec::new(),
            });
        }
    } else if let Some(entry) = current_entry {
        inspect_entry_line(entry, trimmed);
    }
}

/// 检查并闭合已结束的条目与子菜单作用域
fn close_finished_scopes(
    current_depth: usize,
    in_entry_depth: &mut Option<usize>,
    current_entry: &mut Option<BootEntry>,
    root_nodes: &mut Vec<MenuNode>,
    submenu_stack: &mut Vec<SubmenuContext>,
) {
    if let Some(entry_depth) = *in_entry_depth
        && current_depth <= entry_depth
    {
        if let Some(entry) = current_entry.take() {
            add_node_to_current_scope(root_nodes, submenu_stack, MenuNode::Entry(entry));
        }
        *in_entry_depth = None;
    }

    while matches!(
        submenu_stack.last(),
        Some(top) if current_depth <= top.start_depth
    ) {
        let Some(closed) = submenu_stack.pop() else {
            break;
        };
        let full_path = build_submenu_full_path(submenu_stack, &closed.title);
        let submenu_node = MenuNode::Submenu {
            title: closed.title,
            id: closed.id,
            full_path,
            children: closed.children,
        };
        add_node_to_current_scope(root_nodes, submenu_stack, submenu_node);
    }
}

struct SubmenuContext {
    title: String,
    id: Option<String>,
    start_depth: usize,
    children: Vec<MenuNode>,
}

fn add_node_to_current_scope(
    root: &mut Vec<MenuNode>,
    stack: &mut [SubmenuContext],
    node: MenuNode,
) {
    if let Some(current_sub) = stack.last_mut() {
        current_sub.children.push(node);
    } else {
        root.push(node);
    }
}

fn build_full_path(stack: &[SubmenuContext], title: &str) -> String {
    let mut parts: Vec<String> = stack.iter().map(|s| s.title.clone()).collect();
    parts.push(title.to_string());
    parts.join(">")
}

fn build_submenu_full_path(stack: &[SubmenuContext], title: &str) -> String {
    let mut parts: Vec<String> = stack.iter().map(|s| s.title.clone()).collect();
    parts.push(title.to_string());
    parts.join(">")
}

/// 解析 menuentry 声明行：提取标题、id、class 列表
fn parse_entry_header(line: &str) -> (String, Option<String>, Vec<String>) {
    let title = extract_first_quoted(line).unwrap_or_else(|| "未知条目".to_string());
    let id = extract_id(line);
    let classes = extract_classes(line);
    (title, id, classes)
}

/// 解析 submenu 声明行：提取标题和 id
fn parse_submenu_header(line: &str) -> (String, Option<String>) {
    let title = extract_first_quoted(line).unwrap_or_else(|| "未知子菜单".to_string());
    let id = extract_id(line);
    (title, id)
}

/// 提取第一个被单引号或双引号包裹的字符串（即条目标题）
fn extract_first_quoted(line: &str) -> Option<String> {
    let mut quote_char = None;
    let mut start_idx = None;

    for (i, c) in line.char_indices() {
        if let Some(q) = quote_char
            && c == q
            && let Some(start) = start_idx
        {
            return Some(line[start..i].to_string());
        } else if c == '\'' || c == '"' {
            quote_char = Some(c);
            start_idx = Some(i + 1);
        }
    }
    None
}

/// 提取 --class 参数列表
fn extract_classes(line: &str) -> Vec<String> {
    let mut classes = Vec::new();
    let tokens = line.split_whitespace();
    let mut next_is_class = false;

    for token in tokens {
        if next_is_class {
            classes.push(token.trim_matches('\'').trim_matches('"').to_string());
            next_is_class = false;
        } else if token == "--class" {
            next_is_class = true;
        }
    }

    classes
}

/// 提取 ID
fn extract_id(line: &str) -> Option<String> {
    // 典型格式: $menuentry_id_option 'gnulinux-simple-...' 或 --id '...'
    if let Some(pos) = line.find("$menuentry_id_option") {
        let remainder = &line[pos + "$menuentry_id_option".len()..];
        return extract_first_quoted(remainder);
    }
    if let Some(pos) = line.find("--id") {
        let remainder = &line[pos + "--id".len()..];
        return extract_first_quoted(remainder);
    }
    None
}

/// 去除行内的注释部分
fn strip_comments(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;

    for (i, c) in line.char_indices() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => {
                return &line[..i];
            }
            _ => {}
        }
    }
    line
}

/// 逐行反解析条目体，提取内核、initrd、根分区 UUID、启动参数及链式加载器
fn inspect_entry_line(entry: &mut BootEntry, line: &str) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return;
    }

    let cmd = tokens[0];
    if cmd == "linux" || cmd == "linuxefi" || cmd == "linux16" {
        if tokens.len() > 1 {
            entry.kernel_path = Some(tokens[1].to_string());
        }
        if tokens.len() > 2 {
            let params = tokens[2..].join(" ");
            // 尝试从启动参数中提取 root=UUID=...
            for token in &tokens[2..] {
                if let Some(uuid) = token.strip_prefix("root=UUID=") {
                    entry.root_uuid = Some(uuid.to_string());
                } else if let Some(dev) = token.strip_prefix("root=")
                    && entry.root_uuid.is_none()
                {
                    entry.root_uuid = Some(dev.to_string());
                }
            }
            entry.cmdline_params = Some(params);
        }
    } else if cmd == "initrd" || cmd == "initrdefi" || cmd == "initrd16" {
        if tokens.len() > 1 {
            entry.initrd_path = Some(tokens[1].to_string());
        }
    } else if cmd == "chainloader" {
        if tokens.len() > 1 {
            entry.chainloader_path = Some(tokens[1].to_string());
        }
    } else if cmd == "search"
        && line.contains("--fs-uuid")
        && let Some(last) = tokens.last()
        && *last != "--fs-uuid"
        && *last != "--set=root"
        && entry.root_uuid.is_none()
    {
        entry.root_uuid = Some(last.to_string());
    }
}
