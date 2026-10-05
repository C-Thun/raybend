# ORF 内嵌预览提取（含 DNG/3FR strip 预览）+ 11 品牌样张矩阵

**完成时间：2026-10-05 14:31:07**

---

## 1. 起点：现象与根因

崔总 2026-10-05 报：E-M5 Mark II 的 `.ORF` 在网格里没有画面（占位图）。诊断结论（见对话中的问题报告）：

* **不是解码问题** —— rawler 0.8.0 完整解码 ORF 正常（4608×3456，~1.2 s）；
* **不是「没有内嵌预览」** —— 文件里有 3200×2400 JPEG（1,096,496 B，offset 52224），Windows 资源管理器就是靠它出图的；
* 是**我们没去找**：奥林巴斯不写标准 TIFF 的 `0x0201/0x0202`，预览藏在厂商 MakerNote 里；
  而 rawler 的 `OrfDecoder` 根本没实现 `preview_image`/`thumbnail_image`（上游 main 分支同样没有）。

## 2. 改动范围

四处，全在「内嵌预览提取」这条链上（前端零改动）：

| # | 改动 | 文件 |
| --- | --- | --- |
| 1 | **Olympus / OM System MakerNote 预览**：主 IFD `0x0100 ThumbnailImage`、CameraSettings 子 IFD（`0x2010..=0x2050` 里的 `0x0100/0x0101/0x0102`）、主 IFD 的 `0x1035/0x1036/0x1037` 变体；签名 `OLYMPUS\0`（IFD 在 +12）与 `OM SYSTEM\0`（+16），**偏移全相对 MakerNote 起点** | `crates/raybend/src/media/tiff.rs`（新增 `olympus_maker_note_spans` / `maker_note_order` / `checked_span` / `single_int`；`embedded_jpeg_spans` 追加 MakerNote 与 Exif 子 IFD 分支） |
| 2 | **span 选取改为「大的在前」**：`read()` 取第一个能解开的，小图请求要的是「缩下去」不是「放大上来」 | 同上（`sort_unstable_by_key(Reverse(len))`） |
| 3 | **JPEG 压缩的整块 strip 预览**：`Compression ∈ {6,7}` + `0x0111/0x0117`，且 `PhotometricInterpretation ∉ {CFA(32803), LinearRaw(34892)}` —— DNG / 3FR 的预览就是这个形状 | 同上 |
| 4 | **`pick_embedded` 语义修正**：`embedded_only` 原先无条件接受第一张（哪怕 160×120），现在与其它请求一致走「最小够用，都不够就取最大」；同时保持**惰性**（够用就不去解码大预览） | `crates/raybend/src/raw/rawler_backend.rs` |

## 3. 关键决策与依据

* **偏移语义不是猜的**：ExifTool `MakerNotes.pm` 的 `MakerNoteOlympus2/3`（`Start = valuePtr+12/+16`、`Base = start-12/-16`）与 `Olympus::Main` / `Olympus::CameraSettings` 标签表
  （https://exiftool.sourceforge.net/TagNames/Olympus.html ）；实测两个样本互相印证：
  `AM300135.ORF`（E-M5 II）`PreviewImageStart=48652` + MakerNote 起点 3572 = 52224；
  `OM-5 Mark II`（OM SYSTEM 签名）`46728 + 5496 = 52224`，两处都是 `FF D8` 开头、长度字段与 EOI 吻合。
* **必须排除 CFA / LinearRaw**：真 RAW 条带也用 JPEG 系压缩（lossless JPEG），Sigma fp DNG 的 raw 条带就有 28 MB —— 不排掉会白读几十 MB，还会把传感器数据当预览。两个取值都在 `PhotometricInterpretation` 上判，不需要解码试探。
* **`embedded_only` 不能「有总比没有好」**：网格档没有真解码兜底，但「拿 160×120 顶替 6000×4000」不是够用而是错。改成「最小够用 → 不然取最大」后，CR3/RAF 这类缩略图本来就够大的格式**行为不变**（早退路径相同）。
* **不动 rawler**：上游 0.8.0 就是最新版且 main 分支 `orf.rs` 逐字节相同（已下载比对）。预览提取放在我们自己的有界解析器里，顺带覆盖「rawler 不认识的机型也能出网格图」这件事（见 §5 的 A7 V / Hasselblad）。

## 4. 验证

### 4.1 单元测试（`cargo test -p raybend`：1444 passed / 0 failed / 12 ignored）

新增 4 条，都是合成的、不依赖外部样本：

* `orf_maker_note_preview_is_found_and_beats_the_tiny_thumbnail`（CameraSettings 与主 IFD 0x1035 两种写法）
* `orf_preview_is_read_end_to_end`（小端/中文文件名，走 `embedded::read`）
* `orf_maker_note_preview_valid_zero_falls_back_to_the_thumbnail`
* `foreign_maker_note_is_not_parsed_as_olympus`
* `dng_strip_preview_is_extracted_and_the_cfa_strip_is_not`（CFA 与 LinearRaw 两种都要被排除；raw 条带故意以 `FF D8` 开头）
* `embedded_pick_uses_the_smallest_sufficient_then_the_largest`
* 真实样本门控测试：`AM300135.ORF` 能取到 3200×2400 的预览

`cargo check --workspace --all-targets` 通过（仅 2 条既有 warning）。

### 4.2 真实样本（`thumb-probe` 网格档 384 / 与 `raw-smoke --full` 完整解码）

样本放在 `/mnt/c/src/tmp/raw-samples/`（CC0，来自 raw.pixls.us，共 636 MB；**不进仓库**）。
（`AM300135.ORF`、`P1000019.RW2` 取自崔总原有的 `/mnt/c/src/tmp/pic/`。）

| 品牌 | 机型（发布年） | 格式 | 网格小图 | 完整解码 | 备注 |
| --- | --- | --- | --- | --- | --- |
| OM System | OM-5 Mark II（2025） | ORF | ✅ 384×288 | ✅ 5184×3888 | `OM SYSTEM` 签名分支；本次修复的主目标 |
| Olympus | E-M5 Mark II（2015） | ORF | ✅ 384×288 | ✅ 4608×3456 | 崔总报的那台 |
| Panasonic | DC-GH7（2024） | RW2 | ✅ 384×288 | ✅ 5776×4336 | 原本就通 |
| Sony | ILCE-7M5 / A7 V（2025） | ARW | ✅ 384×256 | ❌ Unknown camera（mode `arw6`） | 预览通、解码缺 rawler 支持 |
| Canon | EOS R6 Mark III（2025） | CR3 | ✅ 384×256 | ✅ 6960×4640（4.0 s） | 走 rawler 的 CR3 预览 |
| Nikon | Z 8（2023） | NEF | ✅ 384×256 | ❌ `NEF compression HighEfficency is not supported` | 预览通、HE 压缩缺支持 |
| Pentax | KF（2022） | PEF | ✅ 384×256 | ✅ 6000×4000 | |
| Ricoh | GR IIIx（2021） | DNG | ✅ 384×256（修前 160×120） | ✅ 6000×4000 | 本次 strip 预览修复 |
| Fujifilm | X-T50（2024，X-Trans） | RAF | ✅ 384×256 | ✅ 7728×5152 | 色彩正确性归人类目视 |
| Fujifilm | GFX100RF（2025，GFX Bayer） | RAF | ✅ 384×288 | ✅ 11648×8736（11.5 s） | |
| Sigma | fp（2019，Bayer DNG） | DNG | ✅ 384×256（修前 160×120） | ✅ 6000×4000 | 本次 strip 预览修复 |
| Sigma | sd Quattro（X3F/Foveon） | X3F | ⚠️ 占位图 | ❌ `Failed to decode image` | **rawler 的 X3F 路浪费**：需机型库 + `raw_metadata` 是 `todo!()`；按崔总口径跳过 |
| Hasselblad | CFV 100C（2024） | 3FR | ✅ 384×288（修前占位图） | ❌ Unknown camera（`CFV 100C/Electronic Shutter`） | 本次 strip 预览修复；解码缺机型库 |
| Leica | M10-R（2020） | DNG | ✅ 384×254 | ✅ 7864×5200 | |

修后的三个格式都核对过抽出来的是**真预览**（PIL 解码）：Ricoh 6000×4000、Sigma 6000×4000（另有 640×480/160×120 小图）、Hasselblad 3888×2918。

### 4.3 样张来源（可复现）

raw.pixls.us 的仓库索引 `https://raw.pixls.us/json/getrepository.php?set=all`，
文件地址 `https://raw.pixls.us/getfile.php/<id>/nice/<文件名>`。本次用到的 id：
`8549` OM-5 II、`8062` DC-GH7、`8846` ILCE-7M5、`8961` EOS R6 III、`6616` Z 8、`6664` KF、
`5818` GR IIIx、`7807` X-T50、`8091` GFX100RF、`7273` fp、`6756` sd Quattro、`7782` CFV 100C、`7853` M10-R。

## 5. 遗留问题（都不阻塞；需要人类或上游决定）

1. **rawler 侧的解码缺口（我们改不了，除非上游/换后端）**：
   * Nikon **HE/HE★（High Efficiency）压缩**的 NEF（Z8/Z9/Z6III 等新机常见）→ 报不支持；
   * Sony **arw6**（新式无损压缩）机型库缺 mode 条目（A7 V 报 `Unknown camera`）；
   * Hasselblad **CFV 100C**（机型串带 `/Electronic Shutter`，库里没有 CFV 100C 任何条目）；
   * Sigma **X3F/Foveon**：解码器存在但机型库为空，`raw_metadata` 是 `todo!()`（会 panic）——按崔总口径「不支持就跳过」。
   建议：登记到 `memory/FUTURE.md`（RAW 后端一节）作为「换后端 / 等上游」的判据样本。
2. **DNG 元数据尺寸取的是 IFD0（缩略图）**：Ricoh GR IIIx 的 `read_photo_meta` 报 `160×120`（IFD0 = 160×120 缩略图，真尺寸在 SubIFD）。这会让 tile 排版比例与「看图」尺寸判断出错。CR3/RAF 则是 `0×0`（容器里读不到外层 EXIF，既有已知项）。属于独立问题，未在本次改动里处理。
3. **更新机型的样张缺口**：raw.pixls.us 里 Ricoh 最新只到 GR IIIx（2021）、Leica 只到 M10-R（2020）；dpreview / photographyblog / imaging-resource 在本机一律 403（含 legacy 域），官网样张页只有 JPEG。GR IV（2025）、Leica Q3/M11/SL3（2022–2024）的 RAW 需要人类手工下载后放进 `/mnt/c/src/tmp/raw-samples/` 再跑一遍矩阵。
4. **X-Trans 解码的观感**：X-T50 完整解码「成功」，但 rawler 的 X-Trans 去马赛克质量需要人类目视（本次只做冒烟）。

## 6. 纪律自查

* **XMP 侧影响（`AGENTS.md` §2.19）**：无。本次只改「从 RAW 里取内嵌 JPEG」这一步，不产生、不改写任何进 sidecar 的数据（编辑栈 / issue / 评级 / 色标 / 标签 / 文字 / 地点），也不动存储布局。
* **命令体系接入（§2.15）**：不适用 —— 没有新增用户可触发功能，纯内部行为修正。
* **数据来源**：11 个品牌 + DNG 的样张均为 CC0（raw.pixls.us），未入库。
