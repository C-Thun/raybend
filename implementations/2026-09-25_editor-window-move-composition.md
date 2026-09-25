# editor 移窗镂空：分离 WebView 透明与 GPU 表面合成
完成时间：2026-09-25 17:03:11 CST

## 范围与复用

- 先读 `render_window.rs`、共用 `GpuContext`、editor 的窗口事件/渲染循环及前端视口上报。
- 本次修改 `crates/raybend/src/render/gpu.rs`、`render/mod.rs`、`src-tauri/src/editor.rs`、`src-tauri/src/spike_viewport.rs`；补充 `docs/native-viewport-coordinate-guide.md` §8。
- 沿用唯一一份 GPU 上下文，用 `SurfaceComposition` 显式区分产品不透明底板与透明 spike，未另建渲染器、未增加依赖、未改 DOM 透明链或界面设计。
- 开工时已有大量未提交修改。本次保留所有已有工作，四个相关 Rust 文件的开工快照保存在 `/tmp/raybend-gpu-move-before/`；没有提交、推送或生成发布包。
- 没有新增可触发的产品功能，因此命令注册表与默认热键不适用。

## RapidRAW 对照证据

检查的是指定本地仓库 `/home/andares/repos/refers/RapidRAW`，HEAD `f00145c1`。

- `src-tauri/tauri.conf.json` 和 `gpu_processing.rs`：透明 WebView 覆盖原生 wgpu surface，与本项目采用同一类方法。
- `gpu_processing.rs:275`：Windows 优先 `CompositeAlphaMode::Opaque`，其它情形再考虑预乘/直通透明。
- 来源提交 `dc6b578a4f5bdc9405c5bb604587b79fc4d78a6f`（2026-04-26），标题为 `try to fix alpha mode on windows`：
  https://github.com/CyberTimon/RapidRAW/commit/dc6b578a4f5bdc9405c5bb604587b79fc4d78a6f
- `lib.rs:1768` 附近仅在 resize 时重配并呈现；`lib.rs:2055` 附近的 moved/resize 处理保存窗口状态，没有用移动时持续重绘解决合成问题。
- 这是策略参考，未复制 RapidRAW 代码；按本项目已有接口独立实现。源码和提交标题不能证明 RapidRAW 在所有机器上的闪烁都已消失。

## 修复与边界

1. 原 GPU alpha 策略优先 `PreMultiplied`，候选还漏了 `Opaque`；产品错误沿用了透明窗口实验的策略。
   产品现在明确要求 `Opaque`，初始化前端底色尚未送达时也用不透明底板；不支持该模式就报告错误，不静默退回透明合成。
2. spike 继续优先透明，补全 opaque-only 能力的回退（wgpu 30 的 DX12 HWND surface 有这种能力集合）。仍使用同一份 GPU 渲染实现。
3. 移动事件只唤醒重呈现，不重配交换链。补帧本身不是 alpha 配置问题的根治措施。
4. 原渲染循环在重配后仍等 200ms，且 Timeout/Occluded 的 Skipped 会清掉 dirty，可能无后续事件就不再画。
   现在未成功呈现就保留绘制请求：重配/错误约 16ms 后重试，Skipped 按 200ms 退让，新命令可立即唤醒。
   成功后阻塞等命令，空闲不轮询或持续重绘。
5. resize 保留真实零尺寸；最小化判据改为实际 viewport 尺寸，不能检查永远保留非零旧值的 swapchain config。
6. 初始化 history 增加实际 alpha 模式，便于从 `editor_render_state` 查证。

## 已验证（冒烟）

- `cargo test -p raybend --lib surface_composition -- --nocapture`：2 通过；覆盖全部能力子集、相反枚举顺序、空能力、缺少 Opaque、透明实验与 opaque-only 回退。
- `cargo test -p raybend --lib render::`：91 通过（筛选也命中 thumbnail::render）；包含设备资源重建/丢失恢复与像素回读，运行 1.81s。日志 `/tmp/raybend-gpu-move-render-test.log`。
- `cargo test -p raybend-desktop --lib editor::tests`：24 通过、1 个真实 RAW 手动样本测试按原标记忽略；包含补帧重试→成功休眠回归。运行 0.02s。日志 `/tmp/raybend-gpu-move-editor-test.log`。
- `pnpm debug:win`：最终前端构建、Windows 主程序及 worker 编译、`check:win` 和 target 清理通过。最终 exe `C:\rb-target\raybend\debug\raybend-desktop.exe`，2026-09-25 17:00:10 CST；4 个 dist 引用资源全部匹配，worker 含 `raybend-worker-proto-v3`。日志 `/tmp/raybend-gpu-move-win-build-final.log`。
- Windows 原生命令冒烟（复用 `scripts/lib/cdp.mjs`，未执行拖动、点击、视觉判断或真实照片操作）：
  - `editor_set_viewport` → `editor_bind_renderer` → 轮询 `editor_render_state`；
  - 实际适配器 `Intel(R) Arc(TM) Graphics · Vulkan`，history 为 `Vulkan / Bgra8UnormSrgb / Opaque`；
  - 首次出帧 `drawnFrames=1`，空闲 400ms 保持 1；发送 reset 意图后为 2，再空闲 400ms 保持 2；无错误、无监督器重启；
  - 冒烟创建的 GPU 会话已 `editor_unbind_renderer` 回收，主窗口留着供人工复测；结果 `/tmp/raybend-editor-gpu-smoke.json`，脚本 `/tmp/raybend-editor-gpu-smoke.mjs`。
- `git diff --check`：通过。

## 绕过与未经验证部分

- 本机 sandbox 因 `/mnt/wslg/distro` 挂载报错，普通 exec 与 apply_patch 均无法启动；经自动批准的提权 exec 进行读取/编译，并用唯一字符串断言的 Python 局部编辑绕过。风险是不能依赖 sandbox，故限制命令到明确路径及现有构建入口。
- 临时冒烟脚本首轮误把 viewport 参数多包一层，被 IPC 校验拒绝；按现有扁平契约修正后通过，没有修改产品 IPC。
- 未做 GUI/E2E；未声称已经目视证明拖窗完全无闪烁。需要在新版 Windows 程序中确认：静置照片后快速拖动窗口、拖边缩放、最大化/恢复、最小化/恢复，以及不同 DPI 显示器间移动时，照片与底色持续存在、不透出桌面，覆盖层仍对齐。
- 已验证当前机器默认 Vulkan 路径；DX12 及其它驱动的最终视觉合成效果未经真机确认。

## 后续真机反馈（2026-09-25 17:48:17 CST）

崔总确认大幅改善：同屏内移动几乎不闪，但窗体边缘超出屏幕后移动仍会闪。
这说明 Opaque 修正有效但尚未解决全部合成问题。已继续核查 Tauri 的软件清底与 GPU
同 HWND 呈现冲突，并启动 DX12 DirectComposition 的进程级对照；详见
`implementations/2026-09-25_editor-offscreen-composition-investigation.md`。
