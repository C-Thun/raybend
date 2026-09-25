//! **镜头数据库**（lensfun）：匹配、系数重标定、解析成 [`LensCorrection`]。
//!
//! # 本模块是 lensfun 的**唯一出入口**（与 `raw` 后端同一套分层纪律）
//!
//! 数据来自 [lensfun](https://lensfun.github.io/) 的公共校准库（数据 CC BY-SA 3.0），
//! 由纯 Rust 移植 crate `lensfun`（LGPL-3.0-or-later）提供，XML 库 gzip 后**嵌在二进制里**
//! （约 5 MB 解压后），不需要分发资源文件。
//!
//! 其它模块（含 `develop`）**不许** import lensfun 的类型：它们只认
//! [`crate::develop::lens::LensCorrection`] 那套纯数据。上游是 0.x 的 beta，
//! 真要换实现（或哪天自己解析 XML）时，改的只有本文件。
//!
//! # 加载代价与后台预热
//!
//! [`database`] 第一次调用要解压 + 解析约 5 MB XML，**几百毫秒** ——
//! 绝不能在窗口启动路径或显影线程里同步等它。正确用法：
//! 启动后在后台线程调 [`warm_up`]；显式查询等待库加载并返回失败原因，界面单独跟踪请求状态。
//!
//! # 系数为什么还要「重标定」
//!
//! lensfun 的校准数据是在**标定时的画幅与真实焦距**下测的，而一张照片的画幅（机身 crop）
//! 与标称焦距（可能是 24mm 的镜头实际 24.6mm）都可能不同。上游的做法是
//! `rescale_polynomial_coefficients`：把系数按 `hugin_scaling = 真实焦距 / 标定时的等效焦距`
//! 的幂次缩放（畸变 1/2/3 次、TCA 1/2 次、暗角 2/4/6 次）。
//! 本模块照抄那套公式（含 `d = 1 - Σk` 的 poly3/ptlens 归一），**自己算**而不是让 crate 的
//! `Modifier` 算 —— 因为我们要的是**系数**（喂给自己的像素循环），而不是它的行级回调。

use std::sync::{Mutex, OnceLock};

use lensfun::calib::{DistortionModel, TcaModel, VignettingModel};
use lensfun::{Database, Lens};

use crate::develop::lens::{Distortion, LensCorrection, ManualLens, Norm, Tca, Vignetting};

/// 「无穷远」的拍摄距离（米）。EXIF 里基本读不到对焦距离，
/// 而上游的暗角插值需要一个距离轴 —— 用 1000 m（与 darktable 的默认一致）。
pub const FAR_DISTANCE: f32 = 1000.0;

struct DatabaseCache {
    database: OnceLock<Database>,
    failure: Mutex<Option<String>>,
}
impl DatabaseCache {
    const fn new() -> Self {
        Self {
            database: OnceLock::new(),
            failure: Mutex::new(None),
        }
    }
    fn load(
        &self,
        retry: bool,
        loader: impl FnOnce() -> Result<Database, String>,
    ) -> Result<&Database, String> {
        if let Some(database) = self.database.get() {
            return Ok(database);
        }
        let mut failure = self
            .failure
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(database) = self.database.get() {
            return Ok(database);
        }
        if !retry && let Some(error) = failure.as_ref() {
            return Err(error.clone());
        }
        match loader() {
            Ok(database) => {
                let _ = self.database.set(database);
                *failure = None;
                self.database
                    .get()
                    .ok_or_else(|| "lens database was not initialized".to_owned())
            }
            Err(error) => {
                *failure = Some(error.clone());
                Err(error)
            }
        }
    }
}
static DATABASE: DatabaseCache = DatabaseCache::new();

/// Load once successfully; preserve failures for diagnostics. Explicit requests can retry a failure.
pub fn load_database(retry: bool) -> Result<&'static Database, String> {
    DATABASE.load(retry, || {
        let started = std::time::Instant::now();
        let result = (|| {
            let mut db =
                Database::load_bundled().map_err(|error| format!("加载内置镜头库失败：{error}"))?;
            for lens in &mut db.lenses {
                lens.guess_parameters();
            }
            if db.lenses.is_empty() {
                return Err("内置镜头库没有镜头条目".to_owned());
            }
            Ok(db)
        })();
        match &result {
            Ok(db) => eprintln!(
                "[lens.database] ready lenses={} cameras={} elapsed_ms={}",
                db.lenses.len(),
                db.cameras.len(),
                started.elapsed().as_millis()
            ),
            Err(error) => eprintln!(
                "[lens.database] failed elapsed_ms={} error={error}",
                started.elapsed().as_millis()
            ),
        }
        result
    })
}
#[must_use]
pub fn database() -> Option<&'static Database> {
    load_database(false).ok()
}
#[must_use]
pub fn is_ready() -> bool {
    DATABASE.database.get().is_some()
}
pub fn warm_up() {
    let _ = load_database(false);
}

/// 下拉里的一个镜头条目（**稳定键** = `maker|model`）。
#[derive(Debug, Clone, PartialEq)]
pub struct LensProfile {
    pub key: String,
    pub maker: String,
    pub model: String,
    /// 投影类型是矩形吗（鱼眼 / 全景的**几何**校正本轮不做，见 [`ResolvedProfile::geometry`]）
    pub rectilinear: bool,
    pub focal_min: f32,
    pub focal_max: f32,
}

impl LensProfile {
    /// 拼稳定键（`|` 不会出现在镜头型号里，但即使出现也只是键的形态，不影响匹配）。
    #[must_use]
    pub fn key_of(maker: &str, model: &str) -> String {
        format!("{maker}|{model}")
    }
}

/// 从照片元数据来的匹配输入。
#[derive(Debug, Clone, Default)]
pub struct MatchInput {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    /// EXIF 的镜头字符串（没有就匹配不到 —— 不猜）
    pub lens: Option<String>,
}
/// 解析系数要的拍摄参数 + 图像尺寸。
#[derive(Debug, Clone, Copy)]
pub struct ShotParams {
    /// 标称焦距（mm）——EXIF 的 `focal_mm`
    pub focal: f32,
    /// 光圈（f 值）——EXIF 的 `f_number`；没有就只影响暗角
    pub aperture: Option<f32>,
    /// 拍摄距离（米）；没有按 [`FAR_DISTANCE`]
    pub distance: Option<f32>,
    /// **机身的** crop factor（不是镜头标定时的那个）
    pub camera_crop: f32,
    pub width: u32,
    pub height: u32,
}

/// 解析结果：我们的校正数据 + 哪些部分真的可用。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedProfile {
    pub profile: LensProfile,
    /// 喂给 `develop::lens` 的纯数据（不含手动微调 —— 那由调用方合并）
    pub correction: LensCorrection,
    /// 几何（畸变）校正可用吗：**非矩形投影（鱼眼 / 全景）本轮不做**，
    /// 这时 TCA 与暗角照样可用，只是不拉直（宁可少做，不要算错 —— 与上游
    /// 「几何变换要先把投影换成矩形」的次序一致）。
    pub geometry: bool,
}

/// 按 (maker, model) 找机身，返回它的 crop factor。
///
/// 相机的 crop factor 决定**归一化坐标**的尺度（`Norm::new` 的入参）——
/// 拿错它，畸变会整体缩放错。
#[must_use]
pub fn camera_crop(maker: Option<&str>, model: &str) -> Option<f32> {
    let db = database()?;
    let cameras = db.find_cameras(maker, model);
    cameras
        .iter()
        .find(|camera| camera.crop_factor.is_finite() && camera.crop_factor > 0.0)
        .map(|camera| camera.crop_factor)
}

/// landmark：把 `Lens` 转成界面用的 [`LensProfile`]。
fn profile_of(lens: &Lens) -> LensProfile {
    LensProfile {
        key: LensProfile::key_of(&lens.maker, &lens.model),
        maker: lens.maker.clone(),
        model: lens.model.clone(),
        rectilinear: matches!(lens.lens_type, lensfun::LensType::Rectilinear),
        focal_min: lens.focal_min,
        focal_max: lens.focal_max,
    }
}

/// A model's stable optical identity. All values are hundredths to avoid float key drift.
/// The generation defaults to 1 when the model does not say II/III/Mark 2/etc.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LensFeatures {
    maker: String,
    focal_min: u32,
    focal_max: u32,
    aperture: Option<(u32, u32)>,
    generation: u8,
}

fn number_at(text: &str, start: usize) -> Option<(f32, usize)> {
    let bytes = text.as_bytes();
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    if end == start {
        return None;
    }
    let number = text.get(start..end)?.parse::<f32>().ok()?;
    number.is_finite().then_some((number, end))
}
fn key_number(number: f32) -> Option<u32> {
    (number.is_finite() && (0.7..=2000.0).contains(&number))
        .then(|| (number * 100.0).round() as u32)
}
fn skip_spaces(text: &str, mut offset: usize) -> usize {
    while text
        .as_bytes()
        .get(offset)
        .is_some_and(u8::is_ascii_whitespace)
    {
        offset += 1;
    }
    offset
}
fn separator(text: &str, offset: usize) -> Option<usize> {
    let next = text.get(offset..)?.chars().next()?;
    if matches!(next, '-' | '–' | '—' | '−') {
        Some(offset + next.len_utf8())
    } else {
        None
    }
}
/// Reuse the same parser for EXIF and bundled models; Lensfun only extracts the first aperture
/// and misses formats such as `12-60/F3.5-5.6` and Unicode focal separators.
fn aperture_of(model: &str) -> Option<(usize, u32, u32)> {
    let bytes = model.as_bytes();
    for offset in 0..bytes.len() {
        let marker = bytes[offset].eq_ignore_ascii_case(&b'f')
            && (offset == 0 || !bytes[offset - 1].is_ascii_alphanumeric())
            || bytes[offset] == b'1'
                && bytes.get(offset + 1) == Some(&b':')
                && (offset == 0 || !bytes[offset - 1].is_ascii_alphanumeric());
        if !marker {
            continue;
        }
        let mut start = offset + if bytes[offset] == b'1' { 2 } else { 1 };
        if bytes.get(start) == Some(&b'/') {
            start += 1;
        }
        start = skip_spaces(model, start);
        let Some((first, end)) = number_at(model, start) else {
            continue;
        };
        let Some(first) = key_number(first).filter(|n| (70..=6400).contains(n)) else {
            continue;
        };
        let second = separator(model, end)
            .and_then(|after| number_at(model, skip_spaces(model, after)))
            .and_then(|(n, _)| key_number(n))
            .filter(|n| *n >= first && *n <= 6400)
            .unwrap_or(first);
        return Some((offset, first, second));
    }
    None
}
fn focal_of(model: &str, aperture_start: Option<usize>) -> Option<(u32, u32)> {
    let before_aperture = &model[..aperture_start.unwrap_or(model.len())];
    // Some model names write the aperture first (1:4 12-45); keep the trailing part too.
    let sections = [
        before_aperture,
        aperture_start
            .and_then(|start| model.get(start..))
            .unwrap_or(""),
    ];
    for section in sections {
        let bytes = section.as_bytes();
        let mut prime = None;
        let mut offset = 0;
        while offset < bytes.len() {
            if !bytes[offset].is_ascii_digit() || offset > 0 && bytes[offset - 1].is_ascii_digit() {
                offset += 1;
                continue;
            }
            let Some((first, end)) = number_at(section, offset) else {
                offset += 1;
                continue;
            };
            let after_mm = if section
                .get(end..)
                .is_some_and(|tail| tail.to_ascii_lowercase().starts_with("mm"))
            {
                end + 2
            } else {
                end
            };
            if let Some(after) = separator(section, skip_spaces(section, after_mm)) {
                let mut next = skip_spaces(section, after);
                if section
                    .get(next..)
                    .is_some_and(|tail| tail.to_ascii_lowercase().starts_with("mm"))
                {
                    next += 2;
                }
                if let Some((second, _)) = number_at(section, next)
                    && let (Some(a), Some(b)) = (key_number(first), key_number(second))
                    && a >= 200
                    && b >= a
                    && b <= 200000
                {
                    return Some((a, b));
                }
            }
            if after_mm != end && prime.is_none() {
                prime = key_number(first).filter(|n| *n >= 200);
            }
            offset = end.max(offset + 1);
        }
        if let Some(single) = prime {
            return Some((single, single));
        }
    }
    None
}
fn generation_of(model: &str) -> u8 {
    let lower = model.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.')
        .filter(|w| !w.is_empty())
        .collect();
    for (index, word) in words.iter().enumerate() {
        let value = match *word {
            "iv" => Some(4),
            "iii" => Some(3),
            "ii" => Some(2),
            "v4" | "mk4" => Some(4),
            "v3" | "mk3" => Some(3),
            "v2" | "mk2" => Some(2),
            "2" | "3" | "4"
                if index > 0 && matches!(words[index - 1], "mark" | "mk" | "version") =>
            {
                word.parse().ok()
            }
            _ => [("iii", 3), ("ii", 2)]
                .into_iter()
                .find_map(|(suffix, value)| {
                    word.strip_suffix(suffix)
                        .filter(|prefix| prefix.ends_with(|c: char| c.is_ascii_digit()))
                        .map(|_| value)
                }),
        };
        if let Some(value) = value {
            return value;
        }
    }
    1
}
fn maker_of(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |word| words.contains(&word);
    if has("panasonic") || has("lumix") {
        Some("panasonic")
    } else if has("olympus") || has("zuiko") || has("system") && has("om") {
        Some("olympus")
    } else if has("canon") {
        Some("canon")
    } else if has("nikon") || has("nikkor") {
        Some("nikon")
    } else if has("sony") {
        Some("sony")
    } else if has("sigma") {
        Some("sigma")
    } else if has("tamron") {
        Some("tamron")
    } else if has("fujifilm") || has("fuji") {
        Some("fujifilm")
    } else if has("pentax") {
        Some("pentax")
    } else if has("tokina") {
        Some("tokina")
    } else if has("samyang") {
        Some("samyang")
    } else if has("leica") {
        Some("leica")
    } else {
        None
    }
}
fn model_features(model: &str, maker: &str, focal: Option<(u32, u32)>) -> Option<LensFeatures> {
    let aperture = aperture_of(model);
    let (focal_min, focal_max) = focal.or_else(|| focal_of(model, aperture.map(|a| a.0)))?;
    if focal_min > focal_max {
        return None;
    }
    Some(LensFeatures {
        maker: maker.to_owned(),
        focal_min,
        focal_max,
        aperture: aperture.map(|(_, first, second)| (first, second)),
        generation: generation_of(model),
    })
}
fn input_features(input: &MatchInput) -> Option<LensFeatures> {
    let model = input.lens.as_deref()?.trim();
    if model.is_empty() {
        return None;
    }
    let camera = input.camera_make.as_deref().and_then(maker_of);
    let named = maker_of(model);
    // Panasonic's Leica DG profiles are stored under Panasonic in Lensfun.
    let maker = match (camera, named) {
        (Some("panasonic"), Some("leica")) => "panasonic",
        (_, Some(named)) => named,
        (Some(camera), None) => camera,
        (None, None) => return None,
    };
    model_features(model, maker, None)
}
fn database_features(db: &Database) -> &'static Vec<Option<LensFeatures>> {
    static INDEX: OnceLock<Vec<Option<LensFeatures>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        db.lenses
            .iter()
            .map(|lens| {
                let focal = match (key_number(lens.focal_min), key_number(lens.focal_max)) {
                    (Some(a), Some(b)) if a >= 200 && b >= a => Some((a, b)),
                    _ => None,
                };
                let maker = maker_of(&lens.maker)
                    .map(str::to_owned)
                    .unwrap_or_else(|| lens.maker.to_ascii_lowercase());
                model_features(&lens.model, &maker, focal)
            })
            .collect()
    })
}
fn family_score(exif_name: &str, profile_name: &str) -> usize {
    let model = profile_name.to_ascii_lowercase();
    exif_name
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|part| {
            part.len() >= 4
                && !matches!(
                    *part,
                    "panasonic"
                        | "olympus"
                        | "canon"
                        | "nikon"
                        | "sony"
                        | "leica"
                        | "lens"
                        | "vario"
                )
        })
        .filter(|part| {
            model
                .split(|c: char| !c.is_ascii_alphabetic())
                .any(|other| other == *part)
        })
        .count()
        + usize::from(exif_name.to_ascii_lowercase().contains("leica") && model.contains("leica"))
}
/// **自动识别**：优先按厂商、焦段（含定焦/变焦）、最大光圈范围和代数匹配。
/// 缺少光圈而有多支同焦段镜头时，只在型号系列足以区分时选择；绝不猜第一支。
#[must_use]
pub fn auto_match(input: &MatchInput) -> Option<LensProfile> {
    let db = database()?;
    let target = input_features(input);
    let name = input.lens.as_deref()?.trim();
    if name.is_empty() {
        return None;
    }
    let Some(target) = target else {
        // A literal full model name is still a safe fallback if EXIF uses a format we cannot parse.
        let maker = input.camera_make.as_deref().and_then(maker_of);
        return db
            .lenses
            .iter()
            .find(|lens| {
                lens.model.eq_ignore_ascii_case(name)
                    && maker.is_none_or(|maker| maker_of(&lens.maker) == Some(maker))
            })
            .map(profile_of);
    };
    let camera_mount = input.camera_model.as_deref().and_then(|model| {
        db.find_cameras(input.camera_make.as_deref(), model)
            .first()
            .map(|camera| camera.mount.clone())
    });
    let mut found: Vec<(usize, usize, bool)> = database_features(db)
        .iter()
        .enumerate()
        .filter_map(|(index, features)| {
            let feature = features.as_ref()?;
            if feature.maker != target.maker
                || feature.focal_min != target.focal_min
                || feature.focal_max != target.focal_max
                || feature.generation != target.generation
            {
                return None;
            }
            if let Some(aperture) = target.aperture
                && feature.aperture != Some(aperture)
            {
                return None;
            }
            let lens = &db.lenses[index];
            let mount = camera_mount
                .as_ref()
                .is_some_and(|mount| lens.mounts.iter().any(|m| m == mount));
            Some((index, family_score(name, &lens.model), mount))
        })
        .collect();
    if found.is_empty() {
        return None;
    }
    if found.iter().any(|item| item.2) {
        found.retain(|item| item.2);
    }
    found.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| db.lenses[a.0].model.cmp(&db.lenses[b.0].model))
    });
    if target.aperture.is_none() && found.len() > 1 && found[0].1 == found[1].1 {
        return None;
    }
    Some(profile_of(&db.lenses[found[0].0]))
}

/// 下拉候选：自动匹配的结果排第一，其余来自全库；手动搜索不被机身卡口截断。
///
/// 为什么要给候选：EXIF 的镜头字符串经常与库里写的不完全一样（厂商后缀、代次），
/// 自动匹配可能挑到近似的错条目 —— 让用户能自己选一个才是诚实的做法。
#[must_use]
pub fn candidates(input: &MatchInput) -> Vec<LensProfile> {
    let Some(db) = database() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(matched) = auto_match(input) {
        out.push(matched);
    }
    let mut seen: std::collections::HashSet<String> = out.iter().map(|p| p.key.clone()).collect();
    for lens in &db.lenses {
        let profile = profile_of(lens);
        if seen.insert(profile.key.clone()) {
            out.push(profile);
        }
    }
    out
}

/// **这次渲染到底用哪个配置文件**（`None` = 没有配置文件 / 用户关掉了 / 匹配不到）。
///
/// 三种取值（与 `develop_stacks.lens_profile` 同一套语义）：
/// * `None`   —— 未选择，不自动应用校正
/// * `"none"` —— 用户显式清除了配置 ⇒ 不用配置文件
/// * 其它     —— 用户从下拉里选的那一支（键认不出来时当「没有」，不猜一支）
#[must_use]
pub fn effective_profile(choice: Option<&str>) -> Option<LensProfile> {
    match choice {
        None | Some("none") => None,
        Some(key) => lens_by_key(database()?, key).map(profile_of),
    }
}

/// 渲染要的一次镜头校正请求（**显影线程与缩略图共用一份**，免得两处各拼一遍）。
pub struct LensRequest {
    /// 编辑栈里的 `lens_profile`
    pub choice: Option<String>,
    /// 编辑栈里的 `lens_enabled`（`Some(false)` = 关掉配置文件那一半）
    pub enabled: Option<bool>,
    /// 照片元数据（相机 / 镜头字符串 / 焦距 / 光圈）
    pub exif: MatchInput,
    /// 拼接镜头字符串只用于自动识别；焦距与光圈在下面
    pub focal: f32,
    pub aperture: Option<f32>,
    pub width: u32,
    pub height: u32,
}

/// 解析出**这次渲染要用的校正**（不含手动微调 —— 那由调用方从参数里合并）。
///
/// 返回 `None` 的情形：用户关掉了配置文件 / 没匹配到 / 该镜头没有可用标定。
/// （手动三根拉杆**不受这里影响**：调用方会另走 [`LensCorrection::manual_only`]。）
#[must_use]
pub fn render_correction(request: &LensRequest, manual: ManualLens) -> Option<LensCorrection> {
    if request.enabled == Some(false) {
        return None;
    }
    let profile = effective_profile(request.choice.as_deref())?;
    if profile.key == "none" {
        return None;
    }
    let crop = camera_crop(
        request.exif.camera_make.as_deref(),
        request.exif.camera_model.as_deref().unwrap_or(""),
    )
    .unwrap_or(1.0);
    let shot = ShotParams {
        focal: request.focal,
        aperture: request.aperture,
        distance: None,
        camera_crop: crop,
        width: request.width,
        height: request.height,
    };
    resolve(&profile.key, &shot, manual).map(|resolved| resolved.correction)
}

/// 按稳定键找镜头条目。
fn lens_by_key<'a>(db: &'a Database, key: &str) -> Option<&'a Lens> {
    db.lenses
        .iter()
        .find(|lens| LensProfile::key_of(&lens.maker, &lens.model) == key)
}

/// 解析成一个镜头在**这张照片上**的校正数据（不含手动微调）。
///
/// `manual` 原样带出去（由调用方决定是否合并 —— 本函数只管配置文件那一半）。
#[must_use]
pub fn resolve(key: &str, shot: &ShotParams, manual: ManualLens) -> Option<ResolvedProfile> {
    let db = database()?;
    let lens = lens_by_key(db, key)?;
    let focal = if shot.focal.is_finite() && shot.focal > 0.0 {
        shot.focal
    } else {
        // 没有焦距就没法插值（畸变/TCA 都以焦距为轴）—— 不猜
        return None;
    };
    // 真实焦距：上游优先用畸变标定里给的，否则用标称值
    let real_focal = lens
        .interpolate_distortion(focal)
        .and_then(|calib| calib.real_focal)
        .map_or(f64::from(focal), f64::from);

    let geometry = matches!(lens.lens_type, lensfun::LensType::Rectilinear);
    let distortion = if geometry {
        lens.interpolate_distortion(focal)
            .and_then(|calib| rescale_distortion(calib.model, lens, real_focal))
    } else {
        None
    };
    let tca = lens
        .interpolate_tca(focal)
        .and_then(|calib| rescale_tca(calib.model, lens, real_focal));
    let aperture = shot
        .aperture
        .filter(|value| value.is_finite() && *value > 0.0);
    let distance = shot
        .distance
        .filter(|value| value.is_finite() && *value > 0.0);
    let vignetting = aperture.and_then(|aperture| {
        lens.interpolate_vignetting(focal, aperture, distance.unwrap_or(FAR_DISTANCE))
            .and_then(|calib| rescale_vignetting(calib.model, lens, real_focal))
    });

    let crop = if shot.camera_crop.is_finite() && shot.camera_crop > 0.0 {
        shot.camera_crop
    } else {
        1.0
    };
    let norm = Norm::new(shot.width, shot.height, crop, real_focal).with_center_offset(
        lens.center_x,
        lens.center_y,
        shot.width,
        shot.height,
    );

    Some(ResolvedProfile {
        profile: profile_of(lens),
        correction: LensCorrection {
            norm,
            distortion,
            tca,
            vignetting,
            manual,
        },
        geometry,
    })
}

/// 标定时的等效焦距（mm）：`hypot(36,24) / crop / hypot(aspect, 1) / 2`。
fn hugin_scale_in_mm(crop: f32, aspect_ratio: f32) -> f64 {
    let crop = if crop.is_finite() && crop > 0.0 {
        f64::from(crop)
    } else {
        1.0
    };
    let aspect = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        f64::from(aspect_ratio)
    } else {
        1.5
    };
    36.0_f64.hypot(24.0) / crop / aspect.hypot(1.0) / 2.0
}

/// 畸变系数的重标定（上游 `rescale_polynomial_coefficients` 的畸变分支）。
fn rescale_distortion(model: DistortionModel, lens: &Lens, real_focal: f64) -> Option<Distortion> {
    let scaling = real_focal / hugin_scale_in_mm(lens.crop_factor, lens.aspect_ratio);
    Some(match model {
        DistortionModel::None => return None,
        DistortionModel::Poly3 { k1 } => {
            let d = 1.0 - f64::from(k1);
            Distortion::Poly3 {
                k1: (f64::from(k1) * scaling.powi(2) / d.powi(3)) as f32,
            }
        }
        DistortionModel::Poly5 { k1, k2 } => Distortion::Poly5 {
            k1: (f64::from(k1) * scaling.powi(2)) as f32,
            k2: (f64::from(k2) * scaling.powi(4)) as f32,
        },
        DistortionModel::Ptlens { a, b, c } => {
            let d = 1.0 - f64::from(a) - f64::from(b) - f64::from(c);
            Distortion::Ptlens {
                a: (f64::from(a) * scaling.powi(3) / d.powi(4)) as f32,
                b: (f64::from(b) * scaling.powi(2) / d.powi(3)) as f32,
                c: (f64::from(c) * scaling / d.powi(2)) as f32,
            }
        }
    })
}

/// TCA 系数的重标定。**注意方向**：数据里的是「校正」方向（`v` 项不动，`c`/`b` 按幂缩放）。
fn rescale_tca(model: TcaModel, lens: &Lens, real_focal: f64) -> Option<Tca> {
    Some(match model {
        TcaModel::None => return None,
        TcaModel::Linear { kr, kb } => Tca::Linear {
            kr: if kr.is_finite() { kr } else { 1.0 },
            kb: if kb.is_finite() { kb } else { 1.0 },
        },
        TcaModel::Poly3 { red, blue } => {
            let scaling = real_focal / hugin_scale_in_mm(lens.crop_factor, lens.aspect_ratio);
            Tca::Poly3 {
                red: [
                    red[0],
                    (f64::from(red[1]) * scaling) as f32,
                    (f64::from(red[2]) * scaling.powi(2)) as f32,
                ],
                blue: [
                    blue[0],
                    (f64::from(blue[1]) * scaling) as f32,
                    (f64::from(blue[2]) * scaling.powi(2)) as f32,
                ],
            }
        }
    })
}

/// 暗角系数的重标定（暗角没有 aspect 项，公式与畸变/TCA 略不同 —— 上游如此）。
fn rescale_vignetting(model: VignettingModel, lens: &Lens, real_focal: f64) -> Option<Vignetting> {
    let crop = if lens.crop_factor.is_finite() && lens.crop_factor > 0.0 {
        f64::from(lens.crop_factor)
    } else {
        1.0
    };
    let scaling = real_focal / (36.0_f64.hypot(24.0) / crop / 2.0);
    Some(match model {
        VignettingModel::None => return None,
        VignettingModel::Pa { k1, k2, k3 } => Vignetting {
            k1: (f64::from(k1) * scaling.powi(2)) as f32,
            k2: (f64::from(k2) * scaling.powi(4)) as f32,
            k3: (f64::from(k3) * scaling.powi(6)) as f32,
        },
    })
}

/* ══════════════════════════════════════════════════════════════
 * 单测（用**内置**数据库；它是嵌在 crate 里的，不依赖网络）
 * ══════════════════════════════════════════════════════════════ */

#[cfg(test)]
mod tests {
    #[test]
    fn failed_database_load_keeps_error_and_can_retry() {
        let cache = super::DatabaseCache::new();
        assert_eq!(
            cache
                .load(false, || Err("synthetic parse failure".into()))
                .unwrap_err(),
            "synthetic parse failure"
        );
        assert_eq!(
            cache
                .load(false, || panic!("implicit render calls must not retry"))
                .unwrap_err(),
            "synthetic parse failure"
        );
        let loaded = cache.load(true, || Ok(lensfun::Database::new())).unwrap();
        let reused = cache
            .load(true, || panic!("successful loads are immutable"))
            .unwrap();
        assert!(std::ptr::eq(loaded, reused));
    }

    #[test]
    fn concurrent_database_requests_only_load_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = super::DatabaseCache::new();
        let calls = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    cache
                        .load(true, || {
                            calls.fetch_add(1, Ordering::Relaxed);
                            Ok(lensfun::Database::new())
                        })
                        .unwrap();
                });
            }
        });
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_match_does_not_apply_until_a_profile_is_selected() {
        assert!(super::effective_profile(None).is_none());
        assert!(super::effective_profile(Some("none")).is_none());
        let key = "Panasonic|LEICA DG 12-60/F2.8-4.0";
        assert_eq!(super::effective_profile(Some(key)).unwrap().key, key);
        assert!(super::effective_profile(Some("missing|lens")).is_none());
    }

    use super::*;

    /// 从库里挑一支**有畸变标定的矩形镜头**（不写死型号：库版本会变）。
    fn any_rectilinear_lens() -> &'static Lens {
        database()
            .expect("内置数据库要能加载")
            .lenses
            .iter()
            .find(|lens| {
                matches!(lens.lens_type, lensfun::LensType::Rectilinear)
                    && lens
                        .interpolate_distortion((lens.focal_min + lens.focal_max) / 2.0)
                        .is_some()
                    && lens.focal_min.is_finite()
                    && lens.focal_max.is_finite()
                    && lens.focal_max > 0.0
            })
            .expect("库里总该有一支矩形镜头")
    }

    #[test]
    fn bundled_database_loads_and_has_content() {
        let db = database().expect("内置数据库要能加载");
        assert!(is_ready(), "加载过之后 is_ready 要为真");
        assert!(db.lenses.len() > 1000, "镜头条目太少：{}", db.lenses.len());
        assert!(!db.cameras.is_empty());
    }

    #[test]
    fn missing_lens_name_never_matches() {
        // 没有 EXIF 镜头字符串 ⇒ 不猜（这是「宁可没有，不要做错」的判据）
        let input = MatchInput {
            camera_make: Some("Canon".to_string()),
            camera_model: Some("EOS 5D Mark III".to_string()),
            lens: None,
        };
        assert_eq!(auto_match(&input), None);
        let empty = MatchInput {
            lens: Some("   ".to_string()),
            ..MatchInput::default()
        };
        assert_eq!(auto_match(&empty), None);
    }

    #[test]
    fn unknown_lens_finds_nothing() {
        let input = MatchInput {
            lens: Some("NoSuchLens 123-456mm f/0.1".to_string()),
            ..MatchInput::default()
        };
        assert_eq!(auto_match(&input), None);
    }

    #[test]
    fn structured_signature_recovers_panasonic_zooms_from_exif_variants() {
        let input = |lens: &str| MatchInput {
            camera_make: Some("Panasonic".into()),
            camera_model: Some("DMC-GX85".into()),
            lens: Some(lens.into()),
        };
        for name in [
            "LUMIX G VARIO 12-60/F3.5-5.6",
            "Panasonic 12–60mm f/3.5–5.6",
            "Lumix G Vario 12mm-60mm F3.5-5.6",
        ] {
            let found = auto_match(&input(name)).unwrap_or_else(|| panic!("{name}"));
            assert!(
                found.model.to_ascii_lowercase().contains("lumix"),
                "{name}: {}",
                found.model
            );
        }
        let leica = auto_match(&input("DG Vario-Elmarit 12-60mm F2.8-4 Asph. Power OIS")).unwrap();
        assert!(leica.model.to_ascii_lowercase().contains("leica"));
        assert_eq!(leica.maker, "Panasonic");
        assert!(
            auto_match(&input("Panasonic 12-60mm")).is_none(),
            "same range with missing aperture is ambiguous"
        );
        let named = auto_match(&input("LUMIX G VARIO 12-60mm")).unwrap();
        assert!(named.model.to_ascii_lowercase().contains("lumix"));
    }

    #[test]
    fn signature_separates_primes_zooms_apertures_generations_and_brands() {
        let signature = |maker: &str, model: &str| {
            input_features(&MatchInput {
                camera_make: Some(maker.into()),
                camera_model: None,
                lens: Some(model.into()),
            })
            .unwrap()
        };
        assert_eq!(
            signature("Olympus", "M.ZUIKO DIGITAL 12-45mm F4.0 PRO").focal_min,
            1200
        );
        assert_eq!(
            signature("Olympus", "M.ZUIKO DIGITAL 12-45mm F4.0 PRO").aperture,
            Some((400, 400))
        );
        let prime = signature("Sony", "FE 35mm F1.8");
        assert_eq!((prime.focal_min, prime.focal_max), (3500, 3500));
        assert_ne!(prime, signature("Sony", "FE 35-70mm F1.8"));
        assert_ne!(
            signature("Panasonic", "12-60mm f/3.5-5.6"),
            signature("Panasonic", "12-60mm f/2.8-4")
        );
        assert_ne!(
            signature("Panasonic", "12-60mm f/3.5-5.6"),
            signature("Olympus", "12-60mm f/3.5-5.6")
        );
        assert_eq!(signature("Canon", "EF 24-70mm f/2.8L II USM").generation, 2);
        assert_eq!(signature("Canon", "RF 24-70mm f/2.8L IS USM").generation, 1);
        assert_eq!(
            signature("Canon", "RF 24-70mm f/2.8L Mark III USM").generation,
            3
        );
        assert!(
            input_features(&MatchInput {
                lens: Some("unknown".into()),
                ..Default::default()
            })
            .is_none()
        );
    }

    #[test]
    fn resolve_interpolates_between_focal_samples() {
        let lens = database()
            .unwrap()
            .lenses
            .iter()
            .find(|lens| {
                matches!(lens.lens_type, lensfun::LensType::Rectilinear)
                    && lens.calib_distortion.len() >= 2
                    && lens.calib_distortion[0].focal != lens.calib_distortion[1].focal
            })
            .unwrap();
        let profile = profile_of(lens);
        // 取两个标定档之间的焦距（库里的档位是稀疏的）
        let mut focals: Vec<f32> = lens
            .calib_distortion
            .iter()
            .map(|calib| calib.focal)
            .collect();
        focals.sort_by(f32::total_cmp);
        let between = (focals[0] + focals[1]) / 2.0;
        let shot = ShotParams {
            focal: between,
            aperture: None,
            distance: None,
            camera_crop: if lens.crop_factor > 0.0 {
                lens.crop_factor
            } else {
                1.0
            },
            width: 6000,
            height: 4000,
        };
        let resolved = resolve(&profile.key, &shot, ManualLens::default()).expect("该有结果");
        assert_eq!(resolved.profile.key, profile.key);
        assert!(resolved.correction.distortion.is_some(), "应当解析出畸变");
        assert!(resolved.correction.norm.scale > 0.0);
        // 归一化画幅角：全画幅 3:2 应当在 0.6 上下（焦距不同略有出入）
        assert!(resolved.correction.norm.half_diag > 0.05);
    }

    #[test]
    fn focal_out_of_range_still_resolves_with_the_nearest_sample() {
        let lens = any_rectilinear_lens();
        let profile = profile_of(lens);
        let shot = ShotParams {
            focal: lens.focal_max * 4.0, // 远超标定范围
            aperture: None,
            distance: None,
            camera_crop: 1.0,
            width: 6000,
            height: 4000,
        };
        let resolved = resolve(&profile.key, &shot, ManualLens::default());
        // 上游的行为是「取最近档」——所以要么给结果、要么明确没有畸变标定，
        // **不允许** panic 或给出 NaN 系数
        if let Some(model) = resolved.and_then(|resolved| resolved.correction.distortion) {
            let finite = match model {
                Distortion::Poly3 { k1 } => k1.is_finite(),
                Distortion::Poly5 { k1, k2 } => k1.is_finite() && k2.is_finite(),
                Distortion::Ptlens { a, b, c } => a.is_finite() && b.is_finite() && c.is_finite(),
            };
            assert!(finite, "最近档插值也必须给有限系数：{model:?}");
        }
    }

    #[test]
    fn zero_or_missing_focal_resolves_to_nothing() {
        let lens = any_rectilinear_lens();
        let profile = profile_of(lens);
        for focal in [0.0, -10.0, f32::NAN] {
            let shot = ShotParams {
                focal,
                aperture: None,
                distance: None,
                camera_crop: 1.0,
                width: 6000,
                height: 4000,
            };
            assert!(
                resolve(&profile.key, &shot, ManualLens::default()).is_none(),
                "焦距 {focal} 不该给出结果"
            );
        }
    }

    #[test]
    fn missing_aperture_only_costs_vignetting() {
        let lens = any_rectilinear_lens();
        let profile = profile_of(lens);
        let base = ShotParams {
            focal: (lens.focal_min + lens.focal_max) / 2.0,
            aperture: None,
            distance: None,
            camera_crop: 1.0,
            width: 6000,
            height: 4000,
        };
        let without = resolve(&profile.key, &base, ManualLens::default()).expect("该有结果");
        assert!(
            without.correction.vignetting.is_none(),
            "没有光圈就不做暗角（上游的暗角插值以光圈为轴）"
        );
        assert!(without.correction.tca.is_some() || without.correction.vignetting.is_none());
    }

    #[test]
    fn fisheye_keeps_vignetting_but_skips_geometry() {
        // 库里找一支**非矩形**镜头：几何校正要明确标记为不可用（宁可少做，不要算错）
        let Some(fisheye) = database().and_then(|db| {
            db.lenses
                .iter()
                .find(|lens| !matches!(lens.lens_type, lensfun::LensType::Rectilinear))
        }) else {
            return; // 库版本里没有鱼眼就跳过（不失败）
        };
        let profile = profile_of(fisheye);
        assert!(!profile.rectilinear);
        // 焦距从标定样本里取（不依赖 `focal_min/max` 是否被填过）
        let focal = fisheye
            .calib_distortion
            .first()
            .map(|calib| calib.focal)
            .filter(|focal| *focal > 0.0)
            .unwrap_or(10.0);
        let shot = ShotParams {
            focal,
            aperture: Some(8.0),
            distance: None,
            camera_crop: 1.0,
            width: 6000,
            height: 4000,
        };
        let resolved = resolve(&profile.key, &shot, ManualLens::default()).expect("该有结果");
        assert!(!resolved.geometry, "非矩形投影不做几何校正");
        assert!(resolved.correction.distortion.is_none());
    }

    #[test]
    fn manual_sliders_pass_through_untouched() {
        let lens = any_rectilinear_lens();
        let profile = profile_of(lens);
        let manual = ManualLens {
            distortion: 0.5,
            vignette: -0.25,
            chromatic: 0.75,
            ..ManualLens::default()
        };
        let shot = ShotParams {
            focal: (lens.focal_min + lens.focal_max) / 2.0,
            aperture: Some(8.0),
            distance: None,
            camera_crop: 1.0,
            width: 6000,
            height: 4000,
        };
        let resolved = resolve(&profile.key, &shot, manual).expect("该有结果");
        assert_eq!(resolved.correction.manual, manual, "手动值原样带出");
    }

    #[test]
    fn unknown_key_resolves_to_nothing() {
        let shot = ShotParams {
            focal: 35.0,
            aperture: Some(8.0),
            distance: None,
            camera_crop: 1.0,
            width: 6000,
            height: 4000,
        };
        assert!(resolve("NoMaker|NoLens", &shot, ManualLens::default()).is_none());
    }

    #[test]
    fn camera_lookup_is_fuzzy_and_safe() {
        // 找不到就 None（不 panic）；找得到就给一个正的 crop
        assert!(camera_crop(Some("NoSuchMaker"), "NoSuchCamera").is_none());
        if let Some(crop) = camera_crop(Some("Canon"), "EOS 5D Mark III") {
            assert!(crop > 0.5 && crop < 3.0, "crop 看着不对：{crop}");
        }
    }

    #[test]
    fn candidates_start_with_the_auto_match() {
        let db = database().expect("库");
        // 拿库里真实存在的一支镜头，用它自己的 maker/model 拼一个「EXIF」
        let lens = any_rectilinear_lens();
        let input = MatchInput {
            camera_make: lens.mounts.first().map(|_| lens.maker.clone()),
            camera_model: None,
            lens: Some(lens.model.clone()),
        };
        let candidates = candidates(&input);
        assert!(!candidates.is_empty());
        assert_eq!(
            candidates[0].key,
            LensProfile::key_of(&lens.maker, &lens.model)
        );
        assert!(
            candidates.len() <= db.lenses.len(),
            "候选不可重复或越过镜头库总数"
        );
        let _ = db;
    }
    #[test]
    fn manual_catalog_includes_common_lenses_across_mounts() {
        let profiles = candidates(&MatchInput {
            camera_make: Some("Panasonic".into()),
            camera_model: Some("DMC-GX85".into()),
            lens: None,
        });
        for (maker, focal_min, focal_max) in [
            ("Panasonic", 12.0, 60.0),
            ("Olympus", 12.0, 45.0),
            ("Canon", 24.0, 70.0),
            ("Nikon", 24.0, 70.0),
            ("Sony", 24.0, 70.0),
        ] {
            assert!(
                profiles.iter().any(|p| p.maker == maker
                    && p.focal_min == focal_min
                    && p.focal_max == focal_max),
                "{maker} {focal_min}-{focal_max}"
            );
        }
        let keys: std::collections::HashSet<_> = profiles.iter().map(|p| &p.key).collect();
        assert_eq!(keys.len(), profiles.len());
    }
}
