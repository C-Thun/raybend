# perf-win 反向隧道：修「调试口只绑 127.0.0.1」导致连不上

完成时间：2026-09-21 03:26:10 CST

## 起因

人类真机首跑 `pnpm perf:win --launch` 失败：exe 正常拉起、WebView2 也带上了 CDP 参数，
但 WSL 侧两个候选地址都连不上。诊断（Windows 侧 netstat + 进程命令行）：

- WSLENV 转发**成功**（`msedgewebview2.exe` 命令行里 `--remote-debugging-port=9333
  --remote-debugging-address=0.0.0.0` 都在）；
- 但 **Chromium 153 无视 `--remote-debugging-address`，调试口硬绑 Windows 的
  `127.0.0.1:9333`**（headed 模式的安全限制，无头时代的老经验失效）；
- WSL NAT 下 `localhost` 不通 Windows；实测防火墙把 WSL 子网的**入站**全拦
  （中继绑 `172.24.32.1:9334` 也进不来，445/3389 同样不通）——加规则要管理员，不走。

## 解法：反向隧道（零权限）

Windows **拨出**到 WSL 不经防火墙。`scripts/perf-win.mjs` 新增：

1. 路线判定：先试直连（`PERF_WIN_HOST` / 网关）→ 不通则 `netstat`（整个输出拿回 JS 里滤，
   **不能**经 WSL sh 管道接 `findstr`——它不在 WSL 的 PATH，exec 报 127）确认 Windows 本机
   `:9333` 在听 → 起反向隧道；
2. 隧道：WSL 起 `127.0.0.1:9334` 中继；每个连入的连接临时起随机口，spawn
   `powershell.exe -EncodedCommand` 同时拨 Windows `127.0.0.1:9333` 与 WSL 的随机口，
   `CopyToAsync` 双向对拷；CDP 的 HTTP 与 WebSocket 全走这条链，`connectCdp` 无感；
3. 拨号 8 秒不到才放弃（**拨到就不再拆**——首版无条件 8s 清理把滚动探针的长连接剪断了，
   表现为「选目录成功后 Runtime.evaluate 超时」）；
4. `--launch` 先 netstat，已有调试口就复用实例，不重复拉起窗口；
5. `lib/cdp.mjs` 的 WS 错误从裸 reject ErrorEvent（打出来是 `[object ErrorEvent]`）
   改成提取 `event.message`，失败可见。

验证中顺带清掉一个文件名 bug（`slice` 把日期截成 `2026092103210`，改为全量替换后不截断）。

## 验证

- 真机全链路（人类开着的 exe + 反向隧道）：连上 Edg/153.0.4234.48 → 选目录
  `photos/2026-09-13` → 滚动采样 361 帧（p50 8.3ms ≈ 120fps，**窗口 1862×1048 非全屏，
  不能当 DoD 数字**）→ 筛选端到端 38.3ms（该目录无 3 星，为清空首屏计时）→ 报告落
  `/mnt/c/src/tmp/perf-win-2026-09-2103210.json` ✓
- `PERF_WIN_DEBUG=1` 可看中继逐连接日志（front 连入 / 拨入桥接 / powershell 退出码）。

## 遗留

- 正式的 DoD 采样仍待人类：关掉所有 RayBend 窗口 → `pnpm perf:win --launch` →
  窗口在 2560×1440 那台屏最大化 → 跑一次填结论。
- 中继每个连接 spawn 一个 PowerShell（约 0.5s），探测阶段会有几次失败重试（被 abort
  的连接 PS 以 code=1 退出，无副作用）——能收敛，不值得再优化。
