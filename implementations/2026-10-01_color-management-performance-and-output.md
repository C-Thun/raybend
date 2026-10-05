2026-10-01 03:18:18 CST

# 色彩管理性能闸门与工作图输出续作

## 改动范围与文件

- `crates/raybend/src/color/{icc,input,matrix_trc,working}.rs`：对常见 RGB 矩阵/TRC 配置建立有界的 8 配置变换缓存；RGB8/16 位图直接借用 decoder 的交错缓冲转换，不再先扩出整张 u16 RGB；新工作图导出直接产生编码器使用的交错 u16 缓冲。60MP 图像由此分别避免约 343 MiB 的输入/输出临时副本。复杂 CLUT 保持 Little CMS 参考回退。
- `crates/raybend/src/export/{output,metadata}.rs`：新增 `encode_working` 核心入口，线性 Rec.2020 工作图按目标 RGB ICC 转换并量化一次，随后沿用旧 JPEG/PNG/TIFF/WebP 编码器，嵌入与像素一致的 ICC。非 sRGB 的 EXIF ColorSpace 写 65535（Uncalibrated）；AVIF 非 sRGB 组合明确拒绝。旧 `encode` 仍按原 sRGB 处理旧 issue。
- `crates/raybend/src/thumbnail/render.rs`：保留 ignored 的 AVIF 色彩标记成本基准；默认编码和缓存版本恢复原来的 YCbCr/v14。
- `scripts/check-flowbar.mjs`：组件冒烟从 Vite 编译后的 FlowBar 模块取语言模块的实际 URL，避免热更新 `?t=` 使测试加载第二份 locale 状态。FlowBar 组件无需因此改动。
- `specs/cm-w2.md`：补实际可用范围、内存路径及未通过的性能闸门。

## 性能决策与依据

- ICC 解析、曲线建表和 LCMS 数值核验只在配置身份变化时做；滑杆、平移和逐帧显示不能触发整图 LCMS。常见矩阵/TRC 走通过 LCMS 样本比对的快路径。200 万像素合成图在同机空闲复测：首次配置准备约 10 ms，输入约 15 ms，原输出约 51 ms，交错导出约 51 ms；与此前 Little CMS 整图基准约 388/430 ms 相比留有余量。并行编译时同一基准曾升到输入 25–48 ms、输出 70–96 ms，故不以单次 WSL 数字宣称真机帧率。命令：`cargo test -p raybend bench_two_megapixel_icc_round_trip -- --ignored --nocapture`。
- AVIF `with_colorspace(Srgb)` 虽可独立读回显式 `nclx`，却改变内部编码模型。1920×1280 合成图、同质量/速度/单线程：旧 YCbCr 3.178s / 2,035,960 B，新 RGB 3.579s / 2,385,837 B，约慢 13%、大 17%。按崔总的性能要求撤回默认改动；复现：`cargo test -p raybend bench_avif_srgb_marker_cost -- --ignored --nocapture`。不能把这项标记验收勾掉。
- CLUT ICC 仍走 LCMS 参考路径，尚无独立真实 CLUT 样本和吞吐/误差预算；不接入实时显示。需给 Astro 的候选方向是保持 AV1/YUV 编码只补一致的容器色彩描述，以及一次性 LCMS 生成 GPU LUT 后与 CPU 参考核对；两者均不能未经样本与真机验证默认启用。

## 验证

- `cargo check --workspace` 通过；`cargo test --workspace` 在本轮导出接线后通过。后续内存优化由 `cargo test -p raybend color::`（42 通过、1 ignored）、`working_output_embeds_matching_p3_and_rejects_unmarked_avif` 和 RGB8/16 借用缓冲单测覆盖。P3 输出对 JPEG/PNG/TIFF/WebP 独立读回 ICC 与 EXIF，PNG/TIFF 还逐值读回 16-bit 像素。
- `pnpm typecheck`、`pnpm test`（123 通过）、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm build`、`git diff --check` 通过；FlowBar 浏览器冒烟 38 项通过，UI 冒烟 `problems: []`。
- `pnpm debug:win` 与其 `check:win` 产物核对通过；exe 与含 worker protocol v6 的 RAW worker 同步生成。本轮未做真实 GUI 操作、显示器色度测量或照片性能体感验证。

## 遗留与退路

新高精度工作图的真实编辑算子、GPU 显示、缓存和导出预设尚未连成产品路径，用户当前照片与旧 issue 仍按旧处理版本显示。RAW 高光/负值受上游 `Rescale` 边界影响，缺真 RAW 样本；CLUT ICC、AVIF 低开销显式标记、RGBA/CLUT 的整图内存峰值均待后续测量。未解决前保持新管线不默认启用，避免颜色错误或明显性能下降。Pencil `.pen` 仍需崔总手动保存，真机 GUI 与多显示器色彩由崔总验收。本轮按崔总最近指令未提交代码。
