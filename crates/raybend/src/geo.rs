//! 离线逆地理编码：按经纬度找**最近的城市**，给出国家 / 省州 / 城市。
//!
//! 面向手机相片（崔总 2026-11-03）：手机只写 GPS，**不写**「国家 / 省 / 市」，
//! 于是右栏那三项一直空着。这里用一份随应用发布的紧凑城市表把坐标翻译成地名，
//! 作为**建议值**交给界面 —— 写信的是用户手上的那次操作（可撤销），不是后台自动改库。
//!
//! # 数据
//!
//! 数据由 `scripts/build-geo-data.mjs` 从 GeoNames 的公开 dump 生成（CC BY 4.0，
//! 署名见 `crates/raybend/assets/geo/README.md`），编译期随二进制打进来（`include_str!`）：
//!
//! * `cities.tsv` —— 34 197 个人口 ≥ 1.5 万的城市（纬度、经度、英文名、中文名、
//!   一级行政区代码、国家代码、人口）；
//! * `admin1.tsv` —— 一级行政区（`CN.22` 这类代码 → 英文名 + 中文名，非中国只有英文）；
//! * `countries.tsv` —— 国家（ISO 3166-1 alpha-2 → 英文名 + 中文名）。
//!
//! 为什么**不**在运行时下载：GeoNames 的 dump 每天重新生成、没有稳定版本号，
//! 客户端无法用固定哈希校验；而这里需要的只是「最近的城」，2 MB 静态表一次付清、
//! 离线可用，比「首次下载 + 校验 + 失败重试」简单也更可靠。
//!
//! # 精度
//!
//! 只回答「照片落在哪个城市附近」：城市表按人口排序，**同距离取人口多的那个**
//! （最近的两个居民点常常只差几公里，用户心里的「地点」是大的那个）。
//! 结果里带 `distance_km`，界面可以据此提示「最近的城在 40 km 外」。
use std::collections::HashMap;
use std::sync::OnceLock;

/// 一个坐标附近的地点（**已按请求的语言**选好显示名）。
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// 城市显示名（中文请求且数据里有中文名就给中文，否则英文）
    pub city: String,
    /// 一级行政区显示名（可能是英文；数据里没有中文译名的国家保持原样）
    pub province: Option<String>,
    /// 国家显示名
    pub country: String,
    /// ISO 3166-1 alpha-2（界面上留作将来筛选用）
    pub country_iso: String,
    /// 到城市中心的大圆距离（公里）
    pub distance_km: f64,
}

#[derive(Debug, Clone)]
struct City {
    lat: f64,
    lon: f64,
    en: Box<str>,
    /// 中文名；数据里没有就是空
    zh: Box<str>,
    admin1: Box<str>,
    iso: Box<str>,
    population: u64,
}

#[derive(Debug, Clone, Default)]
struct Names {
    en: Box<str>,
    zh: Box<str>,
}

impl Names {
    fn pick(&self, zh: bool) -> &str {
        if zh && !self.zh.is_empty() {
            &self.zh
        } else {
            &self.en
        }
    }
}

/// 解析好的数据集（三张表）。
#[derive(Debug, Default)]
pub struct Dataset {
    cities: Vec<City>,
    admin1: HashMap<Box<str>, Names>,
    countries: HashMap<Box<str>, Names>,
}

impl Dataset {
    /// 最近的城（没有数据 / 坐标非法 → `None`）。
    ///
    /// `zh` = 用中文名（调用方传界面语言；中文名缺失时自动退回英文）。
    #[must_use]
    pub fn nearest(&self, lat: f64, lon: f64, zh: bool) -> Option<Place> {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return None;
        }
        let mut best: Option<(&City, f64)> = None;
        for city in &self.cities {
            let distance = haversine_km(lat, lon, city.lat, city.lon);
            // 城市表按人口降序：严格小于 ⇒ 同距离保留人口多的那个
            match best {
                Some((_, current)) if distance >= current => {}
                _ => best = Some((city, distance)),
            }
        }
        let (city, distance) = best?;
        let province = self
            .admin1
            .get(&*city.admin1)
            .map(|names| names.pick(zh).to_string())
            .filter(|name| !name.is_empty());
        let country = self
            .countries
            .get(&*city.iso)
            .map_or_else(|| city.iso.to_string(), |names| names.pick(zh).to_string());
        Some(Place {
            city: if zh && !city.zh.is_empty() {
                city.zh.to_string()
            } else {
                city.en.to_string()
            },
            province,
            country,
            country_iso: city.iso.to_string(),
            distance_km: distance,
        })
    }
}

/// 大圆距离（公里）。用 haversine 而不是平面近似：高纬度与跨经线都不会失真。
#[must_use]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.008_8;
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let dphi = (lat2 - lat1).to_radians();
    let dlambda = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (dlambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().clamp(-1.0, 1.0).asin()
}

/// 解析 `cities.tsv`（**跳过 `#` 注释行**；列顺序见生成脚本）。
pub fn parse_cities(text: &str) -> Vec<City> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(lat), Some(lon), Some(en), Some(zh), Some(admin1), Some(iso), Some(population)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            continue;
        };
        let (Ok(lat), Ok(lon)) = (lat.parse::<f64>(), lon.parse::<f64>()) else {
            continue;
        };
        out.push(City {
            lat,
            lon,
            en: en.into(),
            zh: zh.into(),
            admin1: admin1.into(),
            iso: iso.into(),
            population: population.parse().unwrap_or(0),
        });
    }
    out
}

/// 解析「代码 → 英文名 → 中文名」的两列附表（`admin1.tsv` / `countries.tsv`）。
pub fn parse_names(text: &str) -> HashMap<Box<str>, Names> {
    let mut out = HashMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(code), Some(en), zh) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        out.insert(
            code.into(),
            Names {
                en: en.into(),
                zh: zh.unwrap_or_default().into(),
            },
        );
    }
    out
}

/// 从内嵌数据里构造数据集（测试与首次使用各一次）。
#[must_use]
pub fn parse_dataset(cities: &str, admin1: &str, countries: &str) -> Dataset {
    Dataset {
        cities: parse_cities(cities),
        admin1: parse_names(admin1),
        countries: parse_names(countries),
    }
}

/// 随二进制发布的数据集（首次用到时解析一次；之后是纯内存查表）。
#[must_use]
pub fn dataset() -> &'static Dataset {
    static DATASET: OnceLock<Dataset> = OnceLock::new();
    DATASET.get_or_init(|| {
        parse_dataset(
            include_str!("../assets/geo/cities.tsv"),
            include_str!("../assets/geo/admin1.tsv"),
            include_str!("../assets/geo/countries.tsv"),
        )
    })
}

/// 按坐标找最近的城市（中文请求）。
#[must_use]
pub fn locate(lat: f64, lon: f64, zh: bool) -> Option<Place> {
    dataset().nearest(lat, lon, zh)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CITIES: &str = "\
# 注释行不算数据
lat\tlon\tname_en\tname_zh\tadmin1_code\tcountry_iso\tpopulation
30.2936\t120.1614\tHangzhou\t杭州\tCN.02\tCN\t11936010
39.9075\t116.3972\tBeijing\t北京\tCN.22\tCN\t18960744
35.6895\t139.6917\tTokyo\t东京\tJP.40\tJP\t9733276
48.8566\t2.3522\tParis\t\tFR.11\tFR\t2138551
";
    const ADMIN1: &str = "CN.02\tZhejiang\t浙江省\nCN.22\tBeijing\t北京市\nJP.40\tTokyo\t\nFR.11\tIle-de-France\t\n";
    const COUNTRIES: &str = "CN\tChina\t中国\nJP\tJapan\t日本\nFR\tFrance\t法国\n";

    fn tiny() -> Dataset {
        parse_dataset(CITIES, ADMIN1, COUNTRIES)
    }

    #[test]
    fn nearest_picks_the_closest_city_and_localizes_names() {
        let dataset = tiny();
        // 杭州西南约 30 km（西湖区附近）→ 一定是杭州，不是上海（没在表里）
        let place = dataset.nearest(30.1, 120.0, true).unwrap();
        assert_eq!(place.city, "杭州");
        assert_eq!(place.province.as_deref(), Some("浙江省"));
        assert_eq!(place.country, "中国");
        assert_eq!(place.country_iso, "CN");
        assert!(
            (20.0..40.0).contains(&place.distance_km),
            "到杭州的直线距离应在 20–40 km，实际 {}",
            place.distance_km
        );
        // 英文请求拿英文名
        assert_eq!(dataset.nearest(30.1, 120.0, false).unwrap().city, "Hangzhou");
    }

    #[test]
    fn missing_chinese_names_fall_back_to_english() {
        let place = tiny().nearest(48.86, 2.35, true).unwrap();
        assert_eq!(place.city, "Paris", "没有中文名就退回英文，不显示空");
        assert_eq!(place.country, "法国", "国家名有中文就用中文");
        assert_eq!(
            place.province.as_deref(),
            Some("Ile-de-France"),
            "该国家的一级行政区没有中文译名时保持原样"
        );
    }

    #[test]
    fn out_of_range_coordinates_are_rejected() {
        let dataset = tiny();
        assert!(dataset.nearest(91.0, 0.0, true).is_none());
        assert!(dataset.nearest(0.0, -181.0, true).is_none());
        assert!(dataset.nearest(f64::NAN, 0.0, true).is_none());
        assert!(Dataset::default().nearest(0.0, 0.0, true).is_none(), "空数据集不能凭空造一个地点");
    }

    #[test]
    fn haversine_matches_known_distances() {
        // 北京 → 上海 约 1067 km（大圆）
        let beijing_shanghai = haversine_km(39.9042, 116.4074, 31.2304, 121.4737);
        assert!((beijing_shanghai - 1067.0).abs() < 10.0, "{beijing_shanghai}");
        assert_eq!(haversine_km(10.0, 20.0, 10.0, 20.0), 0.0);
    }

    #[test]
    fn same_distance_prefers_the_larger_city() {
        // 两座城市在同一位置：表按人口降序，先出现的（人多的）胜出
        let text = "\
lat\tlon\tname_en\tname_zh\tadmin1_code\tcountry_iso\tpopulation
1.0\t1.0\tBig\t大城\tX.01\tXX\t900000
1.0\t1.0\tSmall\t小城\tX.01\tXX\t1000
";
        let dataset = parse_dataset(text, "", "");
        assert_eq!(dataset.nearest(1.0, 1.0, true).unwrap().city, "大城");
    }

    #[test]
    fn malformed_rows_are_skipped_not_fatal() {
        let text = "\
lat\tlon\tname_en\tname_zh\tadmin1_code\tcountry_iso\tpopulation
not-a-number\t1.0\tBroken\t\t\tXX\t1
1.0\t2.0\tGood\t好城\t\tXX\t10
";
        let dataset = parse_dataset(text, "", "");
        assert_eq!(dataset.nearest(1.0, 2.0, true).unwrap().city, "好城");
    }

    #[test]
    fn shipped_dataset_answers_for_well_known_places() {
        // 内嵌数据的一次冒烟：这三个点换数据后仍然应当是这个答案
        let beijing = locate(39.9042, 116.4074, true).expect("北京必须有地点");
        assert_eq!(beijing.city, "北京");
        assert_eq!(beijing.country, "中国");
        assert_eq!(beijing.province.as_deref(), Some("北京市"));

        let hangzhou = locate(30.2741, 120.1551, true).expect("杭州必须有地点");
        assert_eq!(hangzhou.city, "杭州");
        assert_eq!(hangzhou.province.as_deref(), Some("浙江省"));

        let new_york = locate(40.7128, -74.0060, false).expect("纽约必须有地点");
        assert_eq!(new_york.country, "United States");
        assert_eq!(new_york.country_iso, "US");
    }
}
