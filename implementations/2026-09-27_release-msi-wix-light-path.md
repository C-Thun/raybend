# release 的 MSI 打包失败：32 位 light.exe 与 WSL 路径（含一条结构性死路）

完成时间：2026-09-27 16:55:23 CST

## 范围

`pnpm release … --windows` 在最后一步 `cargo tauri bundle`（MSI）失败：

```
Error failed to bundle project: `failed to run C:\Users\andar\AppData\Local\tauri\WixTools314\light.exe`
```

本次排查的临时脚本与证据留在 `C:\rb-target\_diag\`（可删）。

**最终处置（2026-09-27 崔总定）**：正式版**暂时只出 NSIS**（`scripts/lib/release-windows.mjs` 的 `targets`
去掉 `msi`）；「换 C: 本地源码镜像构建」登记为 `memory/FUTURE.md` **G22**，日后单独做。
改完当场重跑 `pnpm release patch --windows --unsigned --allow-dirty`：**EXIT=0**，
产物落到 `release-out/v0.1.1/`（安装器 + `SHA256SUMS` + `raybend-build.json` + `release-index.json` + `RELEASE-NOTES.md`），
`pnpm check:win` 也过了。

## 一句话结论

`light.exe`（WiX 3，**32 位** .NET）读不了仓库所在的 WSL 路径：`Z:` 盘符只有 64 位进程走得进去；
换成 `\\wsl.localhost\…` UNC 又被 WiX 建 cabinet 时拒掉；而 Tauri 的 `bundle.resources`
**在代码层面无法表达绝对路径**（驱动/UNC 前缀一定被剥掉），所以没有「配置改一改就好」的修法。

**NSIS 那份安装包是好的**，坏的只有 MSI。

## 三层根因（逐层剥出来的）

### 第 1 层：`light.exe` 读不了 `pushd` 造出来的 `Z:`

流程链：`pnpm release`（WSL）→ `cmd.exe /d /c …build.cmd` → `pushd "\\wsl.localhost\…"`（cmd 不能把 UNC
当工作目录，只能造盘符）→ 全树以 `Z:\…` 工作 → tauri 用**工作目录**推导项目目录，
于是 `bundle.resources` 的 `../LICENSE` 被写进 `main.wxs` 时变成 `Z:\…\src-tauri\..\LICENSE`。

证据（同一路径、同一时刻，只差进程位宽）：

| 进程 | 读 `Z:\…\LICENSE` |
| --- | --- |
| 64 位 PowerShell / cmd | ✅ `Test-Path=True`，`ReadAllBytes` 34523 字节 |
| 32 位 PowerShell / `SysWOW64\cmd` | ❌ `Test-Path=False`（但 `Z:\` 这个根看得见） |

`light.exe` 是 `PE32 (console) Intel 80386 Mono/.Net assembly` = 32 位 → 报

```
main.wxs(112) : error LGHT0103 : The system cannot find the file 'Z:\…\src-tauri\..\THIRD-PARTY-NOTICES.md'.
```

`candle.exe` 同样是 32 位却没事 —— 它只解析 `.wxs`，不读源文件。
**NSIS 之所以也没事**：`tauri-bundler` 里 NSIS 先 `copy_resources` 到本地暂存（跑在 64 位 Rust 进程里），
MSI 则把源路径**原样交给 32 位 light.exe**（`tauri-bundler/src/bundle/settings.rs` 的 `resource_files()`）。

### 第 2 层：换成 UNC 也不行，WiX 建 cabinet 时炸

把 `main.wxs` 里的 `Z:\…` 全量替换成 `\\wsl.localhost\…` 后，LGHT0103 消失（文件读到了），但出现新错误：

```
light.exe : error LGHT0001 : 系统找不到指定的路径 (HRESULT:0x80070003)
  System.IO.DirectoryNotFoundException
   at …Cab.Interop.NativeMethods.CreateCabFinish(…)
   at …Cab.WixCreateCab.Complete(…)  at …CabinetBuilder.CreateCabinet(…)
```

`light -v` 给出卡住的那一行：`Creating cabinet 'C:\Users\andar\AppData\Local\Temp\<rand>\#app.cab'`。

**对照实验（决定性）**：把两个许可文件拷到本地 `C:` 再指向它们 →
`light exit = 0`，`local.msi` **19,365,888 字节真的生成了**。
所以机器、WiX、wxs 本身都没毛病，只有「从 WSL 9p 读源文件」这一件事过不去。
UNC 去掉 `..` 再试一次（排除 `..` 干扰）→ 仍然失败，说明不是路径形态问题，是 9p/网络源本身。

### 第 3 层（死路）：`bundle.resources` 不能写绝对路径

想用「把文件暂存到 `C:\…` 再让配置指过去」绕开前两层，做了两轮实测，都被挡：

```
# 字面绝对路径
"resources": { "C:\\rb-target\\raybend-release\\stage\\LICENSE": "licenses/LICENSE" }
→ Error: resource path `/rb-target\raybend-release\stage\LICENSE` doesn't exist
                                                        ↑ C: 被吃掉

# 用 glob 走旁路
"resources": { "C:/rb-target/raybend-release/stage/*": "licenses" }
→ Error: resource path `/rb-target\raybend-release\stage\LICENSE` doesn't exist
```

原因在 `tauri-utils-2.9.3/src/resources.rs`：

```rust
fn normalize(path: &Path) -> PathBuf {
  for component in path.components() {
    match component {
      Component::Prefix(_) => {}        // ← 驱动器/UNC 前缀被直接丢弃
      Component::RootDir => dest.push("/"),
      Component::Normal(s) => dest.push(s),
```

- 非通配分支：`normalize(Path::new(pattern))` → 再 `path.exists()` → `ResourcePathNotFound`；
- **通配分支的 glob 结果也过 `normalize(&entry)`**（`next_current_iter` 里），所以 glob 也不逃不掉。

⇒ **资源路径只能是 tauri 项目目录的相对路径**。项目目录在 WSL 上 ⇒ 必然回到 `Z:` / UNC ⇒ 第 1、2 层二选一。

## 验证矩阵（都在这台机器上实跑过）

| # | 试验 | 结果 |
| --- | --- | --- |
| 1 | 64 位 vs 32 位进程读 `Z:\…\LICENSE` | 64 ✅ / 32 ❌（`Z:\` 根可见但进不去） |
| 2 | 32 位进程读 `\\wsl.localhost\…\LICENSE` | ✅ 34523 字节（**UNC 本身可读**，所以第 2 层不是权限问题） |
| 3 | `net use Z: \\wsl.localhost\Ubuntu-24.04` 建真映射 | ❌ 系统错误 67（9p 共享不是 SMB，无法做网络映射） |
| 4 | `cargo metadata --locked` 在 UNC 工作目录下 | ✅ exit 0，无 stderr（cargo 本身能跑 UNC cwd） |
| 5 | 本地 `C:` 源文件 → candle + light | ✅ **`local.msi` 19,365,888 字节** |
| 6 | UNC 源文件（原始 / 规范化去掉 `..`） | ❌ 都是 `DirectoryNotFoundException` @ `CreateCabFinish` |
| 7 | 配置里写绝对路径（字面 / glob 两种写法） | ❌ 都被 `normalize()` 剥掉前缀 |

复现脚本：`C:\rb-target\_diag\{ftest.ps1,ztest*.cmd,cabtest.ps1,normtest.ps1,bundletest.cmd}`
（`ftest.ps1` 用 `System32` / `SysWOW64` 两份 PowerShell 对照位宽，是最短的那个复现）。

## 未落地：两条出路（**需要崔总定**）

### A. 正式版只出 NSIS，`targets` 去掉 `msi`

`scripts/lib/release-windows.mjs` 现在是 `plan.channel === "release" ? ["nsis","msi"] : ["nsis"]`。
改成只 `["nsis"]` 即可，**零风险、当场可用**（NSIS 那份已经产出并通过核查）。
代价：没有 MSI（企业 GPO / `msiexec /qn` 静默部署会缺一条路）。

### B. Windows 正式版改从 **C: 本地源码镜像**构建（已登记 `memory/FUTURE.md` G22）

把仓库镜像到 `C:\rb-target\raybend-src\`（`rsync -a --delete`，仓库本体很小），从那里跑
`cargo tauri build/bundle`。这样项目目录就在 C: 上 ⇒ 资源路径天然是本地路径 ⇒ 第 1、2 层一起消失，
顺带还甩掉 `pushd`/`WSLENV` 那些互操作讲究（`debug:win` 那套 9p 读取也一并变快）。
代价：多一个镜像步骤；`CARGO_TARGET_DIR` 可沿用现有目录，但工作区 crate 的源码路径变了 ⇒
**每轮正式版要多付一次 LTO 重链**（就是这次那十几分钟）；`AGENTS.md` §5.3 那几条硬规矩要跟着改。

（还有一条已经被否掉：在 C: 上给仓库建符号链接/junction 再就地构建 ——
最终仍会落到 UNC 目标上，第 2 层照旧，且要给机器加一次性开发者模式/管理员动作，不值当。）

## 顺带发现（与本 bug 无关，但记一下）

1. **`__TAURI_BUNDLE_TYPE variable not found in binary` 警告**：我这几轮 `cargo tauri bundle` 都输出
   `Warn: Failed to add bundler type to the binary: … Updater plugin may not be able to update this package`，
   而 16:28 那次原始构建只打了 `Info Patching …` 没有警告。
   **推测**（未证实）：那个标记是**一次性**的 —— 扫描的 `__TAURI_BUNDLE_TYPE` 占位符在第一次
   打补丁时就被改写成具体类型，所以对**已经打过补丁的 exe** 再打一次必然找不到；
   我这几次正是拿 16:28 那份已打过补丁的 exe 反复跑。若是这个原因就无害，不必追；
   但下一轮**全新构建**时值得扫一眼这行还在不在（若还在，才需要怀疑自动更新链条）。
2. **`light.exe` 的错误被 Tauri 吞了**（这次难查的真正原因）：
   `tauri-bundler/src/utils/mod.rs::output_ok()` 把子进程 stdout/stderr 收进内存，
   失败只回 `failed to run <program>`；那些行只在 `log::debug!` 里 ⇒ **`cargo tauri build -vv` 才看得见**。
   下次遇到「failed to run xxx」先去拿 `-vv`，或直接手工跑那条命令。
3. **`TEMP` 猜测被证伪**：一度怀疑 WSL 侧传出非法 `TEMP`，实测 Windows 侧
   `TEMP=C:\Users\andar\AppData\Local\Temp` 正常，换干净 TEMP 重跑错误一模一样。
4. **磁盘**（2026-09-27 实测）：`C:` 剩 **14.3 GB（99% 已用）**。它**不是**本次失败的原因，
   而按 `AGENTS.md` §5.3 第 10 条跑 `pnpm clean:win` 的结论是 **「target 目录里没有失效产物，无需清理」**
   —— debug target 那 19.27 GB 全是还在构建图里/尚在宽限期的，**不是垃圾**。
   真正的吃盘户在别处：**WSL 的 `ext4.vhdx` 182.17 GB**、`AppData\Local\Temp` 6.74 GB、
   `C:\Windows\Installer` 5.62 GB、`C:\rb-target` 合计 23.26 GB（debug 19.27 + release 3.99）。
   结论：想真腾地方，要么删 debug target（代价：下次 `debug:win` 冷构建），要么清 Windows Temp，
   要么压缩 vhdx（需 `wsl --shutdown`，会中断会话）—— 三条都得崔总点头，已单独登记。

## 现状与遗留

- **正式产物已就绪**：`release-out/v0.1.1/`（17:42），1 个 NSIS 安装器 `RayBend_0.1.1_x64-setup.exe`
  （15,512,033 字节）+ 校验和/清单/说明。`pnpm check:win` 通过（exe/worker 09:42:42Z 晚于 dist 09:28:21Z，
  4 个引用资源全命中，含 `raybend-worker-proto-v3`）。
- **未做完的事（按 `AGENTS.md` §2.1 归崔总）**：上传/推送/打 tag 一律由人操作；
  且本轮是**脏树 + 无签名**（`--allow-dirty --unsigned`），发版页必须如实标明。
- **版本文件停在 0.1.1**（流程成功时**不**回滚 —— 回滚只在失败分支），这是预期行为。
- **那个 `__TAURI_BUNDLE_TYPE` 警告在**全新构建**里没再出现** ✅ —— 坐实了推测：它只是「对已打过补丁的 exe
  再打一次补丁」的产物，无害，不必追。
- 本次**没有**为 MSI 留下回归测试 —— 它需要 Windows + 一次真实 bundling，按 §2.8 属于人类 E2E；
  等 G22 落地时再考虑做成可重复执行的脚本。
- 已撤回的中间尝试（暂存到 C: 的 `stageResourceSources` + 8 条单测）没有留在仓库里，
  但它的反面结论（「绝对路径无法表达」）已写进代码注释与 G22，不会再被重新提一遍。
