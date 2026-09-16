//! 核心库的统一错误类型。
//!
//! 各模块落地时逐步补充细分变体，避免过早把尚未验证的错误形态固化成 API。
//! **面向用户的错误信息要能直接展示** —— 所以文案是中文、说清「发生了什么」与「怎么办」，
//! 而不是只把底层错误串起来。

use std::path::PathBuf;

/// 核心库统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 文件系统或设备 IO 错误。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    /// 遇到尚未支持的输入（文件格式、相机型号、特性等）。
    #[error("不支持：{0}")]
    Unsupported(String),

    /// SQLite 层面的错误。
    #[error("数据库错误：{0}")]
    Database(#[from] rusqlite::Error),

    /// 数据库的 schema 版本**比本程序支持的更新** —— 必须拒绝打开。
    ///
    /// 这是「用户用新版程序建了库，又用旧版程序打开」的场景。
    /// 强行打开会写坏新版的表结构，所以宁可不打开。
    #[error(
        "这个库是更新版本的 raybend 创建的（库版本 {found}，本程序支持到 {supported}）。\
         请升级 raybend 后再打开；若你确实要用旧版打开，请先备份这个库。"
    )]
    SchemaTooNew { found: i64, supported: i64 },

    /// 迁移过程中的失败（已回滚，库保持原样）。
    #[error("升级数据库失败（迁移 {version}）：{source}")]
    Migration {
        version: i64,
        #[source]
        source: rusqlite::Error,
    },

    /// 迁移后外键检查不通过（说明迁移写坏了数据）。
    #[error("升级数据库后完整性检查未通过：{0}")]
    IntegrityCheck(String),

    /// 磁盘空间不足 —— 单独成一类，因为它的提示要明确（导入/快照都会遇到）。
    #[error("磁盘空间不足：{0}")]
    OutOfSpace(String),

    /// 目标文件已经存在 —— 导入**绝不覆盖**已存在的文件（`REPOSITORY.md` §3.4）。
    ///
    /// 规划阶段已经用重名后缀尽量避开，这条是最后一道闸：撞上就当这一条失败。
    #[error("目标已存在：{0}")]
    TargetExists(String),

    /// JSON 解析/序列化错误（`repo.json`、任务载荷、设置值等）。
    #[error("JSON 错误：{0}")]
    Json(#[from] serde_json::Error),

    /// 路径不存在（重命名/移动后找不到）。
    #[error("路径不存在：{}", .0.display())]
    PathNotFound(PathBuf),

    /// 目录不是一个库（缺少 `catalog.db`，或其中的库 ID 与预期不符）。
    #[error("不是有效的库目录：{}（{reason}）", .path.display())]
    NotARepository { path: PathBuf, reason: String },

    /// 拿不到系统随机数（沙箱/容器里极少见）。
    #[error("无法获取系统随机数：{0}")]
    Random(String),

    /// 写线程已经不在了（任务里 panic 或线程异常退出）。
    ///
    /// 出现它一定是程序 bug —— 报给用户没有意义，日志里要留全。
    #[error("数据库写线程已停止（程序内部错误）")]
    WriterGone,

    /// 库当前处于离线状态（登记的路径下找不到它的 `catalog.db`）。
    #[error("库「{name}」当前离线：登记过的 {tried} 个路径下都没有找到它")]
    RepositoryOffline { name: String, tried: usize },
}

impl Error {
    /// 供 UI 判断「是否可以提示用户重试」的粗分类。
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Database(_) | Self::OutOfSpace(_))
    }
}

/// 核心库统一 `Result` 别名。
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_too_new_message_mentions_both_versions() {
        let e = Error::SchemaTooNew {
            found: 7,
            supported: 3,
        };
        let msg = e.to_string();
        assert!(
            msg.contains('7') && msg.contains('3'),
            "信息里要有两个版本号"
        );
        assert!(!e.is_retryable(), "版本不匹配重试也不会好");
    }

    #[test]
    fn io_error_is_retryable_but_unsupported_is_not() {
        let io = Error::Io(std::io::Error::other("x"));
        assert!(io.is_retryable());
        assert!(!Error::Unsupported("x".into()).is_retryable());
        assert!(!Error::PathNotFound(PathBuf::from("/tmp/x")).is_retryable());
        assert!(!Error::WriterGone.is_retryable(), "写线程没了，重试也没用");
    }

    #[test]
    fn library_offline_message_has_name_and_count() {
        let e = Error::RepositoryOffline {
            name: "照片库".into(),
            tried: 2,
        };
        let msg = e.to_string();
        assert!(msg.contains("照片库") && msg.contains('2'));
    }

    #[test]
    fn path_not_found_shows_unicode_path() {
        let e = Error::PathNotFound(PathBuf::from("/mnt/x/照片/婚礼.jpg"));
        assert!(e.to_string().contains("婚礼.jpg"));
    }

    #[test]
    fn not_a_library_has_path_and_reason() {
        let e = Error::NotARepository {
            path: PathBuf::from("/mnt/d/库"),
            reason: "缺少 catalog.db".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("catalog.db") && msg.contains("/mnt/d/库"));
    }
}
