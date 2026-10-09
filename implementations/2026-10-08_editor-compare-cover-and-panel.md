完成时间：2026-10-09 01:39:31 +0800

# 对比参照的 cover 口径 + 换展示定稿后变空 + 定稿面板瘦身

崔总 2026-10-08 报的四条（1/2/4 是上一轮 editor 对比的后续，3 是定稿面板），本轮全部落地。

## 一、参照帧改「等比 cover 填进当前画面」（bug 1 + bug 4）

**病**：参照帧与主图共用同一个 quad（同一个 uniform、同一个矩阵），参照的裁切比例与当前画面不同时
被**硬拉伸**（例：当前是未裁的 4:3，参照是裁好的 3:2；或参照是 1M 的低像素 SOOC 而当前是 2M 的 RAW）。
SOOC 那条路看着没问题，只是因为它的参照是**按当前几何裁的** —— 比例天然相同，掩盖了机制缺陷。

**口径**（崔总原话：「将对比图在不改变宽高比的情况下填充进当前图尺寸下的窗口，确保内容充满又不浪费，
只可裁切不可留空」）：

* 缩放系数 `s = max(当前画面宽/参照宽, 当前画面高/参照高)` ⇒ 两轴都不小于当前画面 ⇒ 居中放大，
  多出来的部分在窗口外被裁掉；比例永远是参照自己的（**不拉伸**）。
* 「窗口」= **当前画面的矩形**（`Viewport::image_rect()`），不是洞口：参照不会溢到画面外的留白区，
  对比线两侧几何一致（放大到铺满洞口时两者等价 —— 与旧行为一致，不是新特效）。
* 参照与当前画面比例相同时（SOOC 路径）`s` 恰好把参照放成当前画面大小 ⇒ **与旧画面零可见差异**。
  严格说不是逐位相同：`w * s` 里 f32 除法与乘法各舍入一次，实测差 **0.0002 图像像素（约 1 ULP）**
  （单测：`cover_reference_matches_the_old_shared_quad_when_the_aspect_is_the_same`）——
  旧做法是把这同一个舍入差当拉伸吸收掉，谈不上「更准」。

**实现**：

* `Viewport::matrix_for(source)`：`matrix()` 的唯一实现（画布尺寸参数化），`matrix()` 现在只是把
  当前的 `image_size` 递进去 —— 矩阵数学仍然只有一处。
* `Viewport::cover_size(source)` / `image_rect()` / `reference_scissor(left)`：纯几何 + 单测。
* `GpuContext` 多了**参照自己的 uniform**（`reference_uniform`，与主图同布局不同内容，随设备重建），
  每帧在 `write_uniforms()` 里按当前视口重写（缩放/平移一变，两个矩阵都要跟着变）；
  参照的 scissor 与主图分开算（`reference_scissor`）。
* `DeviceResources` 统一建它 —— 设备丢失重建仍然只有一个入口（本文件里记着两次真机 panic 的教训）。

**踩到并抓住的坑**：`matrix_for` 第一版把平移列的齐次分量写成了 `0.0`（该写 `1.0`）——
GPU 看到 w=0 会把整个四边形裁掉（画面全空），而**纯几何单测看不出来**（我当时用手写 `m[3][0]` 投影，
绕过了 w）。离屏像素冒烟 `device_loss_recovery_still_draws_the_right_pixels` 当场报「Fit 之后左上块在左上：
[0,0,0,0]」，用它把两块代码各自 stash/checkout 才定位到 `matrix_for`。之后：
① 修 w；② 把单测的投影助手改成**齐次（带 w 再除）**，这类错以后纯单测就能红。

## 二、对比里换「展示定稿」后画幅变空（bug 2）

**病**：对比态下点中某张 → 右栏「展示定稿」换成命名 issue（非 latest）→ 再点别的相片，
那张指定过 issue 的就变空。

**根因**：`CompareView` 的取图 effect 的**指纹只含 `id + path`**，而「展示定稿」换的是
`imageKey`（变体）—— path 一动不动 ⇒ 指纹没变 ⇒ 不给新变体 `ensureImage`。
那一格当时靠「它是当前照片」的单图槽位兜着；一旦焦点移走，`imageUrlFor()` 在缓存里查不到
（缓存里只有旧变体的条目），于是返回 `null` ⇒ `<img>` 被卸载 ⇒ 空白。

**修法**：指纹改用 **`viewerPhotoKey(photo)`**（= 存储层认的那把身份，含 `imageKey`），
与 store 的 `imageKeys` 同一份定义（不另写一份拼法）。变体一变 ⇒ 指纹变 ⇒ 重新取图。
单测补了「`viewerPhotoKey` 把展示定稿算进身份 + 指纹会变」这条契约（store.test.ts）。

## 三、定稿面板瘦身（bug 3）

* **去掉不带变量的固定说明文字**：SOOC / RAW / 最近编辑三行的说明小字（`直出 JPEG · 固定不可编辑`
  `原始 RAW 解码 · 固定不可编辑` `工作副本 · 自动保存`）**整行去掉**；语言包里对应的三个 key 一并删除。
  命名定稿那行保留元信息（它带变量：`RAW · 2026-10-08`）。
* **日期只到天、零填充**：`new Date(...).toLocaleString()` → `lib/datetime.ts::formatDay()`，
  输出 `YYYY-MM-DD`（`2026-10-08`，不再有 `2026/10/8` 这种单位数月份）。新增 `datetime.test.ts` 三条。
* **新规矩进 AGENTS.md**：§2.20「界面不加『不带变量的固定说明文字』——界面不是说明书」，
  判据是「这句话里有没有随数据变化的量」；`memory/DESIGN.md` §11.1 加了规则 6 指回它
  （那条管「文字怎么进语言包」，这条管「该不该出现」，两条不冲突）。

## 涉及文件

* Rust：`crates/raybend/src/render/viewport.rs`（`matrix_for`/`cover_size`/`image_rect`/`reference_scissor` + 4 条单测）、
  `render/gpu.rs`（参照 uniform、`write_uniform`、参照 scissor）
* 前端：`src/components/ui/viewer/CompareView.tsx`（取图指纹 + 注释）、
  `src/components/ui/viewer/store.test.ts`（身份契约）、
  `src/features/editor/panels.tsx`（去掉说明行 + `formatDay` + 改名入口沿用）、
  `src/lib/datetime.ts`（`formatDay`）、`src/lib/datetime.test.ts`（新）、
  `src/i18n/zh-CN.ts` / `en-US.ts`（删 3 个 key，加 `editor.issue.renameTitle`）
* 文档：`AGENTS.md` §2.20、`memory/DESIGN.md` §11.1 规则 6、`design/editor.md`（2026-10-08 小节）、
  `specs/M3-W5.md`（2026-10-08 修订：参照口径与手势）

## 验证

* `cargo test --workspace`：1584 条全绿（`raybend` 1454 + desktop 119 + …）0 失败
* `cargo clippy -p raybend -p raybend-desktop`：本轮无新增（剩下的 `too many arguments` 等是仓里既有的）
* `pnpm typecheck` / `pnpm test`（1232）/ `lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build`：全绿
* `pnpm smoke:ui`（临时起 `pnpm dev` 后跑完再停）：`problems: []`
* **未验证（需崔总真机）**：不同裁切/不同像素尺寸的两张图在真机上的观感（是否「充满不浪费」）、
  对比态拖图手感、定稿面板去掉说明行后的版面、改名窗与日期格式；Rust 侧要重建才生效
