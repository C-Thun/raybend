# 控制点取样：方向 bug + 小窗中位数

> 完成时间：2026-11-02 22:04:02
> 起因：崔总 2026-11-02 真实照片（`MYP1000048.RW2`）—— 天空点改到了墙上石质窗体，
> 墙点改到了天空；人体 / 衣服那张（`MYP1000027`）正常。

## 1. 根因（有数字）

**不是权重算法把天空和墙认反了，是取样读错了像素。**

| 证据 | 数值 |
| --- | --- |
| `MYP1000048.JPG` EXIF 方向 | **8**（PIL 读出） |
| 同一 RAW 的 decoder（worker）报的方向 | **1**（rawler 对 TIFF 家族不可靠） |
| 显示路径用的方向 | `raw_orientation(path, decoder)` = **8** |
| 旧 `develop_area_seed` 用的方向 | 直接 `decoded.orientation` = **1** |
| XMP 里 `天空` 的种子 | `[0.0709, -0.0119, 0.0517]` —— 暗部（Y′≈0.07） |
| XMP 里 `墙` 的种子 | `[0.9426, 0.0074, -0.0089]` —— 亮部（Y′≈0.94） |
| 同一坐标在**未摆正** JPEG 上采样 | 天空点 Y≈0.287（暗），墙点 Y≈0.935（亮） |
| 同一坐标在**摆正** JPEG 上采样 | 天空点 Y≈0.936（亮），墙点 Y≈0.536（中间调） |

也就是说：显示、画布标记、覆盖层都按方向 8 摆正了；取样命令却按方向 1 的传感器像素
去取 —— 天空点在未摆正图里落在暗部，墙点落在亮部。**种子被“换成了另一块地方的颜色”**，
后续的全图向量匹配当然会命中相反的区域。

`MYP1000027` 的 EXIF 方向是 **1**，所以那张的人体 / 衣服看起来正常 —— 这正好解释了
“为什么这张对、那张不对”。

另外验证过：`develop::base_curve::analyze` 只用直方图，不用像素位置，所以那边的
`decode_linear` 不摆正**不是** bug，本轮没有动它。

## 2. 改动

### 2.1 方向（根因修复）

`src-tauri/src/develop.rs`：

* 新增 `orient_seed_source(path, decoded)`：`decoded.orientation = raw_orientation(path, decoded.orientation)`。
* `develop_area_seed` 在 `LinearImage::from_raw16` 之前调用它 —— 与
  `editor::decode_linear_source` / `display::output` / `thumbnail::render` 同一条规则
  （文件头优先，worker 报的方向只作兜底）。
* 注释写明 2026-11-02 真实照片的症状，防止再被“直接 use decoder”绕过。

### 2.2 取样从单像素改为 7×7 小窗中位数

`crates/raybend/src/develop/pipeline.rs`：

* 新增 `SEED_SAMPLE_RADIUS = 3`（1024 长边解码上的 7×7；对 3712 宽原图约 25×25 像素）。
* 新增 `compose_linear_median(...)`：在方形窗内对“该层输入”的线性 RGB **逐通道取中位数**；
  窗夹在图像内，非有限值跳过。中位数比均值更抗热噪点 / 压缩边缘 / 单颗异色点，
  又不会像大窗那样把邻近物体混进来。
* `compose_linear_at` 改为 `radius = 0` 的委托，旧单点语义与测试逐位不变。
* 新增单测 `seed_sampling_uses_a_region_median_not_one_pixel`：5×5 全 0.2 + 中心一颗纯红热像素，
  单点必须采到红，7×7 中位数必须回到 0.2；窗贴边也不越界。

### 2.3 已有照片的旧种子：重新取样入口

旧照片的种子已经写进 latest / XMP，代码修复不会自动改它们；必须重新取一次。

* `EditorWorkspace`：把“旧种子迁移”和“重新取样全部控制点”抽成同一个
  `resamplePoints(repositoryId, assetId, shouldResample)`，**按行序**逐点取，
  前面的点换新种子后会进入后面的层输入（与 2026-11-01 评审同一条口径）。
* 新增 `resampleAllAreas`，注册为 `EditorActions.resampleAreas`；命令面板新增
  `editor.area.resample`（无默认热键，低频修复动作）。
* 编辑区面板标题栏新增刷新按钮（只在存在控制点时出现；SOOC 下禁用），
  点击即按层序重新取样全部控制点，然后一次 `confirmEdit()` 落库（一步撤销）。

### 2.4 文档

* `specs/editor-area.md` §3.3：取样改为“7×7 小窗逐通道中位数”，补 2026-11-02
  方向 bug 与真实照片证据，注明旧照片用标题栏刷新按钮重新取样。
* DTO 注释 `[luma, c1, c2]` 同步为 `[Y′, Cb, Cr]`。

## 3. 验证

```bash
# 真照片回归（手动，worker + 测试库）
cargo test -p raybend-desktop area_seed_orientation_matches_the_display_frame -- --ignored
# 输出：orientation seed luma: sky=0.7447, wall=0.2909；1 passed
# 方向摆正后 ΔY′≈0.454，而两个点的容差 103 / 155 的 luma_end≈0.124 / 0.198
# ⇒ 天空点不会再命中墙、墙点也不会再命中天空。

cargo test --workspace
# raybend lib 1631 passed / 0 failed / 11 ignored
# 其余 crate 与 desktop：1 + 7 + 4 + 194 + 2 + 2 passed；0 failed
# 合计 1841 passed；14 ignored（含本轮新增的真照片手动回归）

pnpm typecheck && pnpm test
# tsc 通过；1383 passed / 0 failed

pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
# 全过

pnpm build
# 通过（仅既有 chunk > 500 kB 提示）
```

## 4. 遗留 / 崔总这边要做的

1. **重新构建 Windows 产物**（`pnpm debug:win`），打开 `MYP1000048`。
2. 在「编辑区」标题栏点**刷新图标**（悬停提示「重新取样全部控制点」），
   或 `Ctrl+K` 搜「重新取样全部控制点」。它会按层序先重取「天空」、再重取「墙」，
   然后一次落库；`Ctrl+Z` 可整步撤销。
3. 重新取样后，两个点的温度 / 饱和度参数不变，但种子会换成正确的颜色；
   此时天空的 +7500K/+52 应只作用在天空，墙的 +4050K/+42 只作用在墙。
4. 真机观感、按钮位置、重新取样耗时仍属 E2E，由崔总在 Windows 上确认；
   Agent 只验证到单元 / 集成与生产构建。

## 5. 本轮评审追加修正（2026-11-02 23:15:42）

按第三方评审发现的三处补强：

1. **SOOC 基准下禁止取样**：`develop_area_seed` 在栈校验后显式拒绝
   `source_base != Raw`，不再依赖前端 `editBase() !== 'raw'` 这一道闸门。
   理由：SOOC 下控制点虽被旁路，但按位图栈取出的种子切回 RAW 后会被重新解释。
2. **旧种子迁移不再在 SOOC 下取样**：`migrateLegacySeeds` 增加 RAW 闸门；
   在 `setBase('raw')` 成功后补一次迁移，覆盖「没有 RAW 的照片 → 以后补了 RAW」这条路径。
3. **小 clippy 清理**：`compose_linear_median` 的 `let...else` 改 `?`，
   8 参数函数标 `#[allow(clippy::too_many_arguments)]`（沿用既有同类签名）。

验证：`cargo check --workspace`、`cargo test --workspace`（1841 passed / 0 failed /
14 ignored）、`pnpm typecheck && pnpm test`（1383 passed）、三个 lint、`pnpm build` 全过；
`cargo clippy --workspace --all-targets` 对本轮新函数不再报 warning。
