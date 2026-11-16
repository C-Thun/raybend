# M2 收口推进：perf-win 真机采样脚本 + 画布补两帧 + 欠账核对

完成时间：2026-09-21 02:16:40 CST

## 改动范围

M2 收口中 Agent 侧能推的全部事项（人类 2026-09-21 指示：只推 M2 收口，不开 M3——M3 等界面规划）：

1. **`scripts/perf-win.mjs`（新增）+ `pnpm perf:win`**（W3 计划 6.4，M2 最后一项编码活）：
   - 真机采样：进浏览 → 选目录 → 程序化滚动 3s 采 rAF 帧间隔 → 开筛选点「3 星」测端到端
     （真 IPC + SQLite + 绘制）→ JS 堆 / DOM 节点 / 视口 / DPR，报告落
     `/mnt/c/src/tmp/perf-win-<时间戳>.json`（`PERF_WIN_REPORT` 可覆盖）。
   - `--launch`：脚本自己经 WSLENV 带
     `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333 --remote-debugging-address=0.0.0.0'`
     拉起 exe（AGENTS §5.3 第 2 条的 WSLENV 纪律）。
   - 连接：WSL NAT 下 `localhost` 不通 Windows 侧，依次试 `PERF_WIN_HOST` → `127.0.0.1` → 默认网关
     （`ip route show default`）；`0.0.0.0` 是给 WSL 从网关进来的（Chromium 调试口默认只听 127.0.0.1）。
   - 界面语言两套都认（aria-label 同时匹配 zh/en 语言包现值）；60fps 只上报不判据（归人类，§2.8）。
2. **`scripts/lib/cdp.mjs`**：`connectCdp` 增加 `host` 选项（默认 127.0.0.1 不变），连接前把 DevTools
   报的 ws 地址改写成实际 host（否则跨机连不上）。
3. **`scripts/lib/perf-probes.mjs`（新增）**：滚动采样 / tile 状态 / 视口三支页内探针抽成共享——
   无头（`perf:browse`）与真机（`perf:win`）用**同一份探针**，数字才可比（§2.12 一份实现）。
   `perf-browse.mjs` 改为引用它；并导出 `MOCK`、加 main-guard（被 import 时只提供假后端，供探针验证）。
4. **Pencil 补两帧**（W3 遗留：菜单 / 快捷键设置画布上没有）：
   - `Components / 菜单（标题栏）`（`fOm2i`）：五菜单 + 「编辑」展开态（项 = 名字 + 键位提示，
     指向态辅色底、「移除选中」演示不可用暗态）；
   - `Components / 快捷键设置`（`P1FF5h`）：分组行（命令 + 键位芯片）、捕获态（按下新键…）、
     改过键的行（恢复默认 + 原键对照）、非阻断共享提示、内建键位说明、底部导入/导出/恢复默认/取消/保存。
   - 文案与键位取自命令注册表与语言包现值（反向同步，§5.3 判据）；截图核对通过；
     PNG 导出在 `C:\src\tmp\raybend-canvas-2026-09-21\`（`fOm2i.png` / `P1FF5h.png`）。
   - `design/browse.md`：帧清单加两行 + 新增 §5.5 批次记录。
5. **`plans/M2-W2-pending.md` 重写**：逐条对过代码后按其自身纪律清账——已完成/已转移的删段留对照表
   （PhotoGrid 合并 ✅、浏览右栏 ✅、库卡片 ✅、工具条已按插槽收敛 ✅、重建进度事件 ✅、
   EXIF 作者自动带出 ↪ FUTURE「库级默认 author」）；**还开着的只剩**：左列目录树两份（重构债务，
   动前先写方案）、film 像素锚定（等人类）、标签保存真机验证（归人类）。
6. **`plans/M2-W3.md`**：阶段 6 现状注更新，6.4/6.5/6.6 勾选（6.4 注明真机运行待人类）。
7. **`PLAN.md`**：M2-W3 状态行改为「编码全部完成，仅剩人类真机收口」；M2→M3 闸门改为
   「Agent 侧收口项已全部做完，下一步由人类主导；**M2 正式关门前不开 M3**（人类 2026-09-21 定）」；
   设计阶段表 browse.pen 状态改为已反向同步。

## 关键决策与理由

1. **真机 60fps 用什么库**：屏幕上就几十个 tile，帧率取决于 GPU 合成而非库大小——直接用平时那个库即可；
   筛选计时的硬判已在无头 10 万条（`perf:browse` + `query-bench`）完成，真机筛选只是端到端 sanity。
   不为跑一次采样先造 `perf-library` 合成大库（原 §7-4 的默认 b），需要时再建。
2. **dev server 重启**（02:00 前后）：跑了多天的 dev server 冷挂载实测退化到 ~14s（250 个模块逐个校验），
   `perf:browse` 原有 15s 等待卡在边界上失败。两侧都修：脚本等待放宽到 30s（鲁棒性）+ 重启 server
   （新 nohup 进程，日志 `/tmp/raybend-dev.log`）。重启后 `perf:browse` 连跑两次全绿。
3. **探针可验证性**：真机脚本没法在本机端到端跑，就把三个探针对着「无头 + dev server + 假后端」
   全链路验了一遍（选目录 → 滚动采样 → 筛选计时全部量到数），真机上只剩连接与 GPU 渲染两个变量。
4. **`browse.pen` 落盘捎带了另一会话的画布同步**：00:44 的「胶片带三点缩放把手」会话把 Pencil 内存画布
   的 film/compare 同步留在未保存状态（留给人类手存）；本会话经 Pencil MCP 写画布时把整份内存文档
   存回了磁盘，因此 `design/browse.pen` 的 diff 同时含它的同步与本批两帧，`design/browse.md` 含它一行
   （看图区「8px 三点拖拉条」描述）。两文件无法拆分提交，随本批一起提交并在提交信息里注明；
   **该会话的 src / AGENTS / BROWSE / DESIGN / check-browse-boot / 实施记录一律未动未提交**，
   留给它自己的提交。

## 验证

- `node --check` 四个脚本全过；`pnpm perf:win` 无 exe 时按预期打印三条指引后 exit 1。
- 探针全链路（无头 + 假后端）：选目录 `photos/2026-08-15` ✓、滚动采样 11 帧 ✓、
  筛选端到端 212.7ms 且 tile 70→49、计数「共 27272 张」出现 ✓。
- `pnpm perf:browse` 重启后连跑两次全绿：筛选数据路径 **37.5 / 39.4ms**（判据 <100ms）；
  滚动帧数字为无头软件渲染仅上报值。
- 门禁：`pnpm typecheck` 0 ｜ `pnpm test` **785** 全过 ｜ `lint:colors` / `lint:arch` / `lint:i18n` ✓。
  （未动 `src/` 与 crates，无需重建 dist / Windows 产物。）
- Pencil：两帧结构校验（bounds / 无裁剪）+ 截图目视核对 + PNG 导出成功。

## 遗留与需要人类

1. **真机跑一次**：关掉所有 RayBend 窗口 → `pnpm perf:win --launch` → 窗口拉到 2560×1440 那台屏最大化
   → 结果自动写 `perf-win.json`。连不上时按脚本提示（多半是另一个实例占着 WebView2 或防火墙）。
2. **目视验收**：M2 DoD（60fps、全键盘评片流）+ 2026-09-20 换手文档 §3.1 清单 + 两帧新画布过目
   （PNG 在 `C:\src\tmp\raybend-canvas-2026-09-21\`）。
3. **「以后再说的 bug」清单**：2026-09-20 您说有些 bug 以后再说，原文一直没拿到——请给一份，
   记进 `docs/user-requirements.md` 后逐条排。
4. 另一会话（胶片带三点把手，00:44）的 src 改动仍未提交——那是它的工作产物，本批没有代提交。
