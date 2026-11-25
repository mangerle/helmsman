/// 差异对比报告
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    /// 是否存在内容差异
    pub has_changes: bool,
    /// 格式化后的 Unified Diff 文本
    pub diff_text: String,
    /// 新增行数
    pub added_lines: usize,
    /// 删除行数
    pub removed_lines: usize,
}

/// 基于最长公共子序列 (LCS) 计算两段文本的差异
///
/// # 设计原理
/// - **实现初衷**：为前端“保存前差异审核弹窗”提供标准的 Unified Diff 视图。
///   配置文件通常在百行以内，经典的动态规划 LCS 算法具备最高的可靠性与精准度。
/// - **核心优势**：纯标准库实现，零外部 crate 依赖；采用一维连续数组存储 DP 表，消除二维嵌套分配与未变更行的冗余字符串构造。
/// - **代价与局限**：时间与空间复杂度为 O(N*M)，适合配置文件对比；针对超大文本（数万行）需采用 Myers 算法。
pub fn generate_unified_diff(original: &str, modified: &str) -> DiffReport {
    if original == modified {
        return DiffReport {
            has_changes: false,
            diff_text: String::new(),
            added_lines: 0,
            removed_lines: 0,
        };
    }

    let orig_lines: Vec<&str> = original.lines().collect();
    let mod_lines: Vec<&str> = modified.lines().collect();
    let cols = mod_lines.len() + 1;

    let dp = build_lcs_table(&orig_lines, &mod_lines, cols);
    let (diff_lines, added_count, removed_count) =
        backtrack_lcs_changes(&orig_lines, &mod_lines, &dp, cols);

    let total_bytes: usize = diff_lines.iter().map(|s| s.len() + 1).sum();
    let mut output = String::with_capacity(total_bytes);
    for line in &diff_lines {
        output.push_str(line);
        output.push('\n');
    }

    DiffReport {
        has_changes: true,
        diff_text: output,
        added_lines: added_count,
        removed_lines: removed_count,
    }
}

/// 动态规划构建一维连续内存 LCS 匹配长度表
fn build_lcs_table(orig: &[&str], modif: &[&str], cols: usize) -> Vec<usize> {
    let n = orig.len();
    let mut dp = vec![0usize; (n + 1) * cols];

    for (i, orig_line) in orig.iter().enumerate() {
        let row = i * cols;
        let next_row = (i + 1) * cols;
        for (j, mod_line) in modif.iter().enumerate() {
            dp[next_row + j + 1] = if orig_line == mod_line {
                dp[row + j] + 1
            } else {
                dp[next_row + j].max(dp[row + j + 1])
            };
        }
    }

    dp
}

/// 基于一维 LCS 表回溯生成行级增删标记（跳过未变更行，避免无效字符串分配）
fn backtrack_lcs_changes(
    orig: &[&str],
    modif: &[&str],
    dp: &[usize],
    cols: usize,
) -> (Vec<String>, usize, usize) {
    let mut diff_lines = Vec::new();
    let mut i = orig.len();
    let mut j = modif.len();
    let mut added_count = 0;
    let mut removed_count = 0;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && orig[i - 1] == modif[j - 1] {
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i * cols + j - 1] >= dp[(i - 1) * cols + j]) {
            diff_lines.push(format!("+{}", modif[j - 1]));
            added_count += 1;
            j -= 1;
        } else if i > 0 {
            diff_lines.push(format!("-{}", orig[i - 1]));
            removed_count += 1;
            i -= 1;
        }
    }

    diff_lines.reverse();
    (diff_lines, added_count, removed_count)
}
