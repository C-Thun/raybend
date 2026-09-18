# Windows 挖洞错位修复：WebView 有效 DPR 与系统 DPI 分离
完成时间：2026-09-19 02:32:17 CST

## 范围与结论

修复 `pnpm spike:win` 使用的现有诊断窗口，未变更产品浏览器/看图界面，未发布、推送或提交。
完整分析与后续开发规范见 [原生视口开发指导](../docs/native-viewport-coordinate-guide.md)。

主因：系统显示缩放 125%，Windows 文字大小 110%，WebView 有效像素比例为 1.375；
旧 Rust 层拿 Tauri `scale_factor=1.25` 换算 DOM 洞口、鼠标和拖动位移。
截图中的右侧约 103px 明条、底部露出桌面、十字与光标错位，都由这个比例差解释。

确认依据：

- 崔总的 `QQ_1789714051780.png`：CSS 洞口 `(300,37,524,707)`，Rust 裁剪右边 `1030`，
  DOM 实際右边约 `1133`（全图加窗口左边框约 1px）。
- 只读查询 Windows 注册表 `HKCU\Software\Microsoft\Accessibility /v TextScaleFactor`，
  返回 `REG_DWORD 0x6e`，即 110%；未修改系统设置。
- 微软 WebView2 RasterizationScale 文档解释其同时包含 DPI 与文字缩放。
- 旧报告“洞口物理值正好等于 CSS×DPR，所以单位无误”是循环论证；“CSS 1278×817”也是
  Rust 用同一个错误比例算出来的，并非浏览器实际量值。已在旧报告顶部补勘误，保留原文历史。

## 涉及文件与关键决策

- `src/dev/SpikeViewport.tsx`、`src/api/spike.ts`：原始 CSS 坐标和取样时 WebView DPR 一起传；
  布局报告真实 `innerWidth/innerHeight`；轮询比对补上仅 DPR/位置变化，失败可见。
- `src/dev/latest-sample.ts` 与同名单测：指针合并保留最后一个样本，停手也会发送；
  松手/滚轮刷新；组件卸载取消定时器；捕获取消后不残留拖动状态。
- `src-tauri/src/spike_viewport.rs`：Rust 统一使用实际 DPR 换算洞口/缩放/拖动/命中；
  native resize 不再覆盖 WebView 比例；保留 CSS 洞口真相并将关闭裁剪单独建模。
- 同文件：静止指针随视口更新重算；用原生 WebView bounds 替换 `screenX×DPR` 的伪原点诊断；
  字段名显式 `rename_all_fields="camelCase"`；增加 `spike-coordinates.json` 保存诊断原始量。
- `crates/raybend/src/render/viewport.rs`：修正 DPR 契约注释，变换数学与 shader 不动。
- `crates/raybend/examples/spike-offscreen.rs`：加入截图比例的 GPU 像素回读与缩放锚点回归。
- `plans/M2-W1-windows-gpu.md`：说明正常外观，增加三步复测，保留已填的历史答案；
  `ASSISTANCE.md` 第三节更新真机验证步骤，不新增阻塞。
- `plans/spike-coordinate-repair.md`：记录本工作单元范围与完成情况。

没有强制系统文字大小为 100%，没有硬编码 1.1，也没有给鼠标补标题栏常量。
布局与输入来源仍在前端，所有物理坐标转换仍在 Rust，符合项目分层约束。

## 已验证（冒烟）

| 命令 / 检查 | 结果 |
| --- | --- |
| `pnpm typecheck` | 通过 |
| `pnpm test` | 52 个测试文件通过，包括新增尾样本测试 |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 全部通过 |
| `pnpm build` | 通过；仍有现存大于 500KB 的 chunk 提示 |
| `cargo test -p raybend-desktop --lib` | 40 通过，含 9 个 spike 测试 |
| `cargo test -p raybend render:: --lib` | 61 通过，执行 1.60 秒（过滤也命中 thumbnail render 测试） |
| `cargo clippy -p raybend-desktop -p raybend --lib -- -D warnings` | 通过 |
| `cargo run -p raybend --example spike-offscreen -- /tmp/raybend-spike-coordinate-fix` | 5 组像素断言通过 |
| `pnpm spike:win --dry-run` | Windows debug 构建与资源核对通过，未开 GUI |
| 最后一次 Windows debug 增量构建 + `pnpm check:win` | 通过，最终源码对应 exe；4 个前端引用资源命中 |
| `git diff --check` | 通过 |

离屏回归固定像素判据：`(1120,500)` 原明条处已不透明；`(1140,500)` 洞口外保持透明；
`(773,537)` 白块在 1:1 与围绕图心放大后均保持。输出在 `/tmp/raybend-spike-coordinate-fix/`。

Windows 最终产物：`C:\rb-target\raybend\debug\raybend-desktop.exe`，
核对时 exe 时间 `2026-09-18T18:31:10.021Z`，晚于 dist 的 `18:26:26.387Z`。
第一次 Clippy 指出一处可折叠 if，已按要求改写并重新通过检查和 Windows 编译。

## 未经真机验证 / 后续边界

- 未操作 GUI 宣称通过 E2E。崔总需保持文字大小 110%，先过“复位 → 1:1 → 十字中心滚轮”三步。
- 真实桌面透明合成、跨屏瞬态、窗口状态切换、性能、设备丢失仍是原逐项表待验内容。
- 当前命令仍沿用异步队列与旧的 8ms 等待，不能当作完成确认；未来编辑模块需命令序号与
  带 geometry revision 的帧/布局快照。此轮修的是稳定坐标比例及确定的诊断问题。
- 此次没有清理旧 debug 应用进程；构建前只读确认它已退出，避免覆盖占用中的 exe。
- 当前环境没有 plannotator 工具；按崔总直接修复请求推进，未虚构评审结果，改动留待审阅。
