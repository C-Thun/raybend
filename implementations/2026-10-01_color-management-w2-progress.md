2026-10-01 00:36:38 CST

# 色彩管理 W2 阶段实施

## 改动范围与文件

- `crates/raybend/src/color/icc.rs`、`input.rs`、`mod.rs`：RGB ICC v2/v4 输入校验、原始字节身份、Little CMS CPU 参考转换；生成 sRGB、Display P3、Adobe RGB 配置；JPEG/PNG/TIFF 单次解码读取 ICC，保留 16-bit 位深，PNG sRGB 标记与未知来源区分。
- `crates/raybend/src/raw/backend.rs`、`rawler_backend.rs`、`worker.rs`、`develop/pipeline.rs`：RAW worker v5 回传像素色彩编码与 rawler Calibrate 选用的相机矩阵身份；旧协议拒绝。内嵌预览无可靠配置时标为未解析，不伪称 sRGB。
- `crates/raybend/src/export/output.rs`：现有 sRGB 导出的 JPEG/PNG/TIFF 写入 sRGB ICC；用色彩输入路径回读字节和身份。TIFF 使用已有 EXIF/TIFF 解析器兜底读取 34675 tag，因为 image decoder 对这份合法导出未返回该 tag。
- `src/features/editor/panels.tsx`、`src/workspaces/editor/EditorWorkspace.tsx`、`src/App.tsx`、中英语言包：editor right 第三组加入「色彩管理」页签，明确展示当前未激活的能力，管理入口跳转统一设置弹窗的配置文件页。
- `Cargo.toml` / `Cargo.lock`：接入 lcms2 静态构建和已在依赖树里的 png 元数据读取库。
- `crates/raybend/src/color/working.rs`、`input.rs`：新增独立的 f32 线性 Rec.2020 D65 工作图，允许负值与 >1，拒绝非有限数；位图 16-bit 输入可直接构造工作图；预览缩图保持浮点范围，并在 worker 内完成以减少跨进程传输。
- `crates/raybend/src/raw/backend.rs`、`rawler_backend.rs`、`worker.rs`、`tests/raw_worker.rs`：协议升 v6，新增 `linear-rec2020-f32` 请求。新版路径停在 rawler `Calibrate` 之前，用其矩阵归一化/伪逆与白平衡规则校准相机 RGB，再转 Rec.2020；避免该上游步骤裁负值和压缩 >1。worker 仍隔离解码，响应校验编码、矩阵 ID、有限值与 1 GiB 载荷上限。
- `crates/raybend/src/color/matrix_trc.rs`：矩阵/TRC ICC 的受控快路径。源/目标矩阵及曲线从 Little CMS 已解析标签构造，内置与随机样本逐项对照独立 LCMS 参考变换；CLUT 标签或误差超预算自动回退。该快路径覆盖输入和最终 RGB 输出，绝不放进每帧显示查询。

## 关键决策

旧 `LegacySrgb8V1` 保持原解码和显影路径，不因增添解析库而改变现有 issue 外观。新入口未知来源直接返回未解析，冲突配置拒绝；颜色实际转换仅在明确输入配置下执行。输出先改当前已确认为 sRGB 的容器，广色域导出在像素、EXIF、ICC 与容器标记一致之前不开放。系统显示器配置不进入照片资源身份、缓存或成片。

性能优先：f32 RAW 支持在 worker 内限长边，并让 sRGB→Rec.2020 原色转换原位执行，不给每个像素另建一份结果。手动 2MP 合成基准：`NO_OPTIMIZE` 输入 388ms / 输出 430ms；改为 Little CMS 默认优化后输入 447ms / 输出 448ms，未改善。矩阵/TRC 快路径同机测得输入约 19ms、输出约 50ms，且经过 LCMS 参考样本与随机非网格颜色核对。复杂 CLUT 仍走约 388/430ms 级的参考路径（实际 CLUT 耗时待样本测），这类配置不能在交互/逐帧重跑；后续生成 GPU LUT 或任务级缓存。若短期解决不了，将 CLUT 样本、基准和内存数据交 Astro，不能先默认开启。


## 验证

已通过新 ICC/位图/RAW 单元测试、现有五格式导出测试，以及 `cargo check --workspace`、`pnpm typecheck`、`pnpm lint:i18n`。`cargo test -p raybend color::`、`raw::` 和进程级 `--test raw_worker` 全通过（worker v6）；输入/输出快路径的 4096 随机颜色及越域值均通过 LCMS 对照。`pnpm debug:win` 与产物核对通过：主程序、静态 Little CMS、v6 RAW worker 同步生成（快路径新增之后需再跑一次 Windows 构建）。本阶段的新界面和真实照片色彩仍未经人类 GUI/视觉验收。


## 遗留

W2 验收仍开放：CLUT ICC 样本与更完整的容器冲突策略；PNG 以外标准标记、灰度/浮点位图输入；真实 RAW 样本对照与上游 `Rescale` 的裁切边界；新版调整/LUT 的工作域与旧版兼容；高性能 ICC 转换、所有缓存及输出格式的色彩身份；全量质量门。f32 全图 worker 往返目前可能产生数百 MB 载荷和较高峰值，需在真实 24MP/60MP 上测并考虑共享缓冲/瓦片交接。若 TIFF 原生 decoder 后续能可靠读取 34675，可移除当前已有解析器的兜底分支。`cargo fmt --all -- --check` 仍会命中大量仓内既有格式差异，本轮仅格式化新色彩模块文件。
