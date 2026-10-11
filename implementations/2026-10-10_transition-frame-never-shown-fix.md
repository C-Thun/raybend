完成时间：2026-10-10 21:46:06 +0800

# 「载入照片时看不到 1920 预览」根因修复

## 崔总反馈

> 「这个功能从头到尾都没有实装过，我记得提了很多次了，每次都说做好了，但实际上从来没有。
> 我指的就是 editor 点击相片后的『正在载入照片』背景就是空的，从来没有看到过 preview……
> 仔细检查问题所在」

## 核查结论：不是没实装，是**帧被投递闸门静默丢掉**（外加 V2 根本没发帧）

### 根因 1（主因）：过渡帧和真帧抢同一个「最新任务号」，而它总是慢一步

切一张照片时前端**先后脚**发两条命令：

| 时刻 | 动作 | 渲染线程 `latest_job` | 显影线程 |
| --- | --- | --- | --- |
| t0 | `editor_set_photo` → `SetPhoto`（**带过渡帧计划**，job #N） | N | 被 `recv` 唤醒，立刻开始处理 job #N |
| t0+ε | 前端紧接着 `flush()` 参数 → `SetParams`（job #N+1） | **N+1** | 正在读 + 解 AVIF 快照（约 30–60ms） |
| t0+40ms | 过渡帧以 `Developed{id: N}` 到达 | N+1 | — |

旧代码 `Developed` 第一行就是 `if outcome.id != *latest_job { return Ok(()); }` ——
过渡帧 id = N、`latest_job` 已是 N+1 ⇒ **静默丢弃**，连日志都没有。

`merge_jobs` 只有在「两条任务都还躺在通道里」时才能把它们合成一条（id 取新的），
而那要求显影线程当时正忙（还在解上一张）。**上一张解完、通道空闲时，这条竞态 100% 输** ——
这就是「从来没有看到过」的机器行为。

> 为什么以前的单测没抓到：`transition_frame_carries_job_id_origin_and_original_size`
> 断言的是「过渡帧带的 job id 与当前任务一致」—— 那正是这条竞态会打破的前提；
> 而「一致」只在合并发生时成立。

### 根因 2：V2 照片**根本没有过渡帧**

`editor_working::run`（有 `color_state` 的照片走的 V2 路径）以前不接 `commands`、
不调 `transition_outcome` —— 凡是套过色彩管理（输入 ICC / 相机矩阵）的照片，
一条过渡帧都发不出来。

### 根因 3：快照缺失时**直接不出帧**

L3 规则（W1 定的「有编辑栈但没有 latest 快照 → 不出帧，宁可载入中」）在
「刚确认完就切走 / 上次会话中断 / 缓存被清」时命中 —— 即便前两条都好了，
这些照片仍然空屏。

## 修复

| 根因 | 修复 | 位置 |
| --- | --- | --- |
| 1 | 过渡帧**不再吃 `latest_job` 闸门**：`Developed` 里先判 `outcome.transition`，只走 `accepts_transition`（当前照片 + 还没画过 + 真帧未就绪）；被丢弃时**打日志说明原因** | `src-tauri/src/editor.rs` |
| 2 | V2 在 `decode_working_source` **之前**复用同一份 `transition_outcome` 发帧 | `src-tauri/src/editor_working.rs` |
| 3 | L3 改为**基准图兜底**（崔总 2026-10-10「至少比空屏好」）：套上编辑栈的几何（画幅与真帧一致、不拉伸）+ **强制软化 ≥ 0.35**（缩小档也看得出是临时图） | `src-tauri/src/editor.rs`（`TransitionPlan.geometry` / `stack_is_empty`） |

配套：`transition_sources` 顺序恒为 `latest → SOOC → RAW`（基准图退到兜底位）；
规格 `specs/editor-session-pipeline.md` §3.1 L3 条目同步改写。

## 新增诊断行（真机排查用）

```text
[editor] 过渡帧计划：3 个候选（preview-latest → preview-sooc → preview-raw），有编辑栈
[editor] 过渡帧：preview-latest 1920×1280（逻辑 6000×4000，原图 6000×4000，软化 0.00）耗时 42ms；候选 3 个
[editor] 过渡帧被丢弃：帧 path=…，当前 photo_path=Some(…)，painted=None，decode=loading
[editor] 过渡帧计划：没有可用候选（…）——不发帧，等真帧
[editor] 过渡帧计划：这张照片不在任何库里（…）——不发帧
```

**验收判据**：点一张照片，若几十毫秒内出现预览，日志里应有「过渡帧计划」+「过渡帧：…」两行；
若仍空屏，日志会直接指出停在哪一步（无计划 / 候选都解不开 / 被丢弃）。

## 测试

- 更新：`transition_sources_keep_base_images_as_a_last_resort_for_an_edited_stack`
  （L3 兜底候选存在）、顺序测试（latest 优先 + 基准兜底在后）。
- 新增：`fallback_transition_applies_the_stack_geometry`
  （64×48 的基准图 + 半宽裁切 → 纹理 32×48、逻辑尺寸仍报真帧的 2000×3000）。
- 既有：`transition_delivery_survives_params_but_not_a_real_frame`（投递判据的纯函数用例）。
- 门禁：`cargo test --workspace` 全绿（1478 + 132 + …）、`pnpm typecheck && pnpm test && pnpm build` 全绿。

## 遗留 / 未验证

- **未经人类验证**：真机点照片是否立刻出预览、兜底帧的「套几何 + 强制软化」观感 —— 归崔总。
- 投递闸门的旁路是**结构性**的（`Developed` 里 transition 分支在 id 判断之前），
  没有 GPU 环境写不出端到端单测；用日志行兜住真机排查。
- 基准图兜底只在「快照缺失」时出现；正常情况下 L1（栈指纹快照）仍是首选。
