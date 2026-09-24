//! **镜头匹配的 IPC**（M3-W4）：把照片元数据 + lensfun 库变成界面要的下拉与自动识别，并把
//! 「用哪个配置文件」解析成渲染要的校正数据。
//!
//! # 为什么单独一个文件
//!
//! 这里只有**只读查询**（不改任何东西），与 `develop.rs` 那些落库命令不是一类东西；
//! 而 `editor.rs` 已经够大了。
//!
//! # 为什么要读 catalog
//!
//! 镜头匹配要的是**这张照片的拍摄参数**（机身、镜头字符串、焦距、光圈）——
//! 它们在 `assets` 表里（导入时抽的 EXIF）。前端不必把这几项再传一遍
//! （那会让「同一份事实两处传」成为常态，迟早对不上）。

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use raybend::develop::lens::ManualLens;
use raybend::lens::{self, LensProfile, LensRequest, MatchInput};

use crate::browse::BrowseState;

/// 一个镜头条目（下拉的一行）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensProfileDto {
    pub key: String,
    pub maker: String,
    pub model: String,
    pub rectilinear: bool,
}

impl From<LensProfile> for LensProfileDto {
    fn from(profile: LensProfile) -> Self {
        Self {
            key: profile.key,
            maker: profile.maker,
            model: profile.model,
            rectilinear: profile.rectilinear,
        }
    }
}

/// 这张照片的镜头匹配状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensMatchDto {
    /// 库里就绪了吗（`false` ⇒ 界面显示「加载中」，不是「没匹配到」）
    pub ready: bool,
    /// 自动识别到的配置文件（`null` = 没匹配到 —— **不猜**）
    pub detected: Option<LensProfileDto>,
    /// EXIF 里的镜头字符串（拿它解释「为什么没匹配到」）
    pub lens_name: Option<String>,
    /// 下拉候选（自动匹配的排第一）
    pub candidates: Vec<LensProfileDto>,
}

/// 这张照片的拍摄参数（从 catalog 读；字段都可空 —— 缺什么是常态）。
#[derive(Debug, Clone, Default)]
pub struct ShotInput {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub f_number: Option<f64>,
    /// 像素尺寸（归一化坐标只跟**宽高比**有关，但自动缩放与手动拉杆的归一化要用它）
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl ShotInput {
    /// 镜头匹配的输入。
    fn match_input(&self) -> MatchInput {
        MatchInput {
            camera_make: self.camera_make.clone(),
            camera_model: self.camera_model.clone(),
            lens: self.lens.clone(),
        }
    }
}

/// 读一张照片的拍摄参数（**只读、单点查询**）。
///
/// # Errors
/// 库离线 / 查不到这张资产时的描述。
pub fn shot_input<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    asset_id: i64,
) -> Result<ShotInput, String> {
    let state = app.state::<BrowseState>();
    state.with_catalog(app, repository_id, move |db| {
        let shot = db
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT camera_make, camera_model, lens, focal_mm, f_number, width, height \
                     FROM assets WHERE id = ?1",
                    [asset_id],
                    |row| {
                        Ok(ShotInput {
                            camera_make: row.get(0)?,
                            camera_model: row.get(1)?,
                            lens: row.get(2)?,
                            focal_mm: row.get(3)?,
                            f_number: row.get(4)?,
                            width: row.get::<_, Option<i64>>(5)?.and_then(|v| u32::try_from(v).ok()),
                            height: row.get::<_, Option<i64>>(6)?.and_then(|v| u32::try_from(v).ok()),
                        })
                    },
                )?)
            })
            .map_err(|e| format!("读拍摄参数失败：{e}"))?;
        Ok(shot)
    })
}

/// **镜头匹配状态**（进编辑时问一次）：自动识别 + 下拉候选。
///
/// 库没就绪时返回 `ready = false`（界面据此显示「配置文件库加载中…」），
/// 而不是把「还没加载」说成「没匹配到」。
///
/// # Errors
/// 读 catalog 失败。
#[tauri::command]
pub async fn lens_match<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<LensMatchDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let shot = shot_input(&handle, &repository_id, asset_id)?;
        let ready = lens::is_ready();
        let input = shot.match_input();
        let detected = lens::auto_match(&input).map(LensProfileDto::from);
        let candidates = lens::candidates(&input)
            .into_iter()
            .map(LensProfileDto::from)
            .collect();
        Ok(LensMatchDto {
            ready,
            detected,
            lens_name: shot.lens.clone(),
            candidates,
        })
    })
    .await
}

/// **后台预热镜头库**（窗口起来了再加载，别挡启动）。
pub fn warm_up() {
    std::thread::spawn(|| {
        let started = std::time::Instant::now();
        lens::warm_up();
        eprintln!(
            "[lens] 镜头库就绪（{} 支镜头，{:.0} ms）",
            lens::database().map_or(0, |db| db.lenses.len()),
            started.elapsed().as_secs_f64() * 1000.0
        );
    });
}

/// 解析出**这次渲染要用的镜头校正**（`None` = 不用配置文件）。
///
/// 两个调用方共用：`editor.rs`（显影线程的参数任务）与 `thumbs.rs`（大图缓存那条路）。
/// 手动三根拉杆不在这里 —— 它们从显影参数里合并（那样拖动时才是实时的）。
pub fn render_correction<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    asset_id: i64,
    choice: Option<&str>,
    enabled: Option<bool>,
) -> Option<raybend::develop::lens::LensCorrection> {
    let shot = shot_input(app, repository_id, asset_id).ok()?;
    let request = LensRequest {
        choice: choice.map(str::to_string),
        enabled,
        exif: shot.match_input(),
        focal: shot.focal_mm.map_or(0.0, |value| value as f32),
        aperture: shot.f_number.map(|value| value as f32),
        // 尺寸缺了就按 3:2 兜底（归一化坐标只跟宽高比有关）——**不阻止校正**，
        // 因为「没有尺寸」通常只是元数据没抽到，不该因此丢掉整个镜头校正。
        width: shot.width.unwrap_or(6000),
        height: shot.height.unwrap_or(4000),
    };
    lens::render_correction(&request, ManualLens::default())
}
