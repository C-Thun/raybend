//! 离线逆地理编码的前端入口：数据与算法都在核心（`raybend::geo`），这里只做语言与 DTO。
//!
//! 为什么需要这条命令：照片只带回 GPS，而手机的「国家 / 省 / 市」基本是空的
//!（崔总 2026-11-03 的场景）。这儿用随应用发布的城市表把坐标翻译成地名，
//! **只作为建议值**交给界面 —— 真正写库要用户点一下（走 `browse_mark` 的 `setGeo`，可撤销）。
use serde::Serialize;

use crate::source::blocking;

/// 一个坐标附近的地点（`src/api/types.ts::GeoPlace` 的镜像，进 `dto-contract.json`）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoPlaceDto {
    pub city: String,
    /// 一级行政区（可能只有英文名，见核心 `geo` 模块的说明）
    pub province: Option<String>,
    pub country: String,
    pub country_iso: String,
    /// 到城市中心的大圆距离（公里，保留 1 位小数）
    pub distance_km: f64,
}

/// 按坐标找最近的城市；坐标非法或没有数据 → `None`。
///
/// `locale` 只用来选显示语言（`zh*` 用中文名，其余英文）；地名本身来自同一张表。
#[tauri::command]
pub async fn geo_reverse(lat: f64, lon: f64, locale: String) -> Result<Option<GeoPlaceDto>, String> {
    blocking(move || {
        let zh = locale.to_ascii_lowercase().starts_with("zh");
        Ok(raybend::geo::locate(lat, lon, zh).map(|place| GeoPlaceDto {
            city: place.city,
            province: place.province,
            country: place.country,
            country_iso: place.country_iso,
            distance_km: (place.distance_km * 10.0).round() / 10.0,
        }))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dto_keys_stay_camel_case() {
        let place = GeoPlaceDto {
            city: "杭州".into(),
            province: Some("浙江省".into()),
            country: "中国".into(),
            country_iso: "CN".into(),
            distance_km: 3.2,
        };
        let value = serde_json::to_value(&place).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["city", "country", "countryIso", "distanceKm", "province"]
        );
    }
}
