use std::error::Error;
use std::fmt;
use std::io;
use std::process::{Command, Output};
use tracing::{debug, warn};

/// 安全执行器错误枚举
#[derive(Debug, PartialEq, Eq)]
pub enum SecurityError {
    /// 尝试执行非白名单允许的程序
    ProgramNotAllowed { program: String },
    /// 参数中包含危险的注入字符或非法格式
    DangerousArgument { arg: String, reason: String },
}

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityError::ProgramNotAllowed { program } => {
                write!(
                    f,
                    "安全拦截：程序 '{}' 未在特权执行白名单中，拒绝执行",
                    program
                )
            }
            SecurityError::DangerousArgument { arg, reason } => {
                write!(f, "安全拦截：参数 '{}' 存在注入风险，原因: {}", arg, reason)
            }
        }
    }
}

impl Error for SecurityError {}

/// 受信任的特权引导命令静态白名单集合
const ALLOWED_PROGRAMS: &[&str] = &[
    "update-grub",
    "/usr/sbin/update-grub",
    "grub-mkconfig",
    "/usr/sbin/grub-mkconfig",
    "/usr/bin/grub-mkconfig",
    "grub2-mkconfig",
    "/usr/sbin/grub2-mkconfig",
    "/usr/bin/grub2-mkconfig",
    "grub-script-check",
    "/usr/bin/grub-script-check",
    "grub2-script-check",
    "/usr/bin/grub2-script-check",
    "grub-set-default",
    "/usr/bin/grub-set-default",
    "grub2-set-default",
    "/usr/bin/grub2-set-default",
    // 单元测试与模拟环境允许的受限程序
    "true",
    "false",
    "cmd",
];

/// 严禁出现的危险控制字符集合（防止 shell 注入）
const DANGEROUS_CHARS: &[char] = &['\n', '\r', ';', '&', '|', '`', '$', '(', ')', '<', '>'];

/// 受白名单严格限制的安全命令构建器
///
/// # 设计原理
/// - **实现初衷**：在特权守护进程中调用外部系统命令时，绝不允许动态调用任意程序或直接经由 Shell 解释。
/// - **核心优势**：
///   1. 必须命中预定义白名单（`ALLOWED_PROGRAMS`）；
///   2. 所有参数均逐字符检测是否包含 Shell 元字符（`;`, `&`, `|`, `$()` 等），彻底杜绝拼接提权；
///   3. 统一使用直接进程派生（`Command::new`），绝不调用 `sh -c`。
/// - **代价与局限**：调用方无法使用复杂的 Shell 管道或通配符扩展，需使用纯参数列表。
#[derive(Debug, Clone)]
pub struct SafeCommand {
    program: String,
    args: Vec<String>,
}

impl SafeCommand {
    /// 创建受白名单保护的安全命令实例
    ///
    /// # Errors
    /// 当程序名称或路径未在受信任白名单中时返回 `SecurityError::ProgramNotAllowed`。
    pub fn new(program: &str) -> Result<Self, SecurityError> {
        if !is_program_allowed(program) {
            warn!("拒绝执行非白名单程序: {}", program);
            return Err(SecurityError::ProgramNotAllowed {
                program: program.to_string(),
            });
        }
        Ok(Self {
            program: program.to_string(),
            args: Vec::new(),
        })
    }

    /// 追加单个校验后的安全参数
    ///
    /// # Errors
    /// 当参数包含危险控制字符时返回 `SecurityError::DangerousArgument`。
    pub fn arg(mut self, arg: &str) -> Result<Self, SecurityError> {
        validate_argument(arg)?;
        self.args.push(arg.to_string());
        Ok(self)
    }

    /// 追加一组校验后的安全参数
    ///
    /// # Errors
    /// 当任意参数包含危险控制字符时返回 `SecurityError::DangerousArgument`。
    pub fn args<I, S>(mut self, args: I) -> Result<Self, SecurityError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for a in args {
            let s = a.as_ref();
            validate_argument(s)?;
            self.args.push(s.to_string());
        }
        Ok(self)
    }

    /// 执行命令并获取完整标准输出与标准错误
    ///
    /// # Errors
    /// 当操作系统底层派生子进程失败时返回 `io::Error`。
    pub fn output(&self) -> io::Result<Output> {
        debug!("安全执行特权白名单命令: {} {:?}", self.program, self.args);
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd.output()
    }

    /// 带设限超时执行命令，若超时则主动终止并收割子进程，杜绝僵尸进程残留
    ///
    /// # 设计原理
    /// - **实现初衷**：grub-mkconfig 可能因挂载故障或磁盘坏道而假死，必须设限超时并可靠回收。
    /// - **核心优势**：双管道异步消费防止大输出死锁，超时后显式调用 `.kill()` 和 `.wait()` 彻底收割进程控制块。
    ///
    /// # Errors
    /// 当子进程启动失败、超时或等待异常时返回 `io::Error`（超时返回 `ErrorKind::TimedOut`）。
    pub fn output_with_timeout(&self, timeout: std::time::Duration) -> io::Result<Output> {
        debug!(
            "带超时 ({:?}) 执行特权白名单命令: {} {:?}",
            timeout, self.program, self.args
        );
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        let mut stdout_handle = None;
        let mut stderr_handle = None;

        if let Some(mut out) = child.stdout.take() {
            stdout_handle = Some(std::thread::spawn(move || {
                let mut buf = Vec::new();
                use std::io::Read;
                let _ = out.read_to_end(&mut buf);
                buf
            }));
        }

        if let Some(mut err) = child.stderr.take() {
            stderr_handle = Some(std::thread::spawn(move || {
                let mut buf = Vec::new();
                use std::io::Read;
                let _ = err.read_to_end(&mut buf);
                buf
            }));
        }

        let start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => {
                    let stdout = stdout_handle
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    let stderr = stderr_handle
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    return Ok(Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
                None => {
                    if start.elapsed() >= timeout {
                        warn!(
                            "特权命令执行超时 ({:?})，正在终止子进程 PID: {}",
                            timeout,
                            child.id()
                        );
                        let _ = child.kill();
                        // 强制收割子进程退出状态，杜绝僵尸进程
                        let _ = child.wait();
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("特权命令执行超时 ({:?})，已强制终止并回收子进程", timeout),
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        }
    }
}

/// 检查程序是否在受信任白名单中
fn is_program_allowed(program: &str) -> bool {
    let prog_name = std::path::Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program);

    ALLOWED_PROGRAMS.contains(&program) || ALLOWED_PROGRAMS.contains(&prog_name)
}

/// 校验参数是否合法且不含危险字符
fn validate_argument(arg: &str) -> Result<(), SecurityError> {
    for c in DANGEROUS_CHARS {
        if arg.contains(*c) {
            return Err(SecurityError::DangerousArgument {
                arg: arg.to_string(),
                reason: format!("参数包含危险字符 '{}'，可能引发注入", c),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_programs() {
        assert!(SafeCommand::new("grub-mkconfig").is_ok());
        assert!(SafeCommand::new("/usr/sbin/grub-mkconfig").is_ok());
        assert!(SafeCommand::new("update-grub").is_ok());
        assert!(SafeCommand::new("grub-script-check").is_ok());
        assert!(SafeCommand::new("grub-set-default").is_ok());
    }

    #[test]
    fn test_disallowed_programs() {
        assert_eq!(
            SafeCommand::new("rm").unwrap_err(),
            SecurityError::ProgramNotAllowed {
                program: "rm".to_string()
            }
        );
        assert_eq!(
            SafeCommand::new("/bin/bash").unwrap_err(),
            SecurityError::ProgramNotAllowed {
                program: "/bin/bash".to_string()
            }
        );
    }

    #[test]
    fn test_dangerous_arguments_rejected() {
        let cmd = SafeCommand::new("grub-mkconfig").unwrap();
        assert!(cmd.clone().arg("-o").is_ok());
        assert!(cmd.clone().arg("/boot/grub/grub.cfg").is_ok());

        // 注入测试
        assert!(cmd.clone().arg("test; rm -rf /").is_err());
        assert!(cmd.clone().arg("test | cat").is_err());
        assert!(cmd.clone().arg("test $(whoami)").is_err());
        assert!(cmd.clone().arg("test`id`").is_err());
        assert!(cmd.clone().arg("test\nnewline").is_err());
    }
}
