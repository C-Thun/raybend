//! **独立崩溃助手进程**（`specs/logging-and-crash-report.md` §5.2）。
//!
//! 主程序（可能崩的那个）通过命名管道与我们对话：
//!
//! ```text
//! raybend-desktop.exe ──创建管道──▶ raybend-crashpad.exe（本进程）
//!       │  正常消息：本次崩溃目录（让 dump 与 report.json 落在同一个 RB-* 里）
//!       │  崩溃时：request_dump(crash_context)
//!       ▼
//!   crashes/RB-<yyyymmdd>-<6HEX>/dump.dmp + dump.json
//! ```
//!
//! 三条设计约束（都是血泪换来的）：
//!
//! 1. **不做 re-exec 主程序**：社区有过「re-exec 自己导致启动连崩三次」的记录
//!    （https://github.com/readest/readest/commit/c37d7eb2b37184f20e57d202045fb8379e4b2e2a ）；
//!    独立小二进制没有这个失败模式。
//! 2. **主进程走了就退出**（`on_client_disconnected` → `LoopAction::Exit`）：否则每次崩溃都会
//!    在用户机器上留一个孤儿进程。
//! 3. **dump 档位不含全量内存**：由 minidumper 的默认档（线程栈 + 模块表）决定，
//!    既够定位、又把隐私面压到最小（规格 §5.2）。

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use minidumper::{LoopAction, MinidumpBinary, Server, ServerHandler, SocketName};

/// 消息：本次崩溃的目标目录（主进程在崩溃前发过来）。
const MSG_CRASH_DIR: u32 = 1;

struct Handler {
    /// `crashes/` 根目录（命令行传入）。
    crash_root: PathBuf,
    /// 主进程指定的目标目录；它没来得及说（段错误直接崩）时我们自己开一个。
    target: Mutex<Option<PathBuf>>,
}

impl ServerHandler for Handler {
    fn create_minidump_file(&self) -> Result<(std::fs::File, PathBuf), io::Error> {
        let dir = self
            .target
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .unwrap_or_else(|| self.fresh_dir());
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("dump.dmp");
        let file = std::fs::File::create(&path)?;
        Ok((file, path))
    }

    fn on_minidump_created(&self, result: Result<MinidumpBinary, minidumper::Error>) -> LoopAction {
        match result {
            Ok(mut binary) => {
                let _ = binary.file.flush();
                let path = binary.path.clone();
                let bytes = binary.file.metadata().map(|meta| meta.len()).unwrap_or(0);
                write_marker(&path, bytes);
                eprintln!("[crashpad] dump 已写出：{}（{bytes} 字节）", path.display());
            }
            Err(error) => {
                eprintln!("[crashpad] 写 dump 失败：{error}");
            }
        }
        // 继续守着：同一次会话里主进程如果再次崩（unwind 档位下可能），还能再落一份
        LoopAction::Continue
    }

    fn on_message(&self, kind: u32, buffer: Vec<u8>) {
        if kind == MSG_CRASH_DIR
            && let Ok(text) = String::from_utf8(buffer)
            && let Ok(mut slot) = self.target.lock()
        {
            *slot = Some(PathBuf::from(text));
        }
    }

    fn on_client_disconnected(&self, _num_clients: usize) -> LoopAction {
        // 主进程没了（正常退出或崩掉之后）—— 助手跟着走，不留孤儿
        LoopAction::Exit
    }
}

impl Handler {
    /// 没收到目标目录时自己开一个：`crashes/RB-<yyyymmdd>-<6HEX>/`。
    fn fresh_dir(&self) -> PathBuf {
        let mut bytes = [0_u8; 3];
        if getrandom_fill(&mut bytes).is_err() {
            bytes = [0x00, 0x00, 0x01];
        }
        self.crash_root.join(format!(
            "RB-{}-{:02X}{:02X}{:02X}",
            yymmdd(),
            bytes[0],
            bytes[1],
            bytes[2]
        ))
    }
}

/// 在 dump 旁边落一份小标记（人/机器都能确认「这份 dump 是什么时候、多大」）。
fn write_marker(dump_path: &Path, bytes: u64) {
    let Some(dir) = dump_path.parent() else {
        return;
    };
    let marker = format!(
        "{{\n  \"kind\": \"hard-crash\",\n  \"dump\": \"dump.dmp\",\n  \"bytes\": {bytes},\n  \"writtenAt\": {}\n}}\n",
        now_unix_seconds()
    );
    let _ = std::fs::write(dir.join("dump.json"), marker);
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_secs())
}

/// `yyyymmdd`（UTC）。这里刻意**不复用** `raybend` 主库的实现：助手必须保持极小的依赖树
/// （那 20 行日期算法换来的是「主库崩了它也不崩」）。
fn yymmdd() -> String {
    let secs = now_unix_seconds() as i64;
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}")
}

/// Howard Hinnant 的 `civil_from_days`（与 `raybend::diagnostics::timefmt` 同一算法）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `getrandom` 不在助手的依赖树里（依赖越少越好）：用时间戳凑随机段，
/// 它只影响目录名的唯一性（同一秒内两次崩才可能撞，而那时 dump 是覆盖写，无害）。
fn getrandom_fill(bytes: &mut [u8; 3]) -> Result<(), ()> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.subsec_nanos())
        .unwrap_or(0);
    bytes.copy_from_slice(&nanos.to_le_bytes()[..3]);
    Ok(())
}

struct Args {
    pipe: String,
    crash_root: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

/// 参数解析本体（吃一个迭代器 —— 测试才不用碰真实的 `env::args`）。
fn parse_args_from<I>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = String>,
{
    let mut pipe = None;
    let mut crash_root = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pipe" => pipe = args.next(),
            "--crash-root" => crash_root = args.next(),
            other => return Err(format!("未知参数：{other}")),
        }
    }
    Ok(Args {
        pipe: pipe.ok_or("缺少 --pipe")?,
        crash_root: PathBuf::from(crash_root.ok_or("缺少 --crash-root")?),
    })
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("[crashpad] 参数错误：{error}");
            std::process::exit(2);
        }
    };
    let handler = Handler {
        crash_root: args.crash_root,
        target: Mutex::new(None),
    };
    let mut server = match Server::with_name(SocketName::path(&args.pipe)) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("[crashpad] 建不了管道：{error}");
            std::process::exit(2);
        }
    };
    let abort = std::sync::atomic::AtomicBool::new(false);
    if let Err(error) = server.run(Box::new(handler), &abort, None) {
        eprintln!("[crashpad] 监听退出：{error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_is_eight_digits() {
        let date = yymmdd();
        assert_eq!(date.len(), 8, "{date}");
        assert!(date.chars().all(|c| c.is_ascii_digit()), "{date}");
        assert!(date.starts_with("20"), "{date}");
    }

    #[test]
    fn fallback_dir_has_the_reported_shape() {
        let handler = Handler {
            crash_root: PathBuf::from("C:\\crashes"),
            target: Mutex::new(None),
        };
        let name = handler
            .fresh_dir()
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "{name}");
        assert_eq!(parts[0], "RB");
        assert_eq!(parts[1].len(), 8);
        assert_eq!(parts[2].len(), 6);
    }

    #[test]
    fn args_parse_and_reject_junk() {
        // 合法组合（含换序 —— 参数顺序不该影响结果）
        let args = parse_args_from(
            ["--pipe", "raybend-crashpad-1", "--crash-root", "C:\\crashes"].map(String::from),
        )
        .expect("合法参数应当解析成功");
        assert_eq!(args.pipe, "raybend-crashpad-1");
        assert_eq!(args.crash_root, PathBuf::from("C:\\crashes"));

        let swapped = parse_args_from(["--crash-root", "C:\\crashes", "--pipe", "p"].map(String::from))
            .expect("换序也应成功");
        assert_eq!(swapped.pipe, "p");

        // 缺项 / 未知参数 / 缺值，全部必须是错误（旧版本这条测试断言的是自己写的字面量，永不失败）
        assert!(parse_args_from(Vec::<String>::new()).is_err(), "什么都没给");
        assert!(parse_args_from(["--pipe", "p"].map(String::from)).is_err(), "缺 --crash-root");
        assert!(parse_args_from(["--crash-root", "C:\\c"].map(String::from)).is_err(), "缺 --pipe");
        assert!(parse_args_from(["--pipe", "p", "--nope"].map(String::from)).is_err(), "未知参数");
        assert!(parse_args_from(["--pipe"].map(String::from)).is_err(), "--pipe 没有值");
    }

    #[test]
    fn marker_json_mentions_size_and_kind() {
        let dir = tempfile::tempdir().expect("建临时目录");
        let dump = dir.path().join("dump.dmp");
        std::fs::write(&dump, b"x").expect("写假 dump");
        write_marker(&dump, 1);
        let marker = std::fs::read_to_string(dir.path().join("dump.json")).expect("读标记");
        assert!(marker.contains("hard-crash"));
        assert!(marker.contains("\"bytes\": 1"));
    }
}
