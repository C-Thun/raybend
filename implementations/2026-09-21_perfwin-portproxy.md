# perf-win 连接改道：删 PowerShell 隧道，走一次性 netsh portproxy

完成时间：2026-09-21 03:52:40 CST

## 起因（人类反馈 + 复盘）

PowerShell 反向隧道（上一记录的方案）在人类机器上**持续触发 Windows Defender 告警**：
`powershell.exe -EncodedCommand <base64>` + 反向 TCP 连接是行为监控的重点攻击签名
（编码命令执行 + 反向隧道），且隧道是**每连接冷启一个 PowerShell**，所以每次都弹——
这不是误报巧合，是这条路本身踩线。加上 PS 冷启 2–5 秒（Defender 实时扫描拖慢）与
客户端超时（fetch abort 3s / undici WS 握手）赛跑，WS 握手间歇性失败。
两个毛病同根：用 PowerShell 做隧道是错的工具。人类选了**方案 A：一次性管理员授权**。

## 改动

`scripts/perf-win.mjs`：

1. **删除** `startReverseRelay` / `wslIp` / `net` 依赖 / `PERF_WIN_RELAY_IP`、
   `PERF_WIN_RELAY_PORT` 环境变量——Defender 签名路径整个移除；
2. 直连候选改为路由表：`PERF_WIN_HOST` → 网关:9333 → **网关:9334（portproxy）** →
   `127.0.0.1`（mirrored 网络）；`PERF_WIN_PROXY_PORT` 可改代理口；
3. 连不上时分流提示：调试口没开 → 关窗口重 `--launch`；口在听但直连不通 →
   打印两条一次性 **netsh** 命令（portproxy 9334→127.0.0.1:9333 + 防火墙只放
   WSL 子网 172.16.0.0/12 的 9334 入站），人类在管理员 PowerShell 跑一次后永久直连；
4. `--launch` 保留 netstat 复用逻辑（已开调试口就不重复拉起窗口）。

一次性授权命令（也打印在脚本失败信息里）：

```powershell
netsh interface portproxy add v4tov4 listenaddress=0.0.0.0 listenport=9334 connectaddress=127.0.0.1 connectport=9333
netsh advfirewall firewall add rule name="RayBend CDP (WSL only)" dir=in action=allow protocol=TCP localport=9334 remoteip=172.16.0.0/12
```

## 验证

- `node --check` 过；两条失败分支实测：
  - 调试口没开 → 提示关窗口 + `--launch`；
  - 口在听、直连不通 → 正确打印上面两条 netsh 命令。
- 直连成功路径待人类跑完授权后验证（预期走「网关:9334，portproxy」路由）。

## 遗留

- 人类在管理员 PowerShell 跑一次两条命令，然后 `pnpm perf:win`（app 若还开着会复用实例）
  在 2560×1440 全屏下采正式 DoD 数字。
- Defender 侧无需任何放行：脚本不再产生 PowerShell 子进程。
