# WSL 磁盘空间维护（Windows 侧操作手册）

> 为什么会有这份文档：WSL2 的 `ext4.vhdx` **只涨不缩**。在 WSL 里删文件只释放 ext4 内部的块，
> 虚拟磁盘在 Windows 上仍占着原来的大小。实测 2026-11-05：
> 文件 **229.5 GiB**，而 WSL 内部实际只用 **97 GiB** —— **132 GiB 是幽灵空间**。
> 这些空间只有 `diskpart` 的 `compact vdisk` 才能还给 C 盘。
>
> **必须在 Windows 侧操作**：收缩时 WSL 必须是关闭状态，脚本没法在 WSL 里跑。

## 一句话顺序（不能反）

```text
WSL 里：sudo fstrim -v /
      ↓
Windows：wsl --shutdown
      ↓
管理员：diskpart → attach readonly → compact vdisk → detach
```

**fstrim 必须在 `wsl --shutdown` 之前**。WSL 一关，fstrim 就没地方跑了；
而没 fstrim，compact 几乎回收不到东西（原因见 §1.5）。
如果已经关掉了 WSL，先重新打开一个 WSL 终端把 fstrim 跑了，再重来。

---

## 0. 已知路径（本机，实测有效）

| 对象 | 路径 | 2026-11-05 实测 |
| --- | --- | --- |
| WSL 主磁盘 | `C:\Users\andar\AppData\Local\Packages\CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc\LocalState\ext4.vhdx` | 文件 229.5 GiB / 内部 97 GiB |
| Docker 数据盘 | `C:\Users\andar\AppData\Local\Docker\wsl\disk\docker_data.vhdx` | 文件 38.6 GiB / 内部 2.9 GB |
| WSL swap（每次重建实例新增一个） | `C:\Users\andar\AppData\Local\Temp\<GUID>\swap.vhdx` | 每个 4–8 GB |
| WSL 崩溃转储 | `C:\Users\andar\AppData\Local\Temp\wsl-crashes\` | 单个可到 3.7 GB |
| Windows 页面文件 | `C:\pagefile.sys` | 28 GB（系统默认，非 WSL） |

> 发行版名：`Ubuntu-24.04`。如果换了发行版，用 `wsl -l -v` 查，路径里的包名也会变。

---

## 1. 先在 WSL 里做预清理（推荐）

WSL 还开着的时候，先把构建垃圾清掉 —— 这样 compact 能回收得更多。

```bash
cd ~/repos/c-thun/raybend
pnpm prune:examples                    # 删 examples 的历史 hash 版本，几秒
pnpm clean:wsl --if-stale-days 0       # 删 deps/incremental 的旧变体，1–2 分钟
```

实测效果（2026-11-05）：删 **4521 项 / 13.4 GiB**。

跳过这一步也能 compact，只是回收量少一些。

---

## 1.5 关键一步：`fstrim`（不做这步，compact 几乎无效）

**这是最容易漏、也最致命的一步。** WSL 的 ext4 挂载选项是
`rw,noatime,errors=remount-ro,data=ordered` —— **没有 `discard`**。
所以删掉的文件在 ext4 里变成空闲块，但 **vhdx 根本不知道**，仍把它们当“已占用”。
diskpart 的 `compact vdisk` 只能回收 vhdx 自己知道的空闲块，于是看起来“跑了但没效果”。

在 **WSL 里**执行（要 sudo）：

```bash
sudo fstrim -v /
```

输出形如：

```text
/: 141 GiB (151353815040 bytes) trimmed
```

2026-11-05 实测：**1 秒完成，trimmed 141 GiB**。这一句才是让后续 compact 生效的开关。

> 验证挂载选项：`findmnt -no OPTIONS /`。如果含 `discard` 就不需要手动 fstrim，但 WSL 默认没有。
> 验证底层支持：`lsblk -D`，`DISC-MAX` 非 0 即可（本机 `sdd` 为 4G）。

---

## 2. 关闭 WSL

**Windows PowerShell**：

```powershell
wsl --shutdown
```

等 5 秒，确认已经关干净：

```powershell
Get-Process vmmem,vmmemWSL -ErrorAction SilentlyContinue
```

没有输出 = 关干净了。也可以顺手退出 **Docker Desktop**（托盘图标 → Quit Docker Desktop），否则它会重新拉起 WSL 并锁住 vhdx。

---

## 3. 收缩 WSL 虚拟磁盘（diskpart）

**以管理员身份**打开 Windows PowerShell（开始菜单右键 → 终端(管理员)），输入：

```powershell
diskpart
```

进入 diskpart 后，逐行输入（路径要**带引号**）：

```text
select vdisk file="C:\Users\andar\AppData\Local\Packages\CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc\LocalState\ext4.vhdx"
attach vdisk readonly
compact vdisk
detach vdisk
exit
```

成功时 `compact vdisk` 会显示进度百分比，通常 1–5 分钟（取决于 vhdx 大小）。

---

## 4. 收缩 Docker 数据盘（可选，约 35 GiB）

同样在**管理员** PowerShell、且 **Docker Desktop 已退出**的前提下：

```powershell
diskpart
```

```text
select vdisk file="C:\Users\andar\AppData\Local\Docker\wsl\disk\docker_data.vhdx"
attach vdisk readonly
compact vdisk
detach vdisk
exit
```

要清得更多可以先进 WSL 跑一次：

```bash
docker system prune -a --volumes
```

（会删掉未使用的镜像、容器、卷 —— 确认没有要保留的再跑。）

---

## 5. 顺手清 Temp 里的 WSL 残留

WSL 每次重建实例都会在 `%LOCALAPPDATA%\Temp` 下新建一个 GUID 目录，旧的 `swap.vhdx` 不会自动删。崩溃转储也一样。

**管理员或普通 PowerShell 均可**：

```powershell
Get-ChildItem "$env:LOCALAPPDATA\Temp" -Directory |
  Where-Object Name -match '^[0-9A-Fa-f]{8}-' |
  ForEach-Object { Remove-Item (Join-Path $_.FullName 'swap.vhdx') -Force -ErrorAction SilentlyContinue }

Remove-Item "$env:LOCALAPPDATA\Temp\wsl-crashes" -Recurse -Force -ErrorAction SilentlyContinue
```

实测（2026-11-05）：清出 **20.4 GB**（3 个历史 swap + 8 GB 崩溃转储）。

---

## 6. 验证

**Windows PowerShell**：

```powershell
foreach ($p in @(
  "C:\Users\andar\AppData\Local\Packages\CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc\LocalState\ext4.vhdx",
  "C:\Users\andar\AppData\Local\Docker\wsl\disk\docker_data.vhdx"
)) {
  if (Test-Path $p) { "{0:N2} GiB  {1}" -f ((Get-Item $p).Length / 1GB), $p }
}

"C: 可用 {0:N2} GiB" -f ((Get-PSDrive C).Free / 1GB)
```

**WSL**（重新打开后）：

```bash
df -h /
```

对照关系：`df` 的 `used` 是真实数据量，Windows 上 vhdx 的文件大小应该接近它（略大一点是正常的）。

2026-11-05 基线：vhdx 229.5 GiB / 内部 97 GiB → compact 后预计 **~100 GiB**。

---

## 7. 常见问题

| 现象 | 原因 / 处理 |
| --- | --- |
| `虚拟磁盘正在使用中` / `The virtual disk is already in use` | WSL 没关干净。重跑 `wsl --shutdown`，确认 `Get-Process vmmem,vmmemWSL` 无输出；Docker Desktop 必须先退出 |
| `attach vdisk readonly` 报错 | 路径写错或没加引号；确认文件确实存在 |
| compact 跑完大小没怎么变 | 说明本来就没有多少幽灵空间（`df` 的 used 与文件大小接近），正常 |
| 磁盘已经 100%，`wsl --shutdown` 卡住 | 先清 `%LOCALAPPDATA%\Temp` 的旧 swap 与 `wsl-crashes`，再重启 WSL 试 |
| 收缩后 WSL 启动很慢 | 首次启动要重建 swap 与 metadata，正常；之后恢复 |

---

## 8. 别让它再长回来

| 场景 | 做法 |
| --- | --- |
| 跑 Rust 测试 | **`pnpm test:rust`**（= `cargo test --workspace` + 自动清 examples + 每周一次完整瘦身） |
| 跑 Windows 构建 | `pnpm debug:win`（第 ⑤ 步自动清 WSL target） |
| 发版 | `pnpm release`（收尾同样清理） |
| 手动深清 | `pnpm clean:wsl --if-stale-days 0` |
| 定期 | 每 1–2 个月按本文档跑一次 §2–§6 |

**不要做的事**：直接裸跑 `cargo test` / `cargo build` 不做收尾 —— examples 每构建一次留一份 ~220 MB 的历史版本，cargo 永不删除。实测 30 天能从 14 GB 涨回 59 GB。

---

## 9. 2026-11-05 完整实测记录

| 阶段 | C 盘可用 | WSL 内用量 | ext4.vhdx 文件 |
| --- | --- | --- | --- |
| 起始 | 30 GiB | 139 GiB | 229.5 GiB |
| 清 examples + incremental | — | 107 GiB | 229.5 GiB（不变） |
| 清 Temp 残余（20.4 GB） | 50.6 GiB | — | — |
| 修 `contract.rs` 后首次 `clean:wsl`（-13.4 GiB） | — | 97 GiB | 229.5 GiB（不变） |
| **待执行：§3 compact** | 预计 ~205 GiB | — | 预计 ~100 GiB |
