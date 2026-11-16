完成时间：2026-10-10 15:33:16 +0800

# 编辑会话管道 W1：过渡帧契约（同画幅 + 版本分层 + V2 接入 + 投递可靠）

## 依据

`specs/editor-session-pipeline.md` §3（W1）。四项现象里的现象 3（切图先出「版本不对」的图 ——
一张被拉伸、一张白平衡不对）与现象 2 的一半（V2 照片没有过渡帧）。本波只动过渡帧这一条线。

## 改动范围

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/editor.rs` | `TransitionPlan` 增加 `output_size`；`transition_sources` 增加 `stack_is_empty` 分层；`transition_plan` 读编辑栈算 `output_size`；`transition_outcome` 尺寸归位 + 日志补原图尺寸；新增 `accepts_transition` / `present_transition`；`Developed` 里过渡帧走独立接受判据（不再吃 `latest_job` 闸门）；`Present` 的过渡帧块抽成共享函数 |
| `src-tauri/src/editor_working.rs` | `run()` 加 `commands` 入参；缓存重建分支里在 `decode_working_source` **之前**复用 `transition_outcome` 发帧（V2 首次有过渡帧） |
| `design/editor.md` | 追加「2026-10-10 过渡帧口径」一节（三条契约 + 没有新控件） |

## 关键决策与理由

1. **L3 不出帧**（`transition_sources`）：有编辑栈而没有 `latest` 快照时**不再退回 SOOC/RAW**。
   旧行为就是现象 3 里「一张白平衡不对」的来源 —— 那张是相机直出/RAW 内嵌预览，
   与用户编辑的版本（裁切、LUT、白平衡）根本不是一回事；宁可停在「载入中」多等一会儿。
   编辑栈为空（L2）才允许基准图，因为那时真帧就是它的中性显影，属于「近似同版本」。
2. **同画幅用 `geometry.output_size`**：`transition_plan` 在命令层读一次栈（`develop::issue_of`，
   毫秒级），把几何算好的尺寸放进计划。过渡帧 `source_width/height` 报它、
   `original_width/height` 报原图尺寸（裁切工具边界）。真帧的 `source_*` 本来就是
   `geometry.output_size` —— 两边同一个值，`set_image` 不 refit，拉伸与跳变一起消失。
3. **投递判据（`accepts_transition`）比规格收紧一格**：规格写的是
   `decode != "ready"` **或** `painted_path != path`，实现用 **AND**：
   `photo_path == path && painted_path != path && decode != "ready"`。
   理由：OR 版本在「真帧已上屏、随后又来一轮参数任务（decode 打回 loading）」时会放行旧快照，
   正好命中 §3.6 点名的最高风险「陈旧帧盖新帧」；AND 版本保留规格要挡的全部情形，
   再用 `decode != "ready"` 补掉「真帧已应用但还没绘制」的窗口。判据是纯函数、有单测。
4. **过渡帧与真帧共用一条通道、一个尺寸口径**：`present_transition` 是唯一实现
   （`Developed` 与 `Present` 都调它），没有第二份画过渡帧的代码（§2.12）。
5. **V2 只在真正要重建缓存时发过渡帧**：放在重建分支内、`decode_working_source` 之前 ——
   参数任务 / 换档位任务不该发它。W2 会继续把重建判据收敛到 `should_reload_source` 的口径。

## 验证

- `cargo test -p raybend-desktop --lib transition`：12 条（1 条 `#[ignore]` 真机手跑），全绿。
  新增/覆盖：`transition_frame_reports_the_output_geometry`（裁切 + 旋转两组数据，断言
  `source_*` = `output_size`、`original_*` = 原图）、
  `transition_delivery_survives_params_but_not_a_real_frame`（参数任务抬 `latest_job` 仍被接受；
  真帧应用后、已绘制后、换照片后三种情形被丢）、
  `transition_sources_refuse_base_images_for_an_edited_stack`（L3 空候选）、
  `transition_sources_follow_the_latest_sooc_raw_order`（L1 只剩 latest）。
- `cargo test --workspace`：123 + 2 + doctests 全绿；`cargo clippy -p raybend -p raybend-desktop --all-targets`
  无新增告警（存量告警不动）。
- `pnpm typecheck && pnpm test`（1246 通过）`&& pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build` 全绿。
- **未经人类验证**（`AGENTS.md` §2.8）：裁切照片来回切的首帧观感、V2 照片过渡帧在真机上的出现频率、
  L3 时「载入中」的等待体感 —— 需要崔总在 Windows 真实照片库上确认。

## 遗留

- 过渡帧「L1 用的 latest 快照」目前仍靠旧命名（`latest-src<sig>`，不含栈指纹）＝ 拿到的可能是
  上一次编辑的快照（落库会删、预览刷新会补，但存在时间窗）。W2 的「按栈指纹命名」落地后
  L1 才是**精确**同版本。
- `editor_set_photo` 里「有 permit 但无候选」会造一个空计划（原逻辑，保留）：它只为携带
  `TaskPermit`；显影线程会打一行「0 个候选一个都用不了」的诊断日志。
- 真机诊断行已保留并补了原图尺寸：`[editor] 过渡帧：<origin> WxH（逻辑 axb，原图 cxd）耗时 …`。
