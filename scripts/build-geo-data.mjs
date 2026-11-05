#!/usr/bin/env node
/**
 * 生成「离线逆地理编码」用的紧凑数据集（`crates/raybend/assets/geo/*.tsv`）。
 *
 * 数据源：GeoNames 官方 dump（**CC BY 4.0**，署名见 `crates/raybend/assets/geo/README.md`）
 *
 *   * `cities15000.zip`        —— 3.4 万个人口 ≥ 1.5 万的城市（约 3.3 MB zip）
 *   * `admin1CodesASCII.txt`   —— 一级行政区名（约 3.9 千行）
 *   * 国家名                 —— Node 的 `Intl.DisplayNames`（zh-CN / en），不另抄表
 *
 * 为什么**生成后随仓库发布**而不是运行时下载：
 *   * GeoNames 的 dump **每天重新生成**，没有稳定版本号，下载端无法用固定哈希校验；
 *   * 照片定位只用到「最近的城市 + 省 + 国家」，一个 2 MB 的静态表足够，离线可用；
 *   * 体积一次付清，比「首次使用时下载 + 校验 + 失败重试」的 UX 简单得多。
 *
 * 用法：
 *   node scripts/build-geo-data.mjs                # 下载（带缓存）并生成
 *   node scripts/build-geo-data.mjs --cache /tmp   # 用已经下载好的文件
 *
 * 更新时机：发现新版城市名（尤其中文）明显更全时手动跑一次；**不是**日常任务。
 */

import { createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import { execFileSync } from "node:child_process";

const ROOT = new URL("..", import.meta.url).pathname;
const OUT = join(ROOT, "crates/raybend/assets/geo");
const args = process.argv.slice(2);
const cacheAt = args.indexOf("--cache");
const CACHE = cacheAt >= 0 ? args[cacheAt + 1] : "/tmp/raybend-geo-cache";

const SOURCES = {
  cities: "https://download.geonames.org/export/dump/cities15000.zip",
  admin1: "https://download.geonames.org/export/dump/admin1CodesASCII.txt",
};

/** 中国一级行政区的中文名（GeoNames 的 `CN.*` 代码 → 中文；其它国家保持英文）。 */
const CN_PROVINCES = {
  "CN.01": "安徽省",
  "CN.02": "浙江省",
  "CN.03": "江西省",
  "CN.04": "江苏省",
  "CN.05": "吉林省",
  "CN.06": "青海省",
  "CN.07": "福建省",
  "CN.08": "黑龙江省",
  "CN.09": "河南省",
  "CN.10": "河北省",
  "CN.11": "湖南省",
  "CN.12": "湖北省",
  "CN.13": "新疆维吾尔自治区",
  "CN.14": "西藏自治区",
  "CN.15": "甘肃省",
  "CN.16": "广西壮族自治区",
  "CN.17": "海南省",
  "CN.18": "贵州省",
  "CN.19": "辽宁省",
  "CN.20": "内蒙古自治区",
  "CN.21": "宁夏回族自治区",
  "CN.22": "北京市",
  "CN.23": "上海市",
  "CN.24": "山西省",
  "CN.25": "山东省",
  "CN.26": "陕西省",
  "CN.28": "天津市",
  "CN.29": "云南省",
  "CN.30": "广东省",
  "CN.31": "海南省",
  "CN.32": "四川省",
  "CN.33": "重庆市",
};

const CJK = /[\u3400-\u4dbf\u4e00-\u9fff]/;

async function ensure(url, file) {
  if (existsSync(file)) return file;
  mkdirSync(CACHE, { recursive: true });
  console.log(`下载 ${url}`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`下载失败 ${response.status}：${url}`);
  await pipeline(Readable.fromWeb(response.body), createWriteStream(file));
  return file;
}

/** 从 alternatenames 里挑一个中文名：优先 2–6 字、不以「市/县/区」结尾的写法。 */
function pickChinese(alternates) {
  const names = alternates
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name !== "" && CJK.test(name));
  if (names.length === 0) return "";
  // 单字多半是简称（沪 / 京 / 穗），不适合当地名；先只要 2–6 字的
  const sized = names.filter((name) => name.length >= 2 && name.length <= 6);
  const pool = sized.length > 0 ? sized : names;
  const plain = pool.filter((name) => !/[市县区]$/.test(name));
  const final = plain.length > 0 ? plain : pool;
  final.sort((a, b) => a.length - b.length || a.localeCompare(b));
  return final[0];
}

function build() {
  const citiesZip = join(CACHE, "cities15000.zip");
  const admin1File = join(CACHE, "admin1CodesASCII.txt");

  // 解压（zip 自带 CRC，解压即校验）
  const unpacked = join(CACHE, "cities15000.txt");
  if (!existsSync(unpacked)) {
    execFileSync("unzip", ["-o", "-q", citiesZip, "-d", CACHE]);
  }

  const admin1 = new Map();
  for (const line of readFileSync(admin1File, "utf8").split("\n")) {
    const [code, name] = line.split("\t");
    if (code === undefined || name === undefined) continue;
    admin1.set(code, { en: name, zh: CN_PROVINCES[code] ?? "" });
  }

  const zhNames = new Intl.DisplayNames(["zh-CN"], { type: "region" });
  const enNames = new Intl.DisplayNames(["en"], { type: "region" });
  const countries = new Map();
  const rows = [];

  for (const line of readFileSync(unpacked, "utf8").split("\n")) {
    if (line.trim() === "") continue;
    /*
     * GeoNames cities15000 的列（固定 19 列，docs/geonames-readme.txt）：
     *   0 geonameid · 1 name · 2 asciiname · 3 alternatenames · 4 lat · 5 lon
     *   6 feature class · 7 feature code · 8 country code · 9 cc2 · 10 admin1 code
     *   … 14 population …
     */
    const parts = line.split("\t");
    if (parts.length < 19) continue;
    const name = parts[1];
    const ascii = parts[2];
    const alternates = parts[3];
    const latitude = Number(parts[4]);
    const longitude = Number(parts[5]);
    const iso = parts[8];
    const admin1Code = parts[10];
    const population = Number(parts[14]);
    if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) continue;
    if (!/^[A-Z]{2}$/.test(iso)) continue;

    if (!countries.has(iso)) {
      countries.set(iso, {
        en: enNames.of(iso) ?? iso,
        zh: zhNames.of(iso) ?? "",
      });
    }
    const zh = pickChinese(alternates);
    rows.push({
      lat: latitude.toFixed(4),
      lon: longitude.toFixed(4),
      en: ascii || name,
      zh,
      admin1: admin1Code ? `${iso}.${admin1Code}` : "",
      iso,
      population: Number.isFinite(population) ? population : 0,
    });
  }

  // 城市按人口降序：最近两城距离相近时，人多的那个更可能是用户心里的「地点」
  rows.sort((a, b) => b.population - a.population);

  mkdirSync(OUT, { recursive: true });
  writeFileSync(
    join(OUT, "cities.tsv"),
    [
      "# 生成文件，勿手改：node scripts/build-geo-data.mjs",
      "# 来源 GeoNames cities15000（CC BY 4.0），见同目录 README.md",
      "# lat\tlon\tname_en\tname_zh\tadmin1_code\tcountry_iso\tpopulation",
      ...rows.map(
        (row) =>
          `${row.lat}\t${row.lon}\t${row.en}\t${row.zh}\t${row.admin1}\t${row.iso}\t${row.population}`,
      ),
    ].join("\n") + "\n",
  );

  writeFileSync(
    join(OUT, "admin1.tsv"),
    [
      "# 生成文件，勿手改：node scripts/build-geo-data.mjs",
      "# code\tname_en\tname_zh",
      ...[...admin1.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([code, names]) => `${code}\t${names.en}\t${names.zh}`),
    ].join("\n") + "\n",
  );

  writeFileSync(
    join(OUT, "countries.tsv"),
    [
      "# 生成文件，勿手改：node scripts/build-geo-data.mjs",
      "# iso2\tname_en\tname_zh（名字来自 CLDR / Intl.DisplayNames）",
      ...[...countries.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([iso, names]) => `${iso}\t${names.en}\t${names.zh}`),
    ].join("\n") + "\n",
  );

  const withZh = rows.filter((row) => row.zh !== "").length;
  console.log(
    `城市 ${rows.length}（其中 ${withZh} 个有中文名）、行政区 ${admin1.size}、国家 ${countries.size}`,
  );
}

await ensure(SOURCES.cities, join(CACHE, "cities15000.zip"));
await ensure(SOURCES.admin1, join(CACHE, "admin1CodesASCII.txt"));
build();
