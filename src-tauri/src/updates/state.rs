//! 更新状态的落盘与版本记账。
//!
//! 目录（`<app_local_data>/updates/`）：
//! - `state.json` —— 唯一事实源：phase、进度、跳过 / 待安装 / 已问过安装的版本
//! - `<version>.part` —— 下载中的临时文件（断点续传的字节基础）
//! - `<version>.bin` —— **已通过签名校验**的成品（安装时读它）
//!
//! 写盘一律「临时文件 + 改名」：断电或崩溃不会留下半个 JSON。
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 状态文件版本。不认识的值一律按空状态起步（宁可重来，不猜旧格式）。
pub const SCHEMA: u32 = 1;
const STATE_FILE: &str = "state.json";
const TMP_FILE: &str = "state.tmp";
/// 版本号要进文件名，先过白名单（版本来自签名清单，但不拿远端字符串直接拼路径）。
const VERSION_MAX: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// 没有在办的事
    #[default]
    Idle,
    /// 用户已点「下载」，正在下或上次被进程退出打断（启动后自动接着下）
    Downloading,
    /// 下载出错（网络 / 校验）：不提示、不自动重试，留进度等手动或下次启动重试一次
    Paused,
    /// 下载并校验完成，等下一次启动时问一次「安装 / 取消」
    Ready,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stored {
    pub schema: u32,
    pub phase: Phase,
    /// 在办 / 待安装的版本（`Idle` 时为 `None`）
    #[serde(default)]
    pub version: Option<String>,
    /// 已下载字节（断点续传的起点）
    #[serde(default)]
    pub done_bytes: u64,
    #[serde(default)]
    pub total_bytes: Option<u64>,
    /// 用户点过「跳过」的版本 —— **只压自动提示**，手动检查照常显示
    #[serde(default)]
    pub skipped_version: Option<String>,
    /// 已经弹过「可以安装」的版本（只问一次，取消也不再问）
    #[serde(default)]
    pub install_prompted_version: Option<String>,
    /// 已经静默重试过一次的版本（每个版本只重试一次）
    #[serde(default)]
    pub retry_attempted_version: Option<String>,
    /// 最近一次下载失败的原因（只在「检查更新」里给人看，不弹窗）
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            phase: Phase::Idle,
            version: None,
            done_bytes: 0,
            total_bytes: None,
            skipped_version: None,
            install_prompted_version: None,
            retry_attempted_version: None,
            last_error: None,
        }
    }
}

impl Stored {
    /// 读盘；文件缺失、版本不符或内容坏了都退回空状态（不抛错、不干扰启动）。
    pub fn load(dir: &Path) -> Self {
        let Ok(text) = fs::read_to_string(dir.join(STATE_FILE)) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(stored) if stored.schema == SCHEMA => stored,
            _ => {
                eprintln!("[update] 状态文件不认识，按空状态起步");
                Self::default()
            }
        }
    }

    /// 落盘（临时文件 + 改名）。失败只记日志：状态写不进去不该打断下载。
    pub fn save(&self, dir: &Path) {
        let Ok(text) = serde_json::to_string_pretty(self) else {
            return;
        };
        if let Err(error) = fs::write(dir.join(TMP_FILE), text).and_then(|()| {
            fs::rename(dir.join(TMP_FILE), dir.join(STATE_FILE))
        }) {
            eprintln!("[update] 状态落盘失败：{error}");
        }
    }

    /// 版本号白名单：非空、只含 `[0-9A-Za-z.+-]`、不以 `.` 开头、不超过 64 字节。
    pub fn safe_version(version: &str) -> Result<&str, String> {
        let ok = !version.is_empty()
            && version.len() <= VERSION_MAX
            && !version.starts_with('.')
            && version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
            && version != "."
            && version != "..";
        if ok {
            Ok(version)
        } else {
            Err("UPDATE_VERSION_INVALID".into())
        }
    }

    pub fn part_path(dir: &Path, version: &str) -> PathBuf {
        dir.join(format!("{version}.part"))
    }

    pub fn bin_path(dir: &Path, version: &str) -> PathBuf {
        dir.join(format!("{version}.bin"))
    }

    /// 清掉与 `keep` 无关的 `.part` / `.bin`（换版本、装完之后收摊）。
    pub fn cleanup(dir: &Path, keep: Option<&str>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let (Some(stem), Some(ext)) = (
                path.file_stem().and_then(|s| s.to_str()),
                path.extension().and_then(|s| s.to_str()),
            ) else {
                continue;
            };
            if !matches!(ext, "part" | "bin") || keep == Some(stem) {
                continue;
            }
            if let Err(error) = fs::remove_file(&path) {
                eprintln!("[update] 清理旧文件失败 {}：{error}", path.display());
            }
        }
    }

    /// 自动提示要不要压住：这个版本已跳过、或已经在办（下载中 / 待安装）。
    pub fn auto_suppressed(&self, version: &str) -> bool {
        self.skipped_version.as_deref() == Some(version)
            || (self.version.as_deref() == Some(version) && self.phase != Phase::Idle)
    }

    /// 启动时该不该静默重试一次：上次失败、这个版本还没重试过。
    pub fn should_auto_retry(&self) -> bool {
        self.phase == Phase::Paused
            && self.version.is_some()
            && self.retry_attempted_version != self.version
    }

    /// 开始（或继续）下载。
    pub fn start_download(&mut self, version: &str, done: u64, total: Option<u64>, automatic: bool) {
        if self.version.as_deref() != Some(version) {
            // 换版本：旧进度作废
            self.done_bytes = 0;
            self.total_bytes = None;
            self.install_prompted_version = None;
        }
        self.phase = Phase::Downloading;
        self.version = Some(version.to_string());
        self.done_bytes = done;
        if total.is_some() {
            self.total_bytes = total;
        }
        self.last_error = None;
        if automatic {
            self.retry_attempted_version = Some(version.to_string());
        }
    }

    pub fn advance(&mut self, done: u64, total: Option<u64>) {
        self.done_bytes = done;
        if total.is_some() {
            self.total_bytes = total;
        }
    }

    /// 下载 + 校验通过：进入「等下一次启动问安装」。
    pub fn complete(&mut self) {
        self.phase = Phase::Ready;
        if let Some(total) = self.total_bytes {
            self.done_bytes = total;
        }
    }

    /// 下载或校验失败：留进度（可续），记原因，不提示。
    pub fn fail(&mut self, error: String) {
        self.phase = Phase::Paused;
        self.last_error = Some(error);
    }

    /// 回空状态（安装已交接、或换版本作废）。
    pub fn reset(&mut self) {
        let skipped = self.skipped_version.clone();
        *self = Self {
            skipped_version: skipped,
            ..Self::default()
        };
    }

    /// 记「这个版本跳过了」：只影响自动提示。
    pub fn skip(&mut self, version: &str) {
        self.skipped_version = Some(version.to_string());
        if self.version.as_deref() == Some(version) && self.phase != Phase::Ready {
            // 正在下载 / 失败的同一版本被跳过：进度作废，别留下半截文件
            self.reset_files_pending();
        }
    }

    fn reset_files_pending(&mut self) {
        self.phase = Phase::Idle;
        self.version = None;
        self.done_bytes = 0;
        self.total_bytes = None;
        self.last_error = None;
    }

    /// 记「已经问过安装」。
    pub fn mark_install_prompted(&mut self) {
        self.install_prompted_version = self.version.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "raybend-update-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = temp_dir();
        let mut stored = Stored::default();
        stored.start_download("0.2.0", 4096, Some(120_000_000), false);
        stored.save(&dir);
        let back = Stored::load(&dir);
        assert_eq!(back, stored);
        assert_eq!(back.phase, Phase::Downloading);
        assert_eq!(back.done_bytes, 4096);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_or_missing_state_falls_back_to_empty() {
        let dir = temp_dir();
        assert_eq!(Stored::load(&dir), Stored::default());
        fs::write(dir.join(STATE_FILE), "{ 这不是 JSON").expect("写坏文件");
        assert_eq!(Stored::load(&dir), Stored::default());
        // 版本号不认识（将来改了 schema）也退回空状态
        fs::write(
            dir.join(STATE_FILE),
            r#"{"schema":99,"phase":"ready","version":"9.9.9"}"#,
        )
        .expect("写新版本文件");
        assert_eq!(Stored::load(&dir), Stored::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_whitelist_rejects_path_tricks() {
        assert!(Stored::safe_version("0.1.2").is_ok());
        assert!(Stored::safe_version("1.0.0-beta.1+build").is_ok());
        for bad in ["", ".", "..", "../x", "a/b", "a\\b", "a b", "..\\x"] {
            assert!(Stored::safe_version(bad).is_err(), "{bad} 不该通过");
        }
        assert!(Stored::safe_version(&"9".repeat(65)).is_err());
    }

    #[test]
    fn cleanup_keeps_only_current_version() {
        let dir = temp_dir();
        for name in ["0.1.0.part", "0.1.1.bin", "0.2.0.part", "state.json"] {
            fs::write(dir.join(name), b"x").expect("造文件");
        }
        Stored::cleanup(&dir, Some("0.2.0"));
        assert!(dir.join("0.2.0.part").exists());
        assert!(!dir.join("0.1.0.part").exists());
        assert!(!dir.join("0.1.1.bin").exists());
        assert!(dir.join("state.json").exists(), "状态文件不是清理对象");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_suppression_covers_skip_and_in_flight() {
        let mut stored = Stored::default();
        assert!(!stored.auto_suppressed("0.2.0"));
        stored.skip("0.2.0");
        assert!(stored.auto_suppressed("0.2.0"));
        assert!(!stored.auto_suppressed("0.3.0"), "更高版本要重新提示");
        stored.start_download("0.3.0", 0, None, false);
        assert!(stored.auto_suppressed("0.3.0"), "正在下载的版本不重复提示");
        stored.reset();
        assert!(!stored.auto_suppressed("0.3.0"));
    }

    #[test]
    fn auto_retry_only_once_per_version() {
        let mut stored = Stored::default();
        assert!(!stored.should_auto_retry(), "没失败过不重试");
        stored.start_download("0.2.0", 0, None, false);
        stored.fail("boom".into());
        assert_eq!(stored.phase, Phase::Paused);
        assert!(stored.should_auto_retry(), "失败后下次启动重试一次");
        stored.start_download("0.2.0", 1024, None, true);
        stored.fail("boom".into());
        assert!(!stored.should_auto_retry(), "重试过就不再自动重试");
        let _ = stored.version.take();
        assert!(!stored.should_auto_retry());
    }

    #[test]
    fn switching_version_drops_progress_and_prompt() {
        let mut stored = Stored::default();
        stored.start_download("0.2.0", 8192, Some(1000), false);
        stored.complete();
        stored.mark_install_prompted();
        assert_eq!(stored.install_prompted_version.as_deref(), Some("0.2.0"));
        stored.start_download("0.3.0", 0, None, false);
        assert_eq!(stored.done_bytes, 0, "换版本进度要归零");
        assert_eq!(stored.install_prompted_version, None, "新版本要重新问安装");
    }

    #[test]
    fn skipping_an_in_flight_version_clears_progress() {
        let mut stored = Stored::default();
        stored.start_download("0.2.0", 500, Some(1000), false);
        stored.skip("0.2.0");
        assert_eq!(stored.phase, Phase::Idle);
        assert_eq!(stored.version, None);
        assert_eq!(stored.done_bytes, 0);
    }
}
