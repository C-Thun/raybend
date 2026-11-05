# 离线城市数据（`geo/`）

`cities.tsv` / `admin1.tsv` / `countries.tsv` 是**生成文件**，不要手改。

- **生成**：`node scripts/build-geo-data.mjs`（默认带下载缓存；`--cache <dir>` 用已下载的文件）
- **用途**：右栏「按 GPS 填入地点」——按坐标找最近的城市，给出国家 / 省州 / 城市
- **读取**：`crates/raybend/src/geo.rs`（`include_str!` 打进二进制；首次用时解析一次）

## 来源与许可（CC BY 4.0）

| 内容 | 来源 | 许可 |
| --- | --- | --- |
| 城市（约 3.4 万，人口 ≥ 1.5 万） | [GeoNames](https://www.geonames.org/) 的 `cities15000` dump（`https://download.geonames.org/export/dump/cities15000.zip`） | **CC BY 4.0**（https://creativecommons.org/licenses/by/4.0/ ） |
| 一级行政区名 | GeoNames `admin1CodesASCII.txt` | 同上 |
| 国家中英文名 | CLDR / `Intl.DisplayNames`（运行生成脚本时取） | Unicode CLDR 数据许可 |

中文城市名取自 GeoNames 的 `alternatenames` 字段；**没有中文名的城市显示英文名**（不造译名）。
一级行政区只有中国有中文名（生成脚本里一张固定表），其它国家保持 GeoNames 的英文名。

## 为什么随仓库发布、不在运行时下载

1. GeoNames 的 dump **每天重新生成**，没有稳定版本号 —— 客户端无法用固定哈希校验；
2. 照片定位只需要「最近的城 + 省 + 国家」，一张 2 MB 的静态表足够，且离线可用；
3. 体积一次付清，比「首次使用下载 + 校验 + 失败重试」少一整条失败路径。

数据会随版本变旧（GeoNames 持续更新），**不需要**定期刷新；发现某些地名明显缺失时手动跑一次生成脚本即可。

## 署名（CC BY 4.0 要求）

本目录的数据改编自 GeoNames（https://www.geonames.org/ ），依 CC BY 4.0 使用。
许可原文见仓库根下 `legal/CC-BY-4.0.txt`，分发清单见 `THIRD-PARTY-NOTICES.md`。
