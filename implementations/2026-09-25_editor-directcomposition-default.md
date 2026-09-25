# Windows 编辑视口默认 DirectComposition 与最小平台适配器
完成时间：2026-09-25 18:16:50 CST

## 范围与复用

- 将已获真机确认的 DX12 + DirectComposition visual + Opaque 固化为 Windows 编辑器默认，
  不再要求特殊启动参数。整个编辑视口 wgpu 上下文走 DX12，复用原有 Rust/wgpu/WGSL 管线。
- 动手前检查 `render/` 与 `render_window.rs`：已有唯一 `GpuContext` 和窗口胶水，
  没有平台呈现策略入口。因此新建 `render/presentation.rs` 只容纳配置选择，不复制渲染器。
- 接到“极其简单的 adapter”要求后，将临时放在 `SurfaceComposition` 的后端默认抽出，
  避免 alpha 策略隐式决定后端。新适配器仅约 40 行生产代码，一个枚举、一个产品选择入口、一个配置出口。
- 没有新增依赖、前端功能、存储格式或 UI；不适合接入命令面板/快捷键，因为这是内部呈现策略，
  不提供用户动作，诊断仍复用 wgpu 环境变量。

## 接口与职责

- `PresentationAdapter::for_editor()`：Windows → `WindowsComposition`；其它平台 → `PlatformDefault`。
- `WindowsComposition`：`Backends::DX12` + `Dx12SwapchainKind::DxgiFromVisual`。
  不自动退回存在同 HWND 争用的旧路径；初始化错误继续通过现有编辑器状态与监督器上报。
- `instance_descriptor()`：默认配置后 `.with_env()`，保留显式
  `WGPU_BACKEND` / `WGPU_DX12_PRESENTATION_SYSTEM` 的诊断优先级。
- `SurfaceComposition` 仅管 alpha 与初始底色；editor 传 `Opaque`，spike 传 `Transparent`。
- editor/spike 都调用同一个 `GpuContext`；spike 显式传 `PlatformDefault`，离屏渲染初始化不变。
- `SurfaceDetails.presentation` 与 editor history 增加实际呈现系统，能区分
  `DxgiFromHwnd` 与 `DxgiFromVisual`。非 DX12 路径记录 `RawHandle`。
- macOS/Linux 只是保留入口。后续按真实平台需求补策略，必要时扩展现有窗口 glue；
  不预建 trait、插件注册表或第二个 GPU 生命周期，也不宣称跨平台已完成。

## 文件

- 代码：`crates/raybend/src/render/{presentation,gpu,mod}.rs`、
  `src-tauri/src/{editor,spike_viewport}.rs`。
- 文档：`ARCHITECTURE.md` §2.1 定义接口边界；`docs/native-viewport-coordinate-guide.md` §8
  补充透明度、呈现归属、调度三者的经验；`AGENTS.md` §6.1 固化纪律；
  `PLAN.md` 更新产品方案并修正“只支持 Vulkan”的过度推断；`FUTURE.md` C9 登记已采用方案。
- 根因证据及人工确认继续引用
  `implementations/2026-09-25_editor-offscreen-composition-investigation.md`，不重写历史试验结论。

## 已验证（冒烟）

- `cargo test -p raybend --lib render::`：94 passed，0 failed，0 ignored，运行 1.75s；
  含适配器三个策略回归、所有 alpha 能力子集/顺序、设备资源恢复与视口数学。
- `cargo test -p raybend-desktop --lib editor::tests`：24 passed，0 failed，1 ignored，运行 0.01s；
  跳过项为既有真实 RAW 样本/worker 手工测试。
- 新模块 `rustfmt --check --edition 2024` 与 `git diff --check` 通过。
- Windows 原生 `cargo test -p raybend --lib render::presentation::tests`：3 passed，0 failed，
  运行 0.00s；覆盖 Windows 目标的产品选择，以及 DX12/Visual 组合策略。
- `pnpm debug:win`：前端构建、Windows `raybend-desktop` + `raybend` 构建、产物核对通过；
  exe 为 2026-09-25 18:14:40 CST，4 个 dist 引用资源全部命中，worker 含 `raybend-worker-proto-v3`。
  前端仅有既有大 chunk 提示；构建脚本的 target 清理没有发现需删除的旧产物。
- 最终新产物不带 WGPU 后端覆盖启动：命令冒烟确认
  `Intel(R) Arc(TM) Graphics · Dx12 / Bgra8UnormSrgb / Opaque / DxgiFromVisual`，
  ready，累计出帧 19，无错误、无重启。检查时已有照片会话，因此只读状态，
  没有重设视口、发 reset、unbind 或关闭窗口；本次没有重测该活动会话的休眠时序。
  结果 `/tmp/raybend-presentation-smoke.json`，脚本 `/tmp/raybend-presentation-smoke.mjs`。
- 抽取适配器前，固化默认的 Windows 冒烟已在无 WGPU 覆盖时得到
  `Intel Arc / Dx12 / Bgra8UnormSrgb / Opaque / DxgiFromVisual`；
  首帧 1 → 空闲 1 → reset 后 2 → 再空闲 2，无错误、无重启。
  抽取后的新二进制验证另行记录，不拿旧产物冒充本轮结果。

## 验证障碍与边界

- 常规沙箱执行因 WSLg 的 host mount 路径失败，本轮通过受审批的命令执行继续读写与测试，
  没有改变主机挂载或权限设置。
- 首次 `pnpm debug:win` 由既有可见窗口 PID 30864 锁住 exe，标准脚本预检拒绝覆盖；
  未杀掉会话，先继续运行不依赖关窗的测试与文档审查；崔总回复“已关闭”后重建成功。
- 最终命令冒烟的首次尝试在 WebView 原生接口尚未就绪时触发身份断言；只读检查确认主窗口
  随后正常就绪，临时脚本改为等待接口，而非只依赖固定启动延时。在同一个新实例重试成功。
  未修改产品代码来绕过冒烟，也未把这次脚本时序误报当作渲染失败。
- 人工 E2E：崔总已确认此前相同 DX12 + DirectComposition 方案解决原越屏场景。
  本轮 Agent 仅做编译、单元和命令冒烟；未拖动 GUI、未验证其它显卡/驱动或 macOS/Linux。
