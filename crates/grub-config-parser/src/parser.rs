use crate::ast::{ConfigLine, GrubConfigFile, QuoteType};
use std::error::Error;
use std::fmt;

/// 解析过程中的错误类型
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// 语法未识别
    InvalidLine(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidLine(line) => write!(f, "无法解析的配置行: {}", line),
        }
    }
}

impl Error for ParseError {}

/// 解析单行文本为 ConfigLine
pub fn parse_line(line: &str) -> ConfigLine {
    // 检查是否全为空白
    if line.trim().is_empty() {
        return ConfigLine::Empty;
    }

    // 检查是否为纯注释行
    let trimmed_start = line.trim_start();
    if trimmed_start.starts_with('#') {
        return ConfigLine::Comment(line.to_string());
    }

    // 记录前置空白
    let prefix_len = line.len() - trimmed_start.len();
    let prefix_whitespace = line[..prefix_len].to_string();

    // 查找等号位置
    if let Some(eq_pos) = trimmed_start.find('=') {
        let raw_key = &trimmed_start[..eq_pos].trim();
        // 允许可选的 export 前缀
        let key = if let Some(stripped) = raw_key.strip_prefix("export ") {
            stripped.trim()
        } else {
            *raw_key
        };

        // 验证键名合法性（必须符合 Shell 变量标识符规范：字母/下划线开头，后续为字母/数字/下划线）
        if !is_valid_identifier(key) {
            return ConfigLine::Raw(line.to_string());
        }

        let raw_val = trimmed_start[eq_pos + 1..].trim_start();
        let (value, quote_type, trailing_comment) = parse_value_and_comment(raw_val);

        ConfigLine::Assignment {
            key: key.to_string(),
            value,
            quote_type,
            prefix_whitespace,
            trailing_comment,
        }
    } else {
        ConfigLine::Raw(line.to_string())
    }
}

/// 解析赋值右侧的值、引号类型及可能的行尾注释
fn parse_value_and_comment(raw_val: &str) -> (String, QuoteType, Option<String>) {
    if raw_val.is_empty() {
        return (String::new(), QuoteType::None, None);
    }

    // 双引号包裹
    if let Some(stripped) = raw_val.strip_prefix('"')
        && let Some(end_quote) = find_closing_quote(stripped, '"')
    {
        let value = stripped[..end_quote].to_string();
        let remainder = stripped[end_quote + 1..].trim();
        let trailing_comment = extract_trailing_comment(remainder);
        return (value, QuoteType::Double, trailing_comment);
    }

    // 单引号包裹
    if let Some(stripped) = raw_val.strip_prefix('\'')
        && let Some(end_quote) = find_closing_quote(stripped, '\'')
    {
        let value = stripped[..end_quote].to_string();
        let remainder = stripped[end_quote + 1..].trim();
        let trailing_comment = extract_trailing_comment(remainder);
        return (value, QuoteType::Single, trailing_comment);
    }

    // 无引号情况（处理行尾注释）
    if let Some(hash_pos) = raw_val.find('#') {
        let val_part = raw_val[..hash_pos].trim_end();
        let comment_part = raw_val[hash_pos..].to_string();
        (val_part.to_string(), QuoteType::None, Some(comment_part))
    } else {
        (raw_val.trim_end().to_string(), QuoteType::None, None)
    }
}

/// 寻找闭合引号（忽略转义引号）
fn find_closing_quote(s: &str, quote_char: char) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == quote_char {
            return Some(i);
        }
    }
    None
}

/// 提取剩余部分中的行尾注释
fn extract_trailing_comment(remainder: &str) -> Option<String> {
    if remainder.starts_with('#') {
        Some(remainder.to_string())
    } else {
        None
    }
}

/// 检查是否为合法的 Shell 变量名
fn is_valid_identifier(ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    let mut chars = ident.chars();
    if let Some(first) = chars.next()
        && !first.is_ascii_alphabetic()
        && first != '_'
    {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 解析完整配置文件文本
pub fn parse_grub_config(content: &str) -> GrubConfigFile {
    let lines = content.lines().map(parse_line).collect();
    GrubConfigFile { lines }
}
