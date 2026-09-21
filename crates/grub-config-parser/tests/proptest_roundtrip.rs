use grub_config_parser::parse_grub_config;
use proptest::prelude::*;

/// 生成合法的 Shell 变量名策略
fn arb_key() -> impl Strategy<Value = String> {
    // 标识符以字母或下划线开头，后续由字母、数字或下划线组成
    "[a-zA-Z_][a-zA-Z0-9_]{0,20}"
}

/// 生成合法的赋值右侧值策略
fn arb_value() -> impl Strategy<Value = String> {
    prop_oneof![
        // 双引号包裹的值（内部不含换行、双引号与反斜杠）
        "[^\r\n\"\\\\]{0,30}".prop_map(|v| format!("\"{}\"", v)),
        // 单引号包裹的值（内部不含换行与单引号）
        "[^\r\n']{0,30}".prop_map(|v| format!("'{}'", v)),
        // 无引号的值（不含空白、#、引号）
        "[a-zA-Z0-9_./-]{0,20}",
    ]
}

/// 生成单行配置行策略
fn arb_config_line() -> impl Strategy<Value = String> {
    prop_oneof![
        // 空行
        Just("".to_string()),
        // 纯注释行
        "[^\r\n]{0,50}".prop_map(|c| format!("# {}", c)),
        // 缩进纯注释行
        "[^\r\n]{0,50}".prop_map(|c| format!("   # {}", c)),
        // 赋值行（带前置缩进与行尾注释）
        (arb_key(), arb_value(), any::<bool>()).prop_map(|(k, v, has_comment)| {
            if has_comment {
                format!("{}={} # 动态注释", k, v)
            } else {
                format!("{}={}", k, v)
            }
        }),
        // export 形式的赋值行
        (arb_key(), arb_value()).prop_map(|(k, v)| format!("export {}={}", k, v)),
    ]
}

/// 生成完整多行配置文件策略
fn arb_config_content() -> impl Strategy<Value = String> {
    prop::collection::vec(arb_config_line(), 0..30).prop_map(|lines| {
        let mut content = lines.join("\n");
        content.push('\n');
        content
    })
}

proptest! {
    /// 往返保真测试：任意生成的配置文本，解析后再序列化必须 100% 还原输入
    #[test]
    fn test_roundtrip_idempotence(content in arb_config_content()) {
        let parsed = parse_grub_config(&content);
        let serialized = parsed.serialize();
        prop_assert_eq!(content, serialized);
    }

    /// AST 稳定测试：对任意文本，两次解析的 AST 结构必须完全相等
    #[test]
    fn test_ast_parse_stability(content in arb_config_content()) {
        let first_parse = parse_grub_config(&content);
        let serialized = first_parse.serialize();
        let second_parse = parse_grub_config(&serialized);
        prop_assert_eq!(first_parse, second_parse);
    }

    /// 修改操作幂等测试：更新已存在的变量后，序列化再解析的值必须保持一致
    #[test]
    fn test_mutation_preservation(
        content in arb_config_content(),
        key in arb_key(),
        val in "[a-zA-Z0-9_-]{1,20}"
    ) {
        let mut parsed = parse_grub_config(&content);
        parsed.set(&key, &val);

        let serialized = parsed.serialize();
        let reparsed = parse_grub_config(&serialized);

        prop_assert_eq!(reparsed.get(&key), Some(val.as_str()));
    }

    /// 模糊健壮性测试：对完全随机的任意畸形字节/字符串，解析器必须永远安全防御、杜绝 panic
    #[test]
    fn test_fuzz_arbitrary_string_no_panic(random_input in ".*") {
        let parsed = parse_grub_config(&random_input);
        let serialized = parsed.serialize();
        let _ = parse_grub_config(&serialized);
    }
}
