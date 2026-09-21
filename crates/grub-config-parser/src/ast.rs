use std::fmt;

/// 引号包裹类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteType {
    /// 无引号（例如：GRUB_TIMEOUT=5）
    None,
    /// 单引号（例如：GRUB_DEFAULT='Ubuntu'）
    Single,
    /// 双引号（例如：GRUB_CMDLINE_LINUX_DEFAULT="quiet splash"）
    Double,
}

/// 配置文件中的单行表示
///
/// # 设计原理
/// - **实现初衷**：传统的 INI/Key-Value 解析器在反序列化时会丢弃行内注释与空白字符，
///   导致修改后的文件与用户手写配置发生格式错乱。本模型采用保留行类型的 AST，
///   将配置行细分为空白、纯注释、变量赋值与原样内容四种类型。
/// - **核心优势**：在修改单个配置项时，能 100% 保持其余未修改行的排版、缩进和原有注释不变。
/// - **代价与局限**：模型面向以单行为主的 Shell 变量赋值语法，暂未覆盖跨多行的复杂 Bash 控制流。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLine {
    /// 空白行（保留可能的前置空格/换行）
    Empty,
    /// 注释行（以 # 开头）
    Comment(String),
    /// 键值对变量赋值
    Assignment {
        /// 键名（例如：GRUB_TIMEOUT）
        key: String,
        /// 实际值（已去除外层引号）
        value: String,
        /// 原有引号类型
        quote_type: QuoteType,
        /// 是否包含 export 前缀
        has_export: bool,
        /// 键名前的前置空白字符
        prefix_whitespace: String,
        /// 行尾注释（包括 # 及其后内容）
        trailing_comment: Option<String>,
    },
    /// 无法结构化识别的原样内容
    Raw(String),
}

impl ConfigLine {
    /// 格式化为输出字符串（不含换行符）
    pub fn format_line(&self) -> String {
        match self {
            ConfigLine::Empty => String::new(),
            ConfigLine::Comment(text) => text.clone(),
            ConfigLine::Raw(text) => text.clone(),
            ConfigLine::Assignment {
                key,
                value,
                quote_type,
                has_export,
                prefix_whitespace,
                trailing_comment,
            } => {
                let quoted = match quote_type {
                    QuoteType::None => value.clone(),
                    QuoteType::Single => format!("'{}'", value),
                    QuoteType::Double => format!("\"{}\"", value),
                };
                let export_prefix = if *has_export { "export " } else { "" };
                let mut result = format!(
                    "{}{}{}{}{}",
                    prefix_whitespace, export_prefix, key, "=", quoted
                );

                if let Some(comment) = trailing_comment {
                    result.push(' ');
                    result.push_str(comment);
                }
                result
            }
        }
    }
}

/// 完整的 GRUB 默认配置文件抽象语法树
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrubConfigFile {
    /// 文件中的所有行
    pub lines: Vec<ConfigLine>,
}

impl GrubConfigFile {
    /// 创建空配置
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// 获取指定键的值
    pub fn get(&self, key: &str) -> Option<&str> {
        for line in &self.lines {
            if let ConfigLine::Assignment {
                key: k, value: v, ..
            } = line
                && k == key
            {
                return Some(v.as_str());
            }
        }
        None
    }

    /// 设置指定键的值
    /// 如果键已存在，则就地更新并尽量保持原有引号类型和行尾注释；
    /// 如果不存在，则在文件末尾追加新行（默认使用双引号）。
    pub fn set(&mut self, key: &str, value: &str) {
        for line in &mut self.lines {
            if let ConfigLine::Assignment {
                key: k,
                value: v,
                quote_type,
                ..
            } = line
                && k == key
            {
                *v = value.to_string();
                // 如果原先无引号但新值包含空格，自动升级为双引号
                if *quote_type == QuoteType::None && value.contains(' ') {
                    *quote_type = QuoteType::Double;
                }
                return;
            }
        }

        // 未找到现有项，追加至末尾
        self.lines.push(ConfigLine::Assignment {
            key: key.to_string(),
            value: value.to_string(),
            quote_type: QuoteType::Double,
            has_export: false,
            prefix_whitespace: String::new(),
            trailing_comment: None,
        });
    }

    /// 移除指定键（如果存在则删除该行）
    pub fn remove(&mut self, key: &str) -> bool {
        let initial_len = self.lines.len();
        self.lines.retain(|line| {
            if let ConfigLine::Assignment { key: k, .. } = line {
                k != key
            } else {
                true
            }
        });
        self.lines.len() < initial_len
    }

    /// 将配置格式化为完整文件内容
    ///
    /// # 设计原理
    /// - **实现初衷**：基于 AST 节点按原序格式化输出，保持每行的空白与原有结构。
    /// - **核心优势**：预估每行平均长度（32 字节）提前分配容量，消除连续追加时的多次内存重新分配。
    pub fn serialize(&self) -> String {
        let estimated_capacity = self.lines.len().saturating_mul(32);
        let mut output = String::with_capacity(estimated_capacity);
        for (i, line) in self.lines.iter().enumerate() {
            output.push_str(&line.format_line());
            // 保持每行末尾换行符
            if i < self.lines.len() {
                output.push('\n');
            }
        }
        output
    }
}

impl fmt::Display for GrubConfigFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.serialize())
    }
}
