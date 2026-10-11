# 看图没有档位、100% 是设备像素、一次载入

**完成时间：2026-10-11 21:5x**（按崔总四条指令执行完毕）

## 0. 崔总的四条指令

1. 相关文档全删（含 `memory/FUTURE.md`），可预见的未来不做那套方案；
2. **所有看图的档位禁止存在**，统一到原图；
3. **任何看图只允许一次载入**，禁止一切可能触发多次载入的路径；
4. 不许再提「9 月 24 号认可」这类旧背书当作依据。

## 1. 真机两个症状的根因（都不是"需要新方案"，是 bug）

### 1.1 browse view 双击后倍率超过 100%（您看到"像 200%"）

看图用 `<img>` + CSS `transform: scale(zoom)` 画，`zoom` 的单位是**CSS 像素 / 图像像素**。
而「100%」在物理意义上应是 **1 图像像素 = 1 设备像素**（编辑器原生视口就是这个口径，
`AGENTS.md` §7.9）。于是 DPR=2 的屏幕上：

```text
双击「100%」→ zoom = 1 → 1 图像像素 = 2 设备像素 → 看起来就是 200%
而且浏览器把 5184 宽的全尺寸图放大 2 倍 ⇒ 100% 反而发虚
```

**修**：新增 `src/components/ui/viewer/zoom.ts`（全项目唯一一份换算）——
`oneToOneZoom() = 1 / devicePixelRatio`、`zoomPercent(zoom) = zoom × dpr × 100`、
`isOneToOne(zoom)`。单图看图（`store.ts::toggleFit`、`ViewerControls` 读数）、
对比视图（`CompareView` 的 100% / 读数）、全屏（同一个 store）全部改走它。
DPR **读运行时值**（写死必错）。

### 1.2 选 issue / latest 时尺寸不对（老库或未回填时）

变体尺寸 = 库里那一行；为 NULL 时（迁移后待回填 / 老库），库里没有数字，
于是第一个上屏的是**占位图**（1920），它一 `onLoad` 就把 `natural` 记成 1920 并标成
**「已知」** ⇒ 真身（5184×3888）到了也改不动（`naturalKnown` 挡住了）⇒ 布局盒小一圈，
「100%」被放大 ~2.7×。

**修**：`setNatural(size, authoritative)` —— 只有**真身在屏**（`sharp()`）时量到的尺寸才算
权威；占位图量到的那份只是"先摆个差不多的大小"，真身到达时会再报一次并替换
（`setNatural` 本来就会按比例换算 zoom，画面不跳）。单图、全屏都改。

## 2. 档位机制连根删除（指令 2）

* `crates/raybend/src/render/tier.rs` **删除**（`ImageTier` / `tier_for` / `wanted_tier` 全部消失），
  `render/mod.rs` 的导出同步删；
* `src-tauri/src/editor.rs`：`DevelopJob.tier` / `DevelopOutcome.tier` / `RenderState.tier|wanted_tier`
  删；`ensure_output()`（缩放跨阈值就重出图的那条路）**整个删除**，它的三个调用点一并去；
  进视口直接按原尺寸出图（不再"先屏幕档再升档"两跳）；
* `editor_working.rs`：`edge` 恒为 0（不缩）—— 显影输出**永远是原尺寸**；
* DTO：前后端 `tier` / `wantedTier` 字段与契约清单同步删除
  （`src/api/types.ts`、`src/api/dto-contract.test.ts`、`src/api/dto-contract.json`）；
* 前端与文档里所有 `tier_for_params` / 「预览档 / 全尺寸档」措辞清干净。

保留不动（这些是效率设施，不是档位）：线性源常驻显影线程（只换照片才解码）、
两层限流（前端按帧合并 + 后端 `merge_jobs`）、最新者优先的任务号、过渡帧。

## 3. 一次载入（指令 3）——审计结论

| 事件 | 会不会重新出图 / 载入 |
| --- | --- |
| 缩放 / 平移 / 适应窗口 / 100%（editor） | **不会** —— `RenderCommand::Intent` 处理里 0 次 `developer.send`（代码审计） |
| 窗口尺寸 / DPR 变化 | 不会重出图（视口自己变换；原先那条 `ensure_output` 调用已删） |
| 拖动参数 | 会重算**像素**（同一张图、同一分辨率、限流合并），**不弹载入提示**（`decode` 不被写） |
| 换照片 | **唯一**的一次载入：过渡帧顶着 + 「正在载入照片…」，真帧上屏后提示消失 |
| browse view / compare / 全屏 | 每张照片一次：占位（过渡）→ 原图原地替换；缩放在浏览器侧是纯 CSS 变换，不重取图 |
| 邻图预取 | 后台取下一张（不可见、不算用户可见的载入） |

「载入中」提示的口径（`src/features/editor/source.ts`、viewer 的 `viewer.loading_original`）：
**只属于换照片那一次**；同一张照片的任何变化都不许再亮。

## 4. 文档（指令 1）与永久禁止项

* 删除 `memory/FUTURE.md` 的「editor 只渲染可见区域」整条；把「拖动中只算预览档」标为**已废弃**；
* `memory/FUNCTION-IMAGING.md` §5.1 改为「没有看图档位」；
* `specs/HANDOFF-2026-09-24-avif-decoder-and-preview.md` 的档位小节与提交表行标废弃；
* `specs/M3-W4.md`、`specs/editor-session-pipeline.md`、`specs/view-true-image.md` 同步；
* **`AGENTS.md` §2.25 / §2.26**（永久）：
  * 看图没有档位，一律原图；缩放/平移是纯视口变换，永不重出图、不弹载入；要提速只许动管线/缓存/调度；
  * 任何看图只允许一次载入；占位图只能是载入期的过渡帧，一次、被原地替换、绝不成为终态。

## 5. 门

* `cargo test --workspace`：**1510 + 134** 全绿（含契约测试按新 DTO 校准）；
* `pnpm test`（1292）、`typecheck`、`lint:colors|arch|i18n`、`build` 全绿；
* 新增 `src/components/ui/viewer/zoom.test.ts`（DPR=1/2 的 100% 口径、非法 DPR）。
* 冒烟：`pnpm smoke:ui` 需对**新构建**的实例跑（当前 dev 实例是改动前的二进制）。

## 6. 需要您做的

一次**新构建**（`pnpm debug:win`）—— 上面每一条都在 Rust / 前端两侧改了，旧二进制里还是老行为。
真机请重点看：双击 100% 的倍率（应等于 1:1）、选 issue / latest 后的尺寸与倍率是否一致、
以及缩放时**不再出现**「正在载入照片…」。
