完成时间：2026-10-10 21:19:40 +0800

# 过渡帧的有意软化：1920 预览先出图，按窗口放大倍率轻微模糊

## 依据（崔总 2026-10-10 口述）

> 「editor 中载入照片时能不能先显示 1920 尺寸的 preview 图，可以对其故意做一个轻微模糊化处理
> （因为 1920 本身尺寸就不够大，高分屏下像素本来就会被放大，我本来想依赖这点自然模糊，
> 但是也有可能现有算法下像素会被放大成马赛克，所以不如故意做个适当模糊适合当前 editor 窗口显示），
> 至少比空屏好」

## 现状核查（先量数，再看链）

- 先出 1920 预览这件事 **W1 已实现**（过渡帧 L1 = `latest-<栈指纹>` 快照；L2 = 空栈时的基准图），
  且 W2 之后快照按栈指纹命名、可靠命中。
- 马赛克是**真的**：`crates/raybend/src/render/gpu.rs` 的采样器 `mag_filter: Nearest`
  （为了 1:1 看像素），过渡帧被放大时就是块状放大 —— 与崔总描述一致。

## 改动

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 像素（纯函数） | `crates/raybend/src/display/pixels.rs` | `DisplayPixels::soften(amount)` + `soften_rgb8`：3×3 可分离核 `[w, 1−2w, w]`（`w = 0.25 × amount`），边缘复制不引黑边；`amount` 非有限 / ≤0 / 长度对不上 → 原样返回 |
| 显影线程 | `src-tauri/src/editor.rs` | `transition_soften(viewport, logical)`：倍率取 `Viewport::zoom_for_fit()`（临时换逻辑尺寸读一次再还原，不复制那套数学），`amount = clamp((scale − 1) × 0.75, 0, 1)`；`DevelopJob.transition_soften` 随换照片任务走（`merge_jobs` 保留）；`transition_outcome` 解码后应用（legacy / V2 共用） |
| 渲染线程 | `src-tauri/src/editor.rs` | `present_transition`：先 `set_soft_preview(true)` 再上传；真帧路径 `set_soft_preview(false)`；`clear_image` 复位 |
| 渲染器 | `crates/raybend/src/render/gpu.rs` | 新增 `soft_sampler`（线性放大）+ `GpuContext::set_soft_preview(bool)`；`upload_image` 按开关选采样器；`DeviceResources` / `replace_resources` / `recover` 一起带上（设备重建不丢） |

## 关键决策

1. **软化只给过渡帧**：真帧（全尺寸 / 预览档）仍走最近邻 —— 1:1 档「一个图像像素就是一个屏幕
   像素」是既定口径，不能动。对比参照、覆盖层也不动。
2. **倍率用 `zoom_for_fit`**：Fit / Fill / 1:1 / Free 四种模式的倍率数学只有视口那一份；
   helper 临时换 `image_size` 读一次再还原（不复制、不改状态，单测锁住）。
3. **顺序要求**：`set_soft_preview` 只影响**下一次上传**（绑定组在上传时重建）——
   必须先置开关再 `set_shared_image`；两处调用点都按这个顺序写，注释里记了。
4. **系数 0.75 是手感值**：1.5× 放大给约 0.37、3× 封顶 1.0。真机上觉得不够软 / 太软，
   改这一个数即可（`transition_soften` 一行）。
5. **线性采样 + 有意软化两层**：线性采样解决「块状重采样」，软化解决「1920 本身细节不足」；
   两者都只在放大（scale > 1）时有意义，缩小 / 1:1 不软。

## 验证

- `cargo test --workspace` 全绿：1478 + 7 + 131 + …（含新增
  `display::pixels::tests::soften_only_blends_neighbours_and_never_changes_size`、
  `editor::tests::transition_soften_follows_the_window_magnification`）。
- `cargo clippy -p raybend -p raybend-desktop --all-targets`：无新增告警。
- `pnpm typecheck && pnpm test && pnpm build` 全绿（本次未动前端）。
- **未经人类验证**（§2.8）：真机上「过渡帧看起来是一张软预览而不是马赛克」、
  软化程度是否合适、1:1 / 缩小档是否确实不软 —— 归崔总目视。

## 遗留 / 待定

- **L3 仍不出帧**（有编辑栈、1920 快照还没生成时保持「载入中」）—— 这是 W1 按
  「不给错版本」定的规则。现在有了软化，若崔总认可「模糊基准图也算占位」，
  可以把 L3 改成「出软化后的基准图」；这是一个独立决定，未擅自改。
- 软化只在换照片那一刻按当时的倍率算一次；过渡帧在屏幕上停留期间用户缩放，
  软化量不跟着变（真帧马上就到，不做动态重算）。
- 缩放倍率上限 `MAX_ZOOM` 很大时（例如 8×）软化封顶在 1.0（3×3 核），
  极端放大下仍会看到块状边缘 —— 这是「临时占位」可接受的取舍。
