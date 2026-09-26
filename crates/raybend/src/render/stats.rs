//! 帧时间统计与 spike 报告（自记录）。
//!
//! `PLAN.md` A.2 要的每个数字都由这里落盘成 JSON + Markdown：**能自测的都自测**，
//! 人的操作压到「点几下按钮 + 目视填两格」（M2 计划 §7.6 的原话）。
//!
//! 为什么分位数自己算而不是引依赖：样本量小（几百个 `f32`），排序一遍足够，
//! 而这个 crate 的依赖表是要登记的（AGENTS.md §2.9）—— 不值得为它加一个包。

use serde::{Deserialize, Serialize};

/// 帧时间样本（毫秒）与分位数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameStats {
    /// 原始样本（毫秒），按采集顺序
    samples_ms: Vec<f32>,
    /// 采集的总墙钟时间（毫秒）
    pub window_ms: f64,
}

impl FrameStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一帧。
    pub fn push(&mut self, ms: f32) {
        self.samples_ms.push(ms);
    }

    /// 把一段采集的墙钟时间累加进去（用来算实际帧率）。
    pub fn add_window(&mut self, ms: f64) {
        self.window_ms += ms;
    }

    pub fn clear(&mut self) {
        self.samples_ms.clear();
        self.window_ms = 0.0;
    }

    pub fn count(&self) -> usize {
        self.samples_ms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples_ms.is_empty()
    }

    /// 实际帧率：样本数 ÷ 墙钟时间。没采到就不是个数。
    pub fn fps(&self) -> Option<f64> {
        if self.samples_ms.is_empty() || self.window_ms <= 0.0 {
            return None;
        }
        Some(self.samples_ms.len() as f64 / (self.window_ms / 1000.0))
    }

    fn sorted(&self) -> Vec<f32> {
        let mut copy = self.samples_ms.clone();
        copy.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        copy
    }

    /// 分位数（`q` 取 0..1）。样本为空时 `None`。
    ///
    /// 用**最近秩**取法（`rank = ⌈q·N⌉`，不插值）：帧时间的意义是「有多少帧超过了某条线」，
    /// 插值出来的数会掩盖真实的长尾。
    pub fn percentile(&self, q: f32) -> Option<f32> {
        if self.samples_ms.is_empty() {
            return None;
        }
        let sorted = self.sorted();
        let q = q.clamp(0.0, 1.0);
        let rank = (q * sorted.len() as f32).ceil().max(1.0) as usize;
        let index = (rank - 1).min(sorted.len() - 1);
        Some(sorted[index])
    }

    pub fn min(&self) -> Option<f32> {
        self.samples_ms.iter().cloned().fold(None, |acc: Option<f32>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        })
    }

    pub fn max(&self) -> Option<f32> {
        self.samples_ms.iter().cloned().fold(None, |acc: Option<f32>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        })
    }

    pub fn mean(&self) -> Option<f32> {
        if self.samples_ms.is_empty() {
            return None;
        }
        Some(self.samples_ms.iter().sum::<f32>() / self.samples_ms.len() as f32)
    }

    /// 超过 `budget_ms` 的帧占比 —— 「卡不卡」比平均帧时间更能说明问题。
    pub fn over_budget_ratio(&self, budget_ms: f32) -> f32 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        let over = self.samples_ms.iter().filter(|v| **v > budget_ms).count();
        over as f32 / self.samples_ms.len() as f32
    }

    /// 一行摘要（给报告与终端用）。
    pub fn summary(&self) -> String {
        match (self.percentile(0.5), self.percentile(0.95), self.max(), self.fps()) {
            (Some(p50), Some(p95), Some(max), fps) => format!(
                "{} 帧 · p50 {:.2}ms · p95 {:.2}ms · max {:.2}ms · {}",
                self.count(),
                p50,
                p95,
                max,
                fps.map_or("帧率未知".to_string(), |f| format!("{f:.1} fps"))
            ),
            _ => "（没采到样本）".to_string(),
        }
    }
}

/// 一个场景的采集结果（如「静止」「快速平移」「连续缩放」）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScenarioStats {
    pub name: String,
    /// 场景说明：这一步**人该怎么操作**（报告里要原样打印出来）
    pub how: String,
    /// 帧间隔（present 到 present）—— 用户真正感知到的那个数
    #[serde(flatten)]
    pub frames: FrameStats,
    /// 其中**我们自己的 CPU 开销**（`render()` 调用时长）——
    /// 与帧间隔的差额就是等垂直同步/等驱动的时间。两个都记，
    /// 否则「59fps 是渲染满了还是锁在 vsync 上」分不清。
    #[serde(default)]
    pub cpu: FrameStats,
}

/// 适配器/设备信息（A.2 要求能看出到底跑在哪个后端上）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub backend: String,
    pub name: String,
    pub device_type: String,
    pub driver: String,
    pub driver_info: String,
}

/// 表面与窗口信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurfaceInfo {
    pub format: String,
    pub alpha_mode: String,
    /// 窗口内容区物理像素
    pub size_physical: (u32, u32),
    /// 窗口内容区 CSS 像素
    pub size_css: (u32, u32),
    pub dpr: f32,
    /// 当前所在显示器上的缩放比例（多显示器时与 dpr 应当一致）
    pub monitor_scale: f32,
    pub monitor_name: String,
    pub is_maximized: bool,
    pub is_fullscreen: bool,
    pub is_decorated: bool,
    pub is_transparent: bool,
}

/// spike 的一份完整报告：**自记录**，人只补两三个主观项。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpikeReport {
    /// 生成时间（本地时间字符串）
    pub generated_at: String,
    /// 平台与版本
    pub platform: String,
    pub app_version: String,
    /// 环境变量 `WGPU_BACKEND` 的取值（后端回退实验要记录）
    pub wgpu_backend_env: String,
    pub adapter: AdapterInfo,
    pub surface: SurfaceInfo,
    pub image_size: (u32, u32),
    /// 纹理上传耗时（含生成测试图）
    pub upload_ms: f32,
    /// 各场景帧统计
    pub scenarios: Vec<ScenarioStats>,
    /// 设备丢失与恢复（A.2 的一项）
    pub device_lost: Vec<String>,
    /// 视口自查发现的问题（`Viewport::sanity_problems`）
    pub viewport_problems: Vec<String>,
    /// 程序化判定：坐标同步的最大偏差（物理像素），由 Rust 自己往返算出来
    pub coord_roundtrip_max_error_px: f32,
    /// 人需要填的主观项（由界面上的输入框写回来）
    pub human: HumanNotes,
}

/// 只能由人回答的几项（`specs/M2-W1-windows-gpu.md` 里那张表）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumanNotes {
    /// 透明区看到的是桌面/其他窗口，还是有黑底/白边？
    pub transparency_ok: Option<bool>,
    /// 1:1 档位下，图像中心那个白块是不是**方方正正**的一个像素块？
    pub one_to_one_sharp: Option<bool>,
    /// 跨显示器拖动时画面有没有明显错位/闪烁？
    pub across_monitors_ok: Option<bool>,
    /// 主观帧率（快/一般/卡）
    pub feel: String,
    /// 备注
    pub notes: String,
}

impl SpikeReport {
    /// 报告存到哪里：`<dir>/spike-report.json` 与 `.md`。
    ///
    /// 返回写好的两个路径。JSON 给程序看，MD 给人看（也便于贴进 `implementations/`）。
    pub fn write_to(&self, dir: &std::path::Path) -> std::io::Result<(std::path::PathBuf, std::path::PathBuf)> {
        std::fs::create_dir_all(dir)?;
        let json_path = dir.join("spike-report.json");
        let md_path = dir.join("spike-report.md");
        let json = serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"));
        std::fs::write(&json_path, json)?;
        std::fs::write(&md_path, self.to_markdown())?;
        Ok((json_path, md_path))
    }

    /// 人读的那份（也是往 `implementations/` 里贴的原始材料）。
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# 渲染 spike 报告（自动生成）\n\n");
        out.push_str(&format!("- 生成时间：{}\n", self.generated_at));
        out.push_str(&format!("- 平台：{}\n", self.platform));
        out.push_str(&format!("- 版本：{}\n", self.app_version));
        out.push_str(&format!(
            "- `WGPU_BACKEND`：{}\n",
            if self.wgpu_backend_env.is_empty() {
                "(未设置 → 走默认优先级)".to_string()
            } else {
                self.wgpu_backend_env.clone()
            }
        ));
        out.push_str("\n## 适配器与表面\n\n");
        out.push_str(&format!(
            "- 后端：{} · 设备类型：{}\n",
            self.adapter.backend, self.adapter.device_type
        ));
        out.push_str(&format!("- 适配器：{}\n", self.adapter.name));
        out.push_str(&format!(
            "- 驱动：{} {}\n",
            self.adapter.driver, self.adapter.driver_info
        ));
        out.push_str(&format!(
            "- 表面：{} / alpha {}\n",
            self.surface.format, self.surface.alpha_mode
        ));
        out.push_str(&format!(
            "- 尺寸：物理 {}×{} · CSS {}×{} · dpr {}\n",
            self.surface.size_physical.0,
            self.surface.size_physical.1,
            self.surface.size_css.0,
            self.surface.size_css.1,
            self.surface.dpr
        ));
        out.push_str(&format!(
            "- 显示器：{}（缩放 {}）· 最大化 {} · 全屏 {} · 有边框 {} · 透明 {}\n",
            self.surface.monitor_name,
            self.surface.monitor_scale,
            self.surface.is_maximized,
            self.surface.is_fullscreen,
            self.surface.is_decorated,
            self.surface.is_transparent
        ));
        out.push_str(&format!(
            "- 测试图：{}×{} · 上传耗时 {:.1}ms\n",
            self.image_size.0, self.image_size.1, self.upload_ms
        ));

        out.push_str("\n## 帧时间\n\n");
        out.push_str(
            "帧间隔 = present 到 present（用户感知的数）；CPU = 我们 `render()` 自己花的时间。\n\n",
        );
        out.push_str("| 场景 | 怎么操作 | 样本 | 帧间隔 p50 | p95 | max | 帧率 | 超 16.7ms | CPU p50 |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for scenario in &self.scenarios {
            let f = &scenario.frames;
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {:.1}% | {} |\n",
                scenario.name,
                scenario.how,
                f.count(),
                f.percentile(0.5).map_or("—".into(), |v| format!("{v:.2}ms")),
                f.percentile(0.95).map_or("—".into(), |v| format!("{v:.2}ms")),
                f.max().map_or("—".into(), |v| format!("{v:.2}ms")),
                f.fps().map_or("—".into(), |v| format!("{v:.1} fps")),
                f.over_budget_ratio(16.7) * 100.0,
                scenario.cpu.percentile(0.5).map_or("—".into(), |v| format!("{v:.2}ms"))
            ));
        }

        out.push_str("\n## 程序化判定\n\n");
        out.push_str(&format!(
            "- 坐标往返最大偏差：{:.6} 物理像素（屏幕上换算回来应当 < 0.01）\n",
            self.coord_roundtrip_max_error_px
        ));
        if self.viewport_problems.is_empty() {
            out.push_str("- 视口自查：无问题\n");
        } else {
            for problem in &self.viewport_problems {
                out.push_str(&format!("- ⚠ 视口自查：{problem}\n"));
            }
        }
        if self.device_lost.is_empty() {
            out.push_str("- 设备丢失：本次未模拟\n");
        } else {
            for line in &self.device_lost {
                out.push_str(&format!("- 设备丢失：{line}\n"));
            }
        }

        out.push_str("\n## 需要人回答的几项\n\n");
        let yes_no = |v: Option<bool>| match v {
            Some(true) => "是 ✅",
            Some(false) => "否 ❌",
            None => "（未填）",
        };
        out.push_str(&format!(
            "- 透明区是透明的（看到的是桌面/下层窗口，无黑底白边）：{}\n",
            yes_no(self.human.transparency_ok)
        ));
        out.push_str(&format!(
            "- 1:1 档位下白块像素级锐利：{}\n",
            yes_no(self.human.one_to_one_sharp)
        ));
        out.push_str(&format!(
            "- 跨显示器拖动无错位/闪烁：{}\n",
            yes_no(self.human.across_monitors_ok)
        ));
        out.push_str(&format!("- 主观帧率：{}\n", self.human.feel));
        if !self.human.notes.is_empty() {
            out.push_str(&format!("- 备注：{}\n", self.human.notes));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(values: &[f32]) -> FrameStats {
        let mut stats = FrameStats::new();
        for v in values {
            stats.push(*v);
        }
        stats.add_window(values.len() as f64 * 16.7);
        stats
    }

    #[test]
    fn empty_stats_are_honest_about_being_empty() {
        let stats = FrameStats::new();
        assert!(stats.is_empty());
        assert_eq!(stats.count(), 0);
        assert_eq!(stats.percentile(0.5), None);
        assert_eq!(stats.mean(), None);
        assert_eq!(stats.min(), None);
        assert_eq!(stats.max(), None);
        assert_eq!(stats.fps(), None);
        assert_eq!(stats.over_budget_ratio(16.7), 0.0);
        assert!(stats.summary().contains("没采到样本"));
    }

    #[test]
    fn single_sample_is_every_percentile() {
        let stats = stats(&[7.5]);
        for q in [0.0, 0.25, 0.5, 0.95, 1.0] {
            assert_eq!(stats.percentile(q), Some(7.5), "q={q}");
        }
        assert_eq!(stats.min(), Some(7.5));
        assert_eq!(stats.max(), Some(7.5));
        assert_eq!(stats.mean(), Some(7.5));
    }

    #[test]
    fn percentiles_use_nearest_rank_not_interpolation() {
        // 100 个样本：1..=100ms。最近秩：rank = ⌈q·N⌉
        //   p50 → ⌈50⌉ = 50 → 第 50 个样本 = 50ms
        //   p95 → ⌈95⌉ = 95 → 第 95 个样本 = 95ms
        // 插值法会给出 50.5 / 95.05 这种「不存在的帧」，掩盖长尾。
        let values: Vec<f32> = (1..=100).map(|v| v as f32).collect();
        let stats = stats(&values);
        assert_eq!(stats.percentile(0.0), Some(1.0));
        assert_eq!(stats.percentile(0.5), Some(50.0));
        assert_eq!(stats.percentile(0.95), Some(95.0));
        assert_eq!(stats.percentile(1.0), Some(100.0));
    }

    #[test]
    fn out_of_range_quantiles_are_clamped() {
        let stats = stats(&[1.0, 2.0, 3.0]);
        assert_eq!(stats.percentile(-1.0), Some(1.0));
        assert_eq!(stats.percentile(2.0), Some(3.0));
    }

    #[test]
    fn fps_uses_the_wall_clock_not_the_nominal_frame_time() {
        let mut stats = FrameStats::new();
        for _ in 0..60 {
            stats.push(10.0);
        }
        stats.add_window(1000.0); // 60 帧花掉 1 秒 → 60fps
        assert_eq!(stats.fps(), Some(60.0));
    }

    #[test]
    fn over_budget_ratio_counts_the_long_tail() {
        let stats = stats(&[1.0, 2.0, 100.0, 3.0]);
        assert!((stats.over_budget_ratio(16.7) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn clear_resets_everything() {
        let mut stats = stats(&[1.0, 2.0]);
        stats.clear();
        assert!(stats.is_empty());
        assert_eq!(stats.window_ms, 0.0);
        assert_eq!(stats.fps(), None);
    }

    #[test]
    fn summary_mentions_the_numbers_we_report() {
        let stats = stats(&[4.0, 8.0, 16.0, 33.0]);
        let summary = stats.summary();
        assert!(summary.contains("4 帧"), "{summary}");
        assert!(summary.contains("p95"), "{summary}");
        assert!(summary.contains("fps"), "{summary}");
    }

    #[test]
    fn report_markdown_is_readable_and_flags_unfilled_human_items() {
        let mut report = SpikeReport {
            generated_at: "2026-09-17 13:00:00 CST".to_string(),
            platform: "windows-x86_64".to_string(),
            app_version: "0.1.0".to_string(),
            ..Default::default()
        };
        report.adapter.backend = "Dx12".to_string();
        report.adapter.name = "NVIDIA GeForce RTX 4070".to_string();
        report.surface.dpr = 1.25;
        report.image_size = (6000, 4000);
        report.upload_ms = 42.0;
        report.coord_roundtrip_max_error_px = 0.000_001;
        report.scenarios.push(ScenarioStats {
            name: "快速平移".to_string(),
            how: "按住右键左右晃 5 秒".to_string(),
            frames: stats(&[8.0, 9.0, 11.0]),
            cpu: stats(&[1.0, 1.2, 1.4]),
        });

        let md = report.to_markdown();
        assert!(md.contains("Dx12"), "{md}");
        assert!(md.contains("快速平移"), "{md}");
        assert!(md.contains("按住右键左右晃 5 秒"), "报告要写清人该怎么操作");
        assert!(md.contains("（未填）"), "没填的主观项必须显眼");
        assert!(md.contains("坐标往返最大偏差"));
    }

    #[test]
    fn report_round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("临时目录");
        let report = SpikeReport {
            generated_at: "now".to_string(),
            image_size: (6000, 4000),
            ..Default::default()
        };
        let (json_path, md_path) = report.write_to(dir.path()).expect("写盘");
        assert!(json_path.exists() && md_path.exists());
        let text = std::fs::read_to_string(&json_path).expect("读回来");
        let back: SpikeReport = serde_json::from_str(&text).expect("能解析");
        assert_eq!(back.image_size, (6000, 4000));
        assert_eq!(back.generated_at, "now");
    }

    #[test]
    fn frame_stats_survive_serialization() {
        // 报告是跨进程/跨机器看的：样本没被 serde 丢掉才算数
        let stats = stats(&[1.0, 2.0, 3.0]);
        let text = serde_json::to_string(&stats).expect("序列化");
        let back: FrameStats = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back.count(), 3);
        assert_eq!(back.percentile(0.5), Some(2.0));
    }
}
