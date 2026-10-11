# import tiles 载入慢：真机实测定位 + 「库外/库内两套口径」规则落档

> 完成时间：**2026-10-11 17:55:42 +0800**
> 需求：崔总 2026-10-11 口述（「import 界面 tiles 载入这么慢，检查业务逻辑，先查原因再出方案」）
> 本篇只做**诊断 + 纪律落档**；产品代码一行未改（方案待批）。

---

## 1. 崔总的定性（本次落成 AGENTS.md §2.24）

- **import 与另外三个 flow 不是同一套口径**：import 面对**外部目录**（相机卡 / SD / 待入老库 / 临时盘），
  导入完就不会再访问；库内（浏览/编辑/导出）有 catalog + 缩略图缓存、同一张图反复看几十次。
- **小图格式**：库内 AVIF q90 / 4:4:4（画质与占用优先）；**import 一律 JPEG**（"怎么可能用 avif"）。
- **import 里折腾缓存全是错的**：SD 卡导入完就清、thumbcache 那套在 import 没有有效期可言；
  用户看到的「越优化越慢」正是这么来的。
- **import 只有一道题**：最快把窗口内那几屏 tiles 显示出来。只读必要的、质量可以让步（点中再读真东西）、
  窗口优先、不许卡（要等就上过渡）。
- **做减法不做加法**：先删工作（不读 / 不编码 / 不缓存），剩下的才谈调度。

## 2. 实测（Windows **release**，崔总自己的目录 `C:\src\tmp\pic`，306 张 = 161 JPG + 145 RAW）

探针：`crates/raybend/examples/source-import-probe.rs`（新增，未提交；交叉编译成 Windows exe 跑）。

| 环节 | 实测 | 判读 |
| --- | --- | --- |
| `source_scan`（进目录的"预扫描"） | **1–4 ms / 306 张** | 不是瓶颈（当初"只扫窗口内"的方向对） |
| `source_times`（按时间读全目录 EXIF） | 100–330 ms / 306 张 | 小头；慢盘上会放大（306 次随机读） |
| 冷渲染一张（JPG） | **≈ 450–500 ms** | 主因 |
| 冷渲染一张（RAW） | 200–700 ms（走唯一 worker） | 主因 |
| 命中缓存一张 | **0.55 ms** | 缓存本身极快（差距 ~1000×） |
| 一屏 24 张（4 并发、全冷） | **≈ 3–4 s**（2.97 / 3.87 / 3.93 / 4.32 s） | 用户看到的"载不出来" |

编码分解（同一张 384px 图）：

| 设置 | 单张 | 4 路并发吞吐 | 体积 |
| --- | --- | --- | --- |
| AVIF q90 speed10 **threads=1（当时的现状）** | 118–166 ms | 17.6–20.7 张/s | 25 KB |
| AVIF q90 speed10 threads=auto | 35–60 ms | 44–48 张/s | 25 KB |
| AVIF q80 speed10 threads=auto | 44–50 ms | — | 14.5 KB |
| **JPEG q82（import 第一版口径）** | **3–5 ms** | — | 26.6 KB |

命中率（拿崔总真实 `_sources/thumbs.db` 的副本问当前签名 `avif-q90-grid-v16-embedded-v1`）：
**grid 44/306、strip 0/306** → 该目录 86% 每次都要重渲染。sources 缓存已 151 MB / 4179 行，
同一文件最多 445 行（v7–v16 十代尸体），而 `cache::drop_stale_signatures` / `gc` **全仓没有调用点**。

## 3. 变成这样的过程（git 考古，回答"最早是怎么做的"）

| 时间 | 提交 | 干了什么 | 代价 |
| --- | --- | --- | --- |
| 2026-09-16 | `806d59d` | **第一版**：`JpegEncoder::new_with_quality(…, 82)`，一档编码直出 | ~10–30 ms/张（够用） |
| 2026-09-24 | `13ba305` | M3-W3 把**全系统缓存图**统一 AVIF q90 4:4:4（签名 `jpeg-q82-grid-v5` → `avif-q90-grid-v6`） | **import 被一起带下水**：编码 3–5 ms → 100+ ms |
| 2026-09-30 | `2b383ec` | "tiles 小图直读"：内嵌预览优先 + `embedded-v1`，但**先编码后比尺寸** | 对 160×120 的内嵌缩略图**白付一次 AVIF 编码**（+118 ms） |
| 本次改动前（工作区） | — | `.with_num_threads(Some(1))`（同一文件注释却写着"线程是必须的"） | 再 ×3.4 |

三者叠加 ≈ 40× 负优化，且**与磁盘快慢无关**（这就是"本地 SSD 也卡爆、Explorer 却不卡"的真正原因）。

## 4. 本次实际改动（只有文档 + 一个探针）

- `AGENTS.md`：新增 **§2.24**（库外/库内两套口径，含"禁止在来源目录折腾缓存"、"窗口优先"、
  "变化点清单法"、"先做减法"）；§6.5 与 §11.5 把 AVIF 口径限定为**库内**；§11.4 加一行指向 §2.24。
- `memory/FUNCTION-IMAGING.md`（格式唯一事实源）：三档表补"上表只管库内，来源目录一律 JPEG、不做缓存经营"。
- `memory/ARCHITECTURE.md` §6.4 缓存段、`memory/FUNCTION-REPOSITORY.md` §4.1 缓存段：AVIF 口径收窄为库内。
- `crates/raybend/examples/source-import-probe.rs`：诊断探针（`--screen` / `--encode-bench` /
  `--raw-concurrency` / `--hits`），**未提交**，留作改完复测的前后对比。

## 5. 验证方式

- 探针在 Windows release 下跑过四组：逐张冷/热、编码矩阵（含 4 路并发）、RAW 串并行对照、真实缓存命中率。
- 命令（WSL 侧经 `cmd.exe` 交给 Windows cargo；脚本 `/tmp/win-probe.mjs` 一次性）：
  `cargo build -p raybend --example source-import-probe --release` →
  `…\release\examples\source-import-probe.exe C:\src\tmp\pic <cache> --screen|--encode-bench|--hits <db>`
- 文档改动：`pnpm lint:i18n` / markdown 检查无新增告警（本次只动 .md）。

## 6. 遗留 / 待批

1. **产品代码未动**。待批的减法清单见对话与后续 `specs/`：
   来源目录小图改 JPEG（与库内共用一套 stage 代码、只换 encoder 与缓存签名）、
   内嵌预览"先比尺寸再编码"、tiles 请求窗口优先、RAW 的 EXIF 探测不抢解码锁、按时间 EXIF 读批量化。
2. sources 缓存里的旧 AVIF 行（151 MB）在改口径后**全是尸体**；本轮范围不含 GC ——
   要么下轮接 `drop_stale_signatures`，要么直接删 `cache/_sources/thumbs.db`（可重建缓存，§6.5 红线内）。
3. `with_num_threads(Some(1))` 与库内 AVIF 口径的冲突（单线程 vs 文档说"线程是必须的"）本轮未改 ——
   它属 browse/编辑/导出 的缓存图路径，不在本次范围内。
