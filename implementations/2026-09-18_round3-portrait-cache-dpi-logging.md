# 第三轮：竖拍 RAW 缩略图（缓存版本）/ DPI 卡死取证准备 / RAW 排序定位进展

完成时间：2026-09-18 11:42:08 CST

范围：`crates/raybend/src/thumbnail/render.rs`、`src-tauri/src/spike_viewport.rs`、
`crates/raybend/examples/source-times-probe.rs`（新）、
`src/features/photo-grid/store.{ts,test.ts}`、`plans/M2-W1-round3-fixes.md`
状态：两条已修并验证；一条（RAW 排序）已把范围压到「需要真机日志」，证据与工具齐了

---

## 一、竖拍 RAW：根因是**缓存版本**，不是渲染代码（已修）

**实测证据**（`cargo run -p raybend --example thumb-probe -- .../P1000023.RW2`）：

```text
元数据：3904×5264（已按方向换算），方向=8
✅ 出图：288×384 …… 占位图=false          ← 高 > 宽 = 纵图 ✓ 现管线是对的
```

**所以**：`PIPELINE_VERSION = 3` 是 `2a60576`（09-17 13:25）抬的，而 RAW 的**方向处理**
是 `9efeaf0`（09-17 **13:58**）才进来的 —— 方向修复落在版本号之后 → 旧的未转向缩略图
**仍然命中 v3 签名** → 人类看到「**框是纵的、框里的图是横的**」（框的纵横比走新元数据）。

修法（步 2 与步 4 一起改）：

- `PIPELINE_VERSION` 3 → 4；`render_sig` 里三处硬编码的 `-v3` → `-v4`
  （那条 `render_sig_changes_with_pipeline_version` 测试正是用来钉这个的，抬常量忘了改字面量就会红 —— 这次真的红了）。
- 事故经过写进常量注释：**同一个改动里改渲染行为就顺手抬版本**，
  不要「先合并渲染改动、之后再抬」。

## 二、DPI 变更卡死：把「取证」做足（等人类复现）

**已知**：上一轮那个 `Surface::configure` panic 已消除（日志里两次启动都没有 panic），
所以现在是**卡住**而不是崩溃。渲染循环里的锁是分段持的、没跨 `render()`，事件通道无界 ——
**不是锁死**。

按要求（人类：设置好日志、我操作、你查日志）做了三件事：

1. **不再每帧跨线程问窗口**：`refresh_window_facts`（`inner_size` / `scale_factor` /
   `current_monitor` / `is_maximized` / `is_decorated`）改成**事件驱动**
   （`Resized` / `ScaleFactorChanged` 置脏）+ 1 秒兜底 —— Windows 拖动窗口时跑的是
   模态消息循环，每帧去碰它就是自找卡顿。
2. **等锁计时**：`lock_shared(shared, what)` 在等待 > 100ms 时打一行 ——
   渲染线程、窗口事件处理器、前端 300ms 轮询都在碰这把锁，**等锁耗时是直接探针**。
3. **阶段耗时**：`log_slow`（> 50ms 才打，同类每秒最多一行）罩住 `apply_command` 与 `render`；
   窗口事件按秒汇总打印（拖动跨屏会爆量，正好暴露「事件风暴」）。

有了这三样，人类下次拖动卡死时，`/tmp/raybend-desktop.log` 里会直接显示：
哪个阶段慢、等了多久的锁、事件有多少次。

> 人类选择**自己手拖**（不要我写自动搬窗口的脚本），所以本步的「程序化复现」不做 ——
> 这条记在这里，免得后来者以为是漏做。

## 三、RAW 排序（页面表现：同一天两个分组标题、一遍纯位图一遍纯 RAW）

### 已确证的事实

- 人类确认：现象在**导入工作流**（源目录 tiles），屏幕上就是**两个同名的分组标题**。
- **后端没问题**：新写的 `examples/source-times-probe.rs` 对着真实目录
  `C:\src\tmp\pic` 跑一遍 —— **300 个文件全部从 EXIF 读到时间**（JPG 158 + RAW 142 =
  142 个 RAW 一个不少）。我先前 python 模拟里那个「RAW 0/142」是**脚本自己的局限**
  （手写的 TIFF walker 太浅），不是应用的问题 —— 这一条先前的判断是错的，此处更正。
- **前端逻辑也是对的**：新加测试 `按时间：同名 JPG 与 RW2 补读 EXIF 后落进同一片，并且片内挨着`
  —— 兜底时间刻意把两种格式拉开 2 小时 50 分（> 1 小时阈值 → 会断成两片），
  补读 EXIF 后断言「只落一片、且 JPG 与同名 RAW 挨着」→ **通过**。

### 因此留下的缺口与下一步

后端能读、前端合并逻辑对、却仍出现两个同名标题 ⇒ 问题只可能在**真实运行时**那一段
（例如补读结果没并回来、路径对不上、或补读没被触发）。为此加了一条**控制台诊断**：

```text
[photo-grid] 补读拍摄时间：合并 N 条，有 M 条没对上（按时间分组可能因此分层）
```

只在 `M > 0` 时打印（平时不刷屏），走 `console.warn`（已按 `DESIGN.md` §11.1 标了
`i18n-exempt` 豁免）。人类下次在导入网格切到「按时间」时，这行字会直接给出答案。

## 四、验证

```text
npx tsc --noEmit                     0 error
pnpm test                            503 passed（新增 1 条：JPG/RAW 同片）
pnpm lint:colors / lint:arch / lint:i18n   通过
cargo test -p raybend                759 passed / 1 ignored
cargo test -p raybend-desktop        35 passed
cargo clippy --workspace --all-targets     0 warning
pnpm build                           通过
pnpm smoke:ui                        problems: []
pnpm check:browse                    ✓ 库非空时进浏览正常
```

**待人类验证**（本轮的交付就是「可验证的状态 + 取证手段」）：

1. 22 张纵拍 RAW 的 tiles 与双击看图方向（新产物里 `v4` 签名会让旧缓存失效、重新出图）；
2. 拖动 spike 窗口跨屏：不再卡死（若仍卡，把 `/tmp/raybend-desktop.log` 发我，
   里面有阶段耗时 / 等锁耗时 / 事件风暴三样证据）；
3. 导入网格切「按时间」：同一时间段里 JPG 与 RAW 是否挨着；若仍是两个同名标题，
   把控制台那行 `[photo-grid] 补读拍摄时间…` 发我。

## 五、遗留

- **已导入库的 `taken_at` 回填**（人类定的范围：本轮不做，单独立项）——
  `测试库` 里 8 张资产的时间全是 NULL，那是**导入时**没存下时间，与上面第三条不是同一件事。
- tiles 组件的架构（人类反馈：展示与数据供给要分离、浏览与导入共用一套、
  后续导出也会用同一模式）。`design/browse.md` 里记过「tiles 按实现同步」的口径，
  但**组件分层**这件事还没有落纸；人类还报了浏览网格「两条显眼的白色竖线」。
  这两件并成一轮「tiles 组件重构」来做，单独立计划。
