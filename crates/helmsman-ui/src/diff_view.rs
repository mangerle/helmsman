/// 差异行类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineType {
    /// 新增行 (+)
    Added,
    /// 删除行 (-)
    Removed,
    /// 上下文未变更行 ( )
    Context,
    /// 差异块头信息 (@@ ... @@)
    HunkHeader,
}

/// 结构化差异行数据
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// 行类型
    pub line_type: DiffLineType,
    /// 行文本内容
    pub content: String,
    /// 原始文件行号 (若存在)
    pub old_lineno: Option<usize>,
    /// 新文件行号 (若存在)
    pub new_lineno: Option<usize>,
}

/// 差异可视化视图模型
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffViewModel {
    /// 是否存在实际变更
    pub has_changes: bool,
    /// 新增行总数
    pub added_count: usize,
    /// 删除行总数
    pub removed_count: usize,
    /// 结构化行列表
    pub lines: Vec<DiffLine>,
}

impl DiffViewModel {
    /// 从 Unified Diff 文本解析构建结构化差异视图模型
    ///
    /// # 设计原理
    /// - **实现初衷**：原始 diff 文本适合终端阅读，但在 GUI 中需要精准的行号对齐与色彩标记。
    /// - **核心优势**：自动追踪旧行号与新行号的递增，解析出结构化的行列表，方便前端渲染。
    pub fn from_unified_diff(diff_text: &str) -> Self {
        let mut lines = Vec::new();
        let mut added_count = 0;
        let mut removed_count = 0;

        let mut current_old_lineno = 1;
        let mut current_new_lineno = 1;

        for line in diff_text.lines() {
            if line.starts_with("---") || line.starts_with("+++") {
                // 忽略文件头
                continue;
            } else if line.starts_with("@@") {
                // 简化的块头解析
                lines.push(DiffLine {
                    line_type: DiffLineType::HunkHeader,
                    content: line.to_string(),
                    old_lineno: None,
                    new_lineno: None,
                });
            } else if let Some(stripped) = line.strip_prefix('+') {
                added_count += 1;
                lines.push(DiffLine {
                    line_type: DiffLineType::Added,
                    content: stripped.to_string(),
                    old_lineno: None,
                    new_lineno: Some(current_new_lineno),
                });
                current_new_lineno += 1;
            } else if let Some(stripped) = line.strip_prefix('-') {
                removed_count += 1;
                lines.push(DiffLine {
                    line_type: DiffLineType::Removed,
                    content: stripped.to_string(),
                    old_lineno: Some(current_old_lineno),
                    new_lineno: None,
                });
                current_old_lineno += 1;
            } else {
                let stripped = line.strip_prefix(' ').unwrap_or(line);
                lines.push(DiffLine {
                    line_type: DiffLineType::Context,
                    content: stripped.to_string(),
                    old_lineno: Some(current_old_lineno),
                    new_lineno: Some(current_new_lineno),
                });
                current_old_lineno += 1;
                current_new_lineno += 1;
            }
        }

        Self {
            has_changes: added_count > 0 || removed_count > 0,
            added_count,
            removed_count,
            lines,
        }
    }

    /// 获取行数统计中文描述（例如："+3 / -1"）
    pub fn summary_text(&self) -> String {
        format!("+{} / -{}", self.added_count, self.removed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_diff() {
        let model = DiffViewModel::from_unified_diff("");
        assert!(!model.has_changes);
        assert_eq!(model.added_count, 0);
        assert_eq!(model.removed_count, 0);
        assert!(model.lines.is_empty());
        assert_eq!(model.summary_text(), "+0 / -0");
    }

    #[test]
    fn test_parse_unified_diff_content() {
        let diff_sample = "--- original\n+++ new\n@@ -1,3 +1,3 @@\n GRUB_DEFAULT=0\n-GRUB_TIMEOUT=5\n+GRUB_TIMEOUT=10\n";
        let model = DiffViewModel::from_unified_diff(diff_sample);

        assert!(model.has_changes);
        assert_eq!(model.added_count, 1);
        assert_eq!(model.removed_count, 1);
        assert_eq!(model.summary_text(), "+1 / -1");

        // 验证行类型
        assert_eq!(model.lines[0].line_type, DiffLineType::HunkHeader);
        assert_eq!(model.lines[1].line_type, DiffLineType::Context);
        assert_eq!(model.lines[1].content, "GRUB_DEFAULT=0");
        assert_eq!(model.lines[2].line_type, DiffLineType::Removed);
        assert_eq!(model.lines[2].content, "GRUB_TIMEOUT=5");
        assert_eq!(model.lines[3].line_type, DiffLineType::Added);
        assert_eq!(model.lines[3].content, "GRUB_TIMEOUT=10");
    }
}
