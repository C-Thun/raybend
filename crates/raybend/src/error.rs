//! 核心库的统一错误类型。
//!
//! M0 阶段刻意保持最小：只覆盖「IO」与「不支持」两类。
//! 各模块落地时（M0-3 RAW、M0-4 索引……）再补充细分变体，
//! 避免过早把尚未验证的错误形态固化成 API。

/// 核心库统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 文件系统或设备 IO 错误。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    /// 遇到尚未支持的输入（文件格式、相机型号、特性等）。
    #[error("不支持：{0}")]
    Unsupported(String),
}

/// 核心库统一 `Result` 别名。
pub type Result<T, E = Error> = std::result::Result<T, E>;
