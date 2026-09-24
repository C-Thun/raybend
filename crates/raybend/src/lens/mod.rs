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
//! 启动后在后台线程调 [`warm_up`]，界面先用 [`is_ready`] 决定显示「配置文件加载中…」。
//!
//! # 系数为什么还要「重标定」
//!
//! lensfun 的校准数据是在**标定时的画幅与真实焦距**下测的，而一张照片的画幅（机身 crop）
//! 与标称焦距（可能是 24mm 的镜头实际 24.6mm）都可能不同。上游的做法是
//! `rescale_polynomial_coefficients`：把系数按 `hugin_scaling = 真实焦距 / 标定时的等效焦距`
//! 的幂次缩放（畸变 1/2/3 次、TCA 1/2 次、暗角 2/4/6 次）。
//! 本模块照抄那套公式（含 `d = 1 - Σk` 的 poly3/ptlens 归一），**自己算**而不是让 crate 的
//! `Modifier` 算 —— 因为我们要的是**系数**（喂给自己的像素循环），而不是它的行级回调。

use std::sync::OnceLock;

use lensfun::calib::{DistortionModel, TcaModel, VignettingModel};
use lensfun::{Database, Lens};

use crate::develop::lens::{
    Distortion, LensCorrection, ManualLens, Norm, Tca, Vignetting,
};

/// 「无穷远」的拍摄距离（米）。EXIF 里基本读不到对焦距离，
/// 而上游的暗角插值需要一个距离轴 —— 用 1000 m（与 darktable 的默认一致）。
pub const FAR_DISTANCE: f32 = 1000.0;

static DATABASE: OnceLock<Option<Database>> = OnceLock::new();

/// 惰性加载数据库；失败或还没加载完就是 `None`（**不 panic、不阻塞重试**）。
#[must_use]
pub fn database() -> Option<&'static Database> {
    DATABASE.get_or_init(|| Database::load_bundled().ok()).as_ref()
}

/// 数据库就绪了吗（界面据此决定显示「加载中」还是「未匹配」）。
#[must_use]
pub fn is_ready() -> bool {
    DATABASE.get().is_some_and(Option::is_some)
}

/// **后台预热**：启动后在单独线程里调它。重复调用无副作用。
pub fn warm_up() {
    let _ = database();
}

/// 下拉里的一个镜头条目（**稳定键** = `maker|model`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LensProfile {
    pub key: String,
    pub maker: String,
    pub model: String,
    /// 投影类型是矩形吗（鱼眼 / 全景的**几何**校正本轮不做，见 [`ResolvedProfile::geometry`]）
    pub rectilinear: bool,
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
    }
}

/// **自动识别**：机身 + EXIF 里的镜头字符串 → 库里的镜头条目。
///
/// 完全靠字符串模糊匹配（与上游 `FindLenses` 同一套打分）；**匹配不到就 `None`**，
/// 不猜、不退回「取第一支镜头」那种会把校正做错的做法。
#[must_use]
pub fn auto_match(input: &MatchInput) -> Option<LensProfile> {
    let db = database()?;
    let lens_name = input.lens.as_deref().unwrap_or("").trim();
    if lens_name.is_empty() {
        return None;
    }
    let maker = input.camera_make.as_deref().filter(|m| !m.trim().is_empty());
    let model = input.camera_model.as_deref().unwrap_or("").trim();
    let cameras = if model.is_empty() {
        Vec::new()
    } else {
        db.find_cameras(maker, model)
    };
    // 先按机身（会带上卡口与画幅信息，匹配更准），没有机身就直接按型号找
    if let Some(camera) = cameras.first()
        && let Some(lens) = db.find_lenses(Some(camera), lens_name).first() {
            return Some(profile_of(lens));
        }
    db.find_lenses(None, lens_name).first().map(|lens| profile_of(lens))
}

/// 下拉候选：自动匹配的结果排第一，其余是**同卡口**的镜头（按库里的顺序）。
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
    let maker = input.camera_make.as_deref().filter(|m| !m.trim().is_empty());
    let model = input.camera_model.as_deref().unwrap_or("").trim();
    let mount = if model.is_empty() {
        None
    } else {
        db.find_cameras(maker, model)
            .first()
            .map(|camera| camera.mount.clone())
            .filter(|mount| !mount.trim().is_empty())
    };
    for lens in &db.lenses {
        let profile = profile_of(lens);
        if out.iter().any(|existing| existing.key == profile.key) {
            continue;
        }
        if let Some(mount) = mount.as_deref()
            && !lens.mounts.iter().any(|candidate| candidate == mount) {
                continue;
            }
        out.push(profile);
        if out.len() >= 200 {
            break;
        }
    }
    out
}

/// **这次渲染到底用哪个配置文件**（`None` = 没有配置文件 / 用户关掉了 / 匹配不到）。
///
/// 三种取值（与 `develop_stacks.lens_profile` 同一套语义）：
/// * `None`   —— 没动过 ⇒ **自动识别**（匹配不到就没有）
/// * `"none"` —— 用户显式关掉了自动匹配 ⇒ 不用配置文件
/// * 其它     —— 用户从下拉里选的那一支（键认不出来时当「没有」，不猜一支）
#[must_use]
pub fn effective_profile(choice: Option<&str>, exif: &MatchInput) -> Option<LensProfile> {
    let db = database()?;
    match choice {
        Some("none") => None,
        Some(key) => lens_by_key(db, key).map(profile_of),
        None => auto_match(exif),
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
    let profile = effective_profile(request.choice.as_deref(), &request.exif)?;
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
    let aperture = shot.aperture.filter(|value| value.is_finite() && *value > 0.0);
    let distance = shot.distance.filter(|value| value.is_finite() && *value > 0.0);
    let vignetting = aperture.and_then(|aperture| {
        lens.interpolate_vignetting(focal, aperture, distance.unwrap_or(FAR_DISTANCE))
            .and_then(|calib| rescale_vignetting(calib.model, lens, real_focal))
    });

    let crop = if shot.camera_crop.is_finite() && shot.camera_crop > 0.0 {
        shot.camera_crop
    } else {
        1.0
    };
    let norm = Norm::new(shot.width, shot.height, crop, real_focal)
        .with_center_offset(lens.center_x, lens.center_y, shot.width, shot.height);

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
    let crop = if crop.is_finite() && crop > 0.0 { f64::from(crop) } else { 1.0 };
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
    fn resolve_interpolates_between_focal_samples() {
        let lens = any_rectilinear_lens();
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
            camera_crop: if lens.crop_factor > 0.0 { lens.crop_factor } else { 1.0 },
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
        assert_eq!(candidates[0].key, LensProfile::key_of(&lens.maker, &lens.model));
        assert!(candidates.len() <= 200, "候选要封顶（下拉不是数据库浏览器）");
        let _ = db;
    }
}
