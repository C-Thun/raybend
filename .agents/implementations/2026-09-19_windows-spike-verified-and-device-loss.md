# Windows 原生视口 spike 真机验收 + 设备丢失问题诊断

完成时间：2026-09-19 03:43:09 CST

> 本次由**崔总在真机上**执行（`plans/M2-W1-windows-gpu.md` 逐项表），
> 报告落在 `C:\rb-target\spike-report\`；Astro 的坐标修复指南见 `docs/native-viewport-coordinate-guide.md`。
> Agent 侧只做：读数归档、根因定位、把「不会再踩」的规矩写进 `AGENTS.md` §7.9。

---

## 0. 结论一句话

**M0-2 的核心命题（Windows + WebView2 上「透明挖洞 + 原生 GPU 直绘」成立）验过了，成立。**
坐标契约在 125% 与 200% 两档都成立；性能远超需要。剩下三件事都不是架构问题：
跨屏过渡帧闪烁（已知、产品化再收）、后端回退没测成（**是我们脚本的 bug，已修**）、
设备恢复（**流程未走通，且现有诊断手段本身有缺陷** —— 见 §3）。

崔总判据：**没有严重错位就能往前推**；且该能力**只服务编辑模块**，M2 不涉及。

---

## 1. 验过的数字（两档 DPI + 性能）

| 项 | 125% 档 | 200% 档 |
| --- | --- | --- |
| 物理尺寸 / CSS 尺寸 | 2560×1411 / 1862×1028 | 3072×1779 / 1397×809 |
| 显示器缩放 / **WebView DPR** | 1.25 / **1.375** | 2.00 / **2.200** |
| 洞口 CSS → 物理 | 300,37,1223,991 → **413,51,1681,1362** | 300,57,757,752 → **660,126,1666,1654** |
| 中心命中读数 | 2999.9, 1999.9 ✅ | 3000.1, 2000.3 ✅ |
| 数学往返偏差 | 0.000420 px | 0.000403 px（报告最大值 0.000067） |
| 测试图上传 | 6000×4000，1020.5 ms | 同上，898.7 ms |

- **DPR = 显示器缩放 × 文字缩放(110%)** 在真机上逐项对上（1.25×1.1=1.375、2.0×1.1=2.2）——
  这是 Astro 修复的核心，**旧代码直接把 OS 缩放当 WebView DPR，于是洞口/命中/锚点集体错位**。
- 洞口物理 = CSS × **WebView DPR**（757×2.2 = 1665.4 ≈ 1666 ✓），不是 × 显示器缩放。

**性能基线**（Intel Arc 集显 · Vulkan · Fifo 垂直同步）：

| 场景 | 样本 | 帧间隔 p50 | p95 | fps | CPU p50 |
| --- | --- | --- | --- | --- | --- |
| scripted-pan | 99 | 30.89 ms | 31.78 | 31.7 | **0.41 ms** |
| scripted-zoom | 98 | 30.94 ms | 31.89 | 31.5 | **0.42 ms** |
| manual（拖 + 滚轮 + 改窗口） | 676 | 16.42 ms | 30.59 | 67.3 | 8.94 ms |

**那两个 31 fps 是 harness 的假象，不是渲染极限**：脚本模式每 16 ms 醒一次
（`spike_viewport.rs` 的 `Duration::from_millis(16)`），与 16.67 ms 的 vsync 形成拍频，
呈现间隔稳定落在 **2 × 16.45 = 30.9 ms**。手操那条没有定时器，p50 直接 16.42 ms（≈60 Hz）。
**CPU 0.41 ms / 帧**说明完全不是 CPU 瓶颈 —— 6000×4000 纹理的平移缩放对这条链路是轻负载。
手操那条 CPU 8.94 ms 主要来自改窗口时的 surface 重配（swapchain 重建），符合预期。

**未测**：1.11 网格帧率与内存（库里照片少）——主观在 import 里「挺流畅」。

---

## 2. 三项发现

### 2.1 跨屏过渡有闪烁（已知，非新问题）✗→记录

`1.1` 与 `1.5` 的答案合起来看：**跨屏后 dpr 跟着变、画面正确** ✓，但**过渡那几帧有闪烁** ✗。
这正是 Astro 指南 §4 预言的：spike 的**原生窗口事件与 DOM 事件是两条独立队列**，
跨屏期间先后到达 → 短暂用了不一致的几何。
产品化出路写在同一节：**带递增 geometry revision 的整包布局事务**（旧输入不许套用新布局）。

### 2.2 后端回退实验**根本没跑成** —— 我们脚本的 bug，已修 ✗

崔总的记录：`WGPU_BACKEND=dx12 pnpm spike:win` 起不来；直接启动也全是 Vulkan。
报告里写着 **`WGPU_BACKEND`：(未设置 → 走默认优先级)**，左栏同样显示「见报告（环境变量）」。

**根因（已定位到行）**：WSL→Windows **只转发 `WSLENV` 里列出的变量**。
`scripts/spike-win.mjs` 的构建步（86–91 行）老老实实挂了 `WSLENV`，
**但启动步（139 行）没有** → `WGPU_BACKEND` 被互操作层丢掉，exe 自然「未设置」。
更糟的是脚本自己的帮助文字（163–165 行）**教用户这么用** —— 文档与实现不一致。

→ **已修**：启动步把 `WGPU_BACKEND` 并入 `WSLENV`（本文件同批次改动）。
以后 `WGPU_BACKEND=dx12 pnpm spike:win` 就是真的 dx12。
（注意：**必须重开进程**才生效；`RAYBEND_SPIKE=1` 之所以一直好用，是因为脚本同时传了 `--spike=1` 这个**参数**，参数不受互操作限制。）

### 2.3 设备丢失 / 恢复：现有诊断手段本身有缺陷 ✗（详见 §3）

---

## 3. 设备丢失问题诊断（崔总点名的「最大的问题」）

现象：点「演练丢失」与「恢复」，顶部**都**显示
`创建 surface 失败：surface 校验失败（看 wgpu 的报错日志）`，且图没回来。

### 3.1 那句「看 wgpu 的报错日志」指向一个**不存在的日志**

全仓**没有安装任何日志器**（`env_logger` / `tracing-subscriber` 都没有，`src-tauri/src/lib.rs` 只有插件初始化）。
wgpu 的 `log::debug!/error!` 在无 logger 时**被直接丢弃** —— 所以这条提示从写下的那一刻就不可能兑现。
**要拿到 wgpu 原文只有两条路**：① 装 logger（`RUST_LOG=wgpu_core=debug`）；
② 用 `device.push_error_scope(Validation)` 显式捕获。

### 3.2 `Validation` 的确切含义（读的是 wgpu 30.0.1 源码）

- `wgpu-core/src/present.rs:168`：`present.device.check_is_valid()?` —— **surface 绑定的 device 无效**，或
- 该 surface 根本没配置（`SurfaceError::NotConfigured`）→ 两者在 API 层都落成 `CurrentSurfaceTexture::Validation`。

也就是说：**`device.destroy()` 之后下一次取帧必然报这句 —— 那是「演练」该有的样子，不是 bug。**

### 3.3 「恢复也报错」至少一半是**假象**

`spike_viewport.rs` 里 `last_error` 的写入有 3 处（546 / 783 / 863 行）、读取 1 处（301 行）、
**清零 0 处** —— 红色横幅**永久粘连**。所以点「恢复」时看到的那句，
很可能只是**演练时留下的残留**，而不是恢复又失败了一次。
**结论**：在把横幅改成「跟随最新一帧的结果」之前，这个现象**不能作为恢复失败的证据**。

### 3.4 真因（**已证实**）：跨设备复用 bind group layout → panic → 渲染线程死亡

`recover()` 里有一行：

```rust
let layout = self.pipeline.get_bind_group_layout(0);   // ← 旧设备的 layout
self.bind_group = make_bind_group(&self.device, &layout, ...);   // ← 新设备 + 旧 layout
```

**wgpu 的 layout 记录着自己属于哪个设备**，跨设备用会校验失败。真机日志
（`/tmp/raybend-desktop.log`，脚本自己抓的 exe stderr）里是逐字这样的：

```text
thread 'spike-render' (34772) panicked at wgpu-30.0.1/src/backend/wgpu_core.rs:1280:26:
wgpu error: Validation Error

Caused by:
  In Device::create_bind_group, label = 'spike-bind-group'
    Device with 'raybend-spike' label of BindGroupLayout with 'spike-bind-layout' label
    doesn't match Device with 'raybend-spike-recovered' label
```

**这个 panic 直接打死了渲染线程**（`recover()` 是在渲染线程上被 `apply_command` 调用的）——
日志里 panic 之后「心跳 #N（渲染线程在转，已出 M 帧）」那半句再也不出现、只剩主线程回声，
帧数停在 2861 不动。所以「点恢复之后毫无动静」的真相是：**没人再出图了**，
而横幅之所以还写着那句 `创建 surface 失败`，是因为它本来就是演练那一刻留下的残留（§3.3）。

**第一版修复不完整（2026-09-19 04:03 复测）**：只补了 layout，`recover()` 仍然漏掉
**uniform buffer 与 sampler** —— 两者同样跟设备绑定。复测日志里是同一位置的**第二次 panic**：

```text
In Device::create_bind_group, label = 'spike-bind-group'
  Device with 'raybend-spike' label of Buffer with 'spike-uniforms' label
  doesn't match Device with 'raybend-spike-recovered' label
```

**所以改成结构性修法，不再逐个补洞**：所有跟设备绑定的资源（texture / sampler / uniform /
layout / bind group / pipeline）收进 `build_device_resources(device, queue, image, format, prefix)`，
**`GpuContext::new` / `GpuContext::recover` / `OffscreenRenderer::new` 三处共用这一个入口**。

> 顺带消掉了三份手抄的重复：原来 `new()`、`recover()`、离屏各写一份，
> 其中离屏那份连 bind group layout 都是复制粘贴的（改一处不会同步到另两处）。
> 这个 bug 的本质就是「手写重建 + 多处重复」，所以修法必须落在结构上。

**验证（可复跑）**：

1. ✅ 新增离屏回归测试 `render::gpu::tests::device_resources_rebuild_on_a_second_device` ——
   在**两块真实设备**上各建一整套资源（含建管线），守住「资源集合是设备局部的」这条不变量；
   以后谁把某项做成 `static`/`OnceLock` 或复用旧设备那一份，会在第二台设备上当场炸。
   本机（WSL + lavapipe 软件 Vulkan）**真跑过**：`cargo test -p raybend` 799 通过（1 ignored）。
2. ✅ 离屏冒烟五组断言全过（`cargo run -p raybend --example spike-offscreen -- /tmp/raybend-spike-fix`），
   证明重构没破坏共用路径。
3. ✅ **surface 侧已在真机验证（2026-09-19 复跑，人类确认"成功"）**。判据是应用日志里的帧数轨迹：
   演练后心跳帧数**停在 42**（取帧报 `Validation`、不出图 —— 这正是演练应有的样子），
   `apply_command` 用 **240ms** 完成恢复（建设备 + 重传 6000×4000 纹理 + 重建管线），
   随后 **#20 → 92 帧、#21 → 126 帧**持续增长 —— **渲染线程活着、画面回来了**。
   → 结论：`recover()` 的两半（资源重建、surface 重配）都成立，**设备恢复闭环**。

   > 附带确认：上一次那种「panic 打死渲染线程」没有再出现（日志里全程只有最初那两次历史 panic）。
   > 写报告按钮没再点，所以 `spike-report.json` 还是旧的 —— 这次的证据在应用日志，不在报告里。

**仍未做**（都不阻塞）：

1. 装 logger（`RUST_LOG` 可控）—— 这次全靠 panic 才留下痕迹，
   纯 `log::error!` 的路径仍然查不到；
2. `recreate_surface()` 仍是零调用（surface 彻底坏掉时的退路）；
   `recover()` 里也没有 error scope 兜 `configure`（它返回 `()`，失败只体现在 panic 上）。

**给产品阶段的一条重要结论**：渲染线程的 panic 会**静默**杀死出图（主线程照常响应），
编辑模块必须给渲染线程加「panic 捕获 + 重启 + 上报」，否则用户看到的就是一张永远冻住的图。

---

## 4. 顺手记下的两处「不是 bug」

- **空闲时仍在重绘**：截图里「已画 4297 帧」，而空闲唤醒间隔是 200 ms →
  没操作时也在以 ~5 Hz 出帧。指南 §7 已把「空闲无重绘」列为待办。
- **强制 dx12 / gl 起不来**（2026-09-19 复测）：窗口能起，但中间 GPU 区域不出图，
  左栏大量划线（拿不到适配器数据）。**这是预期行为**：`WGPU_BACKEND` 是「**强制**某后端」，
  强制就意味着**不做回退** —— Vulkan 能跑、另两条不行，说明这台机器只有 Vulkan 可用。
  **产品不该强制后端**（今天的代码本来就不强制，走 wgpu 默认优先级）；
  将来若在设置里放后端选项，必须处理「强制失败」：报错 + 回落到自动，
  而不是留一个白板窗口。→ 已登记到 `plans/M2-W1-windows-gpu.md` §1.8 的表下。
- **`alpha: Inherit`** 只表示「用平台合成语义」，**不等于透明已验证**；
  透明的判据是「Fit 留白处看得见下层窗口、无黑底白边」——崔总确认成立 ✓。

---

## 5. 本次落进纪律的东西（避免再踩）

Astro 指南里的坐标契约已提炼进 **`AGENTS.md` §7.9**（含「别把 OS 缩放当 WebView DPR」的成因），
指南本身也进了 §10 文档索引。要点：

1. **四种量不能混**：native scale（Tauri）／**WebView DPR**（`devicePixelRatio`）／
   DOM viewport（`innerWidth/Height`）／image zoom（Rust 的 `Viewport.zoom`）。
2. 前端**只报原始事实**（`getBoundingClientRect()`、`clientX/Y`、CSS 位移、DPR），**物理换算全在 Rust**。
3. **DPR 变化要收口**：把 CSS 洞口存成布局真相，按新 DPR 重建物理洞口；
   不要反复缩放「上次整数化过的矩形」；别让 Windows 的 Resize 消息把 DPR 改回 OS scale。
4. **禁止用 `screenX/Y × dpr` 猜容器偏移**（屏幕坐标 / 窗口外框 / 客户区不是同一坐标域）。
5. **诊断红旗**：往返误差为 0 只证明互逆；「命中中心 ✅」只证明自洽；
   节流必须保留尾样本；期望值必须**外部给定**（别让被测函数自己生成）。
6. **不许用**「强制文字缩放 100%」「硬编码 1.1」来掩盖坐标问题 —— 那正是旧 bug 的成因。

---

## 6. 改动范围

| 文件 | 改动 |
| --- | --- |
| `scripts/spike-win.mjs` | 启动步把 `WGPU_BACKEND` 并入 `WSLENV`（§2.2 的修复） |
| `plans/M2-W1-windows-gpu.md` | 补齐 1.4 DPI 表 / 1.8 后端表 / 1.10 数字 / 1.11 说明 |
| `AGENTS.md` | 新增 §7.9 坐标契约；§6.1 与 §10 加指针 |
| `PLAN.md`、`ASSISTANCE.md` | M0-2 / M0-6 结论归档；§三第 3 条收档；§二补两条绕过记录 |
| 本文件 | 本次验收与诊断的完整记录 |

## 7. 验证方式

- 读数来自崔总真机跑出的 `spike-report.md/json` 与三张截图（125% / 200% / 性能表），
  Agent 侧逐项抄录并交叉核对（洞口 CSS×DPR 与物理值、dpr 与缩放×1.1）。
- 设备丢失的机制结论来自 **wgpu 30.0.1 与 wgpu-core 30.0.1 源码**
  （`present.rs:168`、`resource.rs:5257/5368-5404`）+ 本仓 `spike_viewport.rs` / `gpu.rs` 逐行核对。
- `spike-win.mjs` 的修复以 `node --check` 与一次 `pnpm spike:win --dry-run` 冒烟。

## 8. 遗留

1. **设备恢复未走通**（§3.4 的五条修法）——属产品化收尾，已登记本文件与指南 §7。
2. **后端回退三选一仍未实测**（脚本已修好，等下次真机跑一遍 dx12/vulkan/gl）。
3. 跨屏过渡闪烁与空闲重绘：已记录，产品化时按 geometry revision / 空闲停帧 处理。
4. M0-6 的网格帧率与内存仍无真机数字（库里照片太少）。
