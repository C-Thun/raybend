# M2-W1 阶段 7（前半）：渲染 spike 的 Rust 侧地基

完成时间：2026-09-17 12:39:51 CST

计划：`plans/M2.md` 阶段 7 的第 1–2 步与第 5 步的一半（7.1 / 7.2 落地，7.5 的测量装置就位）
范围：`crates/raybend/src/render/`（新增 4 个模块 + 1 个 WGSL）、依赖表、`FUTURE.md`、`THIRD-PARTY-NOTICES.md`
状态：**crate 侧完成**；Tauri 调试窗口（7.3）、界面、一键脚本（7.6）、人类检查单（7.7）**未做**，见文末

---

## 一、为什么先做这一块

`AGENTS.md` §6.1 的方案 B（webview 挖洞 + wgpu 直绘）是**返工成本极高**的一条路，
`PLAN.md` A.2 把它列为最高风险项，且这是 M0/M2 里唯一必须**人类在 Windows 真机上看**的关卡。
所以先把它能自测的部分全部自测掉，再把人叫来 —— 人的时间花在「只有人能回答」的问题上。

## 二、做了什么

### 1. `render/viewport.rs` —— 视口变换（红线 #1 / #2）

**三套坐标一处收口**：图像像素 / 物理像素 / CSS 像素，`Viewport` 是唯一持有者：

```text
physical = 洞口中心 + pan + R(rotation)·(image − 图像中心)·zoom
```

- 前端只发 **CSS 像素**的指针位置，`dpr` 由 Rust 持有并在这里换算 —— 前端不做坐标数学。
- `clip_rect`（洞口）是**摆图的基准矩形**：照片适配洞口而不是整窗（方案 B 的语义）。
- `zoom_at(anchor, factor)` 先记下锚点下的图像像素、改完 zoom 再把 pan 修回去 ——
  这一条就是 A.2 里「快速缩放是否漂移」那项（RapidRAW 踩过「差 1 像素就图跟不上鼠标」）。
- `matrix()` 给 WGSL；`hit_test_css()` 是命中测试的完整口径（洞口外、图外都返回 `None`——
  透明区不能吞事件）；`scissor()` 给 wgpu 用（**自己夹到 surface 内**：越界时 wgpu 是静默不画，不是报错）。

18 项测试，重点是：锚点在 DPR 1.0/1.25/1.5 下连续缩放 50 次后**漂移 < 0.01 像素**、矩阵与直接映射逐点一致、
洞口中心 = 适配中心、退化输入（0 尺寸/NaN/到缩放上下限）不 panic 且自查能报出来。

### 2. `render/scene.rs` —— 合成测试图（6000×4000）

把「要人肉眼判的几件事」直接画进图里：四角方位标记（四色四形状）、上缘刻度尺、边缘 1px 棋盘格、
中心十字 + 8×8 纯白块（1:1 档位的判据）、平滑渐变。生成确定性、纯 CPU。
12 项测试（尺寸/缓冲区/中心白块位置/十字臂/四角颜色与臂朝向/棋盘格相位/刻度尺/确定性/内存账 96MB）。

### 3. `render/stats.rs` —— 帧统计与自记录报告

- `FrameStats`：最近秩分位数（**不插值** —— 帧时间的意义是「多少帧超过某条线」，插值会掩盖长尾）、
  帧率按墙钟算、超预算帧占比。
- `ScenarioStats`：每个场景同时记 **帧间隔**（present→present，用户感知的那个数）与 **CPU 时长**
  （`render()` 自己花的）—— 只看帧率分不清「渲染满了」和「锁在 vsync 上」。
- `SpikeReport`：适配器/后端/驱动、表面格式与 alpha 模式、尺寸与 dpr、显示器缩放、各场景帧统计、
  设备丢失记录、视口自查问题、坐标往返偏差、以及**留给人的几项**（透明是否成立 / 1:1 是否锐利 /
  跨显示器是否错位 / 主观帧率 / 备注）。`write_to(dir)` 落 JSON + Markdown，Markdown 直接可贴进实施记录。

### 4. `render/gpu.rs` + `spike.wgsl` —— wgpu 上下文

- **不依赖 Tauri**：只吃一对裸句柄（`RawHandles`），从 Tauri 窗口取句柄是 `src-tauri` 的事。
- 句柄类型用 **`wgpu::rwh`（wgpu 自己 re-export 的 rwh）** 而不是直接依赖 `raw-window-handle`：
  A.2 把「rwh 版本不一致」列为最常见的集成失败点，而版本分叉是**运行时**才炸。实测 `tauri 2.11.5`
  与 `wgpu 30.0.1` 都吃 rwh 0.6，锁里只有一份 `raw-window-handle 0.6.2`。
- 后端由 `InstanceDescriptor::new_without_display_handle_from_env()` 读 `WGPU_BACKEND`（dx12/vulkan/gl）——
  回退实验靠它，不写死。
- 透明挖洞：prefer `PreMultiplied` → `PostMultiplied` → `Auto`；clear 用透明黑；**scissor 之外一个像素都不碰**。
- 设备丢失：`set_device_lost_callback` 记录 → `simulate_device_loss()`（`device.destroy()`）→
  `recover()` 用**同一个适配器**重建设备/表面配置/管线/绑定组/纹理并重新上传。
  `get_current_texture` 的 `Outdated/Lost` 自动重配，`Timeout/Occluded`（最小化、被遮住）当「这帧不画」而**不是错误**。
- 测试图整块 `write_texture`（96MB 一次拷完），采样放大用**最近邻**（1:1 判据要能看出一个像素）。

### 5. 依赖与文档

- `Cargo.toml`：`wgpu 30.0.1` + `pollster 0.4`（`FUTURE.md` C6 记了版本理由与升级纪律）。
- `THIRD-PARTY-NOTICES.md`：wgpu 条目从「待落定」改为 **30.0.1 落定**，新增 pollster。
- `FUTURE.md` C6：**wgpu 版本锁定与升级路径**，含 RapidRAW 降 29.0（Apple P3）前例的取舍说明，
  以及这次实测到的 wgpu 30 API 变动清单（免得下次又靠猜）。

## 三、踩到的坑（都是真错，被测试/编译器抓住）

| # | 坑 | 后果 | 怎么发现 |
| --- | --- | --- | --- |
| 1 | `matrix()` 里 NDC 的 y 方向两个系数漏了负号 | 画面上下颠倒 / 整体偏移 | `matrix_matches_direct_mapping` 逐点比对（矩阵 vs 直接映射） |
| 2 | 四角标记的「角内坐标」按标记框左边界算，没按「距自己那个角」算 | 右上/右下的方块落在框的左端，**角上一个像素都没盖住** | 四角颜色测试 |
| 3 | `resolve_inside` 先 `trim_matches('/')` 再判绝对路径 | `/etc` 被削成 `etc` 溜过去（越界检查形同虚设） | 「绝对路径应当被拒」测试 |
| 4 | `remove_empty_tree` 先删空子目录、最后卡在非空目标上 | 报错但东西少了一些（半途而废最难解释） | 「非空时一点都不动」测试 |
| 5 | 分位数用 `round(q·(N−1))` | p50 取到第 51 个样本而不是第 50 个 | 100 个样本的最近秩测试 |
| 6 | wgpu 30 的 API 与我记忆里的一批不一致 | 编译失败（8 处） | 直接读本地 registry 里的 wgpu 30.0.1 源码，不猜 |

第 1、2 条是**这次最有价值的产出**：它们在真机上表现为「图上下颠倒」和「角标不见了」，
而人只会说「看着不对」。现在它们被测试钉住了。

## 四、验证方式（全部实跑过）

```text
cargo test -p raybend                 739 passed / 1 ignored
cargo test -p raybend --lib render     72 passed（其中 viewport 18 / scene 12 / stats 16）
cargo test -p raybend-desktop           30 passed
cargo clippy --workspace --all-targets  0 warning
npx tsc --noEmit                        0 error
pnpm test                              486 passed
pnpm lint:colors / lint:arch / lint:i18n 通过
pnpm build                             通过
```

**明确没验证的**：GPU 真的画出来了没有。本机（WSL）只有 lavapipe 软件 Vulkan，
`GpuContext::new` 需要一个**真窗口句柄** —— 那要求先把 Tauri 调试窗口写出来（下一步）。
所以到目前为止，`gpu.rs` 只过了编译与静态检查，**一次真实绘制都没有发生过**。
这一点不许含糊：下次提交要带上离屏渲染的像素回读证据，再之后才是人眼在 Windows 上确认。

## 五、遗留与下一步

1. `src-tauri/src/spike_viewport.rs`：调试窗口 `label = "spike-viewport"`（`transparent: true`）+
   渲染线程 + 命令层（缩放/平移/适配/旋转/洞口开关/脚本化运动/场景采集/设备丢失/落盘报告）。
   **不动主窗口**（`PLAN.md` A.2 的窗口策略）。
2. 前端 spike 页（`?spike=1` 打开）：状态面板 + 洞口区（`pointer-events: none`）+ 场景按钮 + 人的两格填写。
3. 离屏渲染 + 像素回读（我自己的冒烟，顺便产出一张 PNG 给人看）。
4. `pnpm spike:win` 一键脚本 + `plans/M2-W1-windows-gpu.md` 人类逐项表。
5. 设计侧 0.1–0.6（`design/browse.pen` 三帧 + tiles 重画 + AppIcon 真 logo）**需要 Pencil 连接**，
   本轮实测连不上（桥能应答，但报「编辑器里没有打开的文件」）—— 已请人类重启 VSCode，等恢复后做。

## 六、设计口径上的一个决定

A.2 的检查项里，凡是**程序能算的**都放进来了（坐标往返、分位数、后端名、dpr、alpha 模式、
设备丢失恢复成败），**只把「人眼才能判的」留给表格**（透明区是不是真的透、1:1 是否锐利、
跨屏是否错位、主观帧率）。理由：人的每一次点击都贵，报告要能自己长出一半。
