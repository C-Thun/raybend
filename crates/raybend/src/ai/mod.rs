//! AI 标签的数值契约。推理依赖在 ai-runtime 特性下启用，实验 probe 复用同一编码器与评分器。
pub mod jobs;
pub mod pack;
pub mod preprocess;
pub mod scoring;

pub mod source;

pub mod status;

#[cfg(feature = "ai-runtime")]
pub mod runtime;
#[cfg(feature = "ai-runtime")]
pub mod worker;

pub mod result;

pub mod coordinator;
pub mod session;

pub mod registry;

pub mod range;

pub mod service;

/// Shared worker marker, recognized even by builds without the inference backend.
pub const WORKER_ARG: &str = "--raybend-ai-worker";
