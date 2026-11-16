# M2-W1 阶段 7（后半）：调试窗口 + spike 页 + 离屏冒烟

完成时间：2026-09-17 13:09:26 CST

计划：`plans/M2.md` 阶段 7 的 7.1 / 7.3 落地，7.4 的**观测装置**齐备，7.5 有真实数字
范围：`src-tauri/src/spike_viewport.rs`（新）、`src/api/spike.ts`（新）、`src/dev/SpikeViewport.tsx`（新）、
`crates/raybend/src/render/gpu.rs`（离屏渲染器）、`crates/raybend/examples/spike-offscreen.rs`（新）、
`src/index.tsx`、`src-tauri/{lib.rs,Cargo.toml}`、`render/spike.wgsl`
状态：**剩下 7.6 一键脚本与 7.7 人类检查单**（下一批）；设计侧 0.1–0.6 仍卡在 Pencil

---

## 一、这一批解决的核心问题

前一批结束时，渲染代码**一次真实绘制都没发生过**（WSL 只有 lavapipe，而 `GpuContext`
要真窗口句柄）。这一批把它变成：**有像素证据、有人能验的窗口、有自记录的报告**。

## 二、做了什么

### 1. Tauri 调试窗口（`src-tauri/src/spike_viewport.rs`）

- 窗口 `label = "spike-viewport"`，`transparent: true`，**按需建、不影响主窗口**
  （`PLAN.md` A.2 的窗口策略）。
- **渲染在自己的线程**：空闲时阻塞在通道上（**一帧都不画、CPU 为 0**），
  只在「脚本化运动」或「正在采集」时按 ~60Hz 循环 —— 这样「静止成本」与「运动成本」
  才是两个数，而不是量到我们的空转。
- 命令层：缩放（以光标为锚点）/ 平移 / 档位 / 旋转 / 洞口开关 / 洞口矩形 / 复位 /
  脚本化运动 / 场景采集 / 设备丢失演练 / 恢复 / 命中测试 / 落盘报告。
- 窗口事件（`Resized` / `ScaleFactorChanged`）转成命令送进线程 —— 不接这两个的话
  resize 与换屏之后 surface 与坐标全错。
- **坐标往返自查**：程序化地把一圈 CSS 点走 `CSS → 图像 → 物理 → CSS`，取最大偏差写进报告。

### 2. spike 页（`src/dev/SpikeViewport.tsx` + `src/api/spike.ts`）

- 入口：`index.html?spike=1`（**查询串**而不是路由：dev 与打包两种形态下都不影响资源解析）。
  与陈列室不同，它**必须进产物**（人类要在 Windows 打包版里验），所以是静态 import。
- 中间是**透明洞口**（无背景、靠窗口透明透出 wgpu 画的东西），四周面板不透明；
  洞口矩形由 DOM 报给 Rust（**布局权威在 CSS、变换权威在 Rust**，两者在 `clip_rect` 会合）。
- 指针事件留在 webview 里转成「意图」交给 Rust —— 这是产品架构（红线 #1），
  所以洞口**不**用 `pointer-events: none`。
- 左栏：适配器/后端/驱动、表面格式与 alpha、物理与 CSS 尺寸、dpr、显示器、窗口状态、
  视口状态、**命中测试实时值**（鼠标放白块上时应当报「命中图像中心」）、坐标往返偏差。
- 右栏：档位/旋转/洞口按钮、脚本场景、手动采集、帧统计表（帧间隔 + CPU 分开）、设备丢失演练与恢复、
  视口自查问题、**只有人能填的几项** + 「写报告」落 JSON/MD。

### 3. 离屏渲染器（`gpu.rs` 的 `OffscreenRenderer`）+ 冒烟示例

不需要窗口、不需要表面：同样的管线、同一个矩阵，画进纹理后**回读像素**。
用途写进代码注释了三条：没有真窗口的环境里能验证管线；产出可核对的像素证据；
将来导出/缩略图本来就要走离屏路径。

`cargo run -p raybend --example spike-offscreen -- /tmp/raybend-spike` 跑四组断言并落 4 张 PNG：

| 组 | 断言 |
| --- | --- |
| ① Fit | 图心落在画布中心、上下留白**完全透明**（alpha=0）、左上红/右下蓝方位标记各就各位 |
| ② 1:1 + 洞口 | 洞口中心是纯白块、**洞口之外四角与上边完全透明**（这就是挖洞）、洞口内边角有内容 |
| ③ 旋转 90° | 绕中心旋转后中心仍在中心 |
| ④ Fill | 四角都有内容（不留白） |

实测：`✅ 离屏冒烟全部通过（4 项断言组）`，PNG 见 `/tmp/raybend-spike/`。
**目视核对了 01-fit.png**：对角渐变、四角红/绿/黄/蓝标记（**无镜像无翻转**）、
左右棋盘格、顶部刻度尺都在位，上下是 letterbox；
**02-hole-1to1.png**：内容只画在洞口 (100,80,400×300) 内，外面全透明，图心白块正在洞口中心。

## 三、离屏冒烟抓到的两个真 bug（这就是它的价值）

| # | 症状（在真机上会是什么样） | 根因 |
| --- | --- | --- |
| 1 | **建管线直接失败**，spike 窗口白屏、控制台一堆 wgpu 报错 | 纹理绑定的可见性只给了 `FRAGMENT`，而顶点着色器要用 `textureDimensions` 算四角 → 必须 `VERTEX_FRAGMENT` |
| 2 | 绘制时报 `bound with size 80 where the shader expects 96` | WGSL 的 `vec3<f32>` 要 **16 字节对齐**：`mat4x4 + f32 + vec3` 实际是 96 字节，而我按 80 分配 → 改成 `mat4x4 + vec4`（64+16=80，两边一眼对得上） |

**这两个都不是「测试写得不巧」，是会在人面前炸的真错**。第 2 条尤其阴：它能编译、能建管线，
只在绘制那一瞬炸。若不是先跑离屏冒烟，就会是「崔总在 Windows 上点开 spike 窗口 → 白屏或报错」。

另外修正了两条**我自己写错的断言**（不是代码错）：Fit 档 zoom≈0.133 时图心那个 8×8 白块
只有约 1 个屏幕像素，线性缩小必然把它平均成灰（实测 [211,211,211]）——
那是**正确**的重采样。改成「中心明显发亮」口径，把「像素级纯白」交给 1:1 那一组（那里过了）。

## 四、验证方式

```text
cargo test -p raybend                739 passed / 1 ignored
cargo test -p raybend-desktop         34 passed（含 spike 的 4 项：往返偏差/命中/场景记账/采集门控）
cargo clippy --workspace --all-targets 0 warning
cargo run --example spike-offscreen   4 组断言全过 + 4 张 PNG
npx tsc --noEmit                      0 error
pnpm test                             486 passed
lint:colors / lint:arch / lint:i18n   通过
pnpm build                            通过
pnpm smoke:ui                         problems: []
```

**仍未验证的**：真窗口上的透明合成、DPI 缩放、多显示器、跨屏拖动、最大化/全屏/最小化恢复
—— 这些**只有人类在 Windows 上能判**（`AGENTS.md` §2.8）。装置已经齐了，见下一批的
`pnpm spike:win` 与 `plans/M2-W1-windows-gpu.md`。

## 五、下一步（仍在 W1 范围内）

1. 7.6 `pnpm spike:win`：构建 → 开窗（`RAYBEND_SPIKE=1`）→ 测量 → 落盘，把人的操作压到「点几下 + 填两格」。
2. 7.7 `plans/M2-W1-windows-gpu.md`：每项写清「怎么做 → 看什么 → 填什么」。
3. 8.x 收尾：质量门、Windows 产物构建 + `check:win`、实施记录、`ASSISTANCE.md` 人类执行包、文档同步。
4. **设计侧 0.1–0.6 等 Pencil**（本轮实测仍连不上：桥能应答，但报「编辑器里没有打开的文件」）。
