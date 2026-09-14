# M0-1 收尾：Windows 构建路径打通 + 设计/协助体系建立

完成时间：2026-09-15 02:52:21 CST

## 本次改动范围

M0-1 的最后三个步骤（Windows 工具链验证、UNC 构建路径实测、Windows Rust 确认），加上用户中途插入的四项文档需求（单元测试纪律、DESIGN.md、ASSISTANCE.md、辅色改色）。

## 涉及文件

| 路径 | 改动 |
| --- | --- |
| `DESIGN.md` | **新建**：视觉与配色体系（唯一事实来源）——基础色板（品牌主色 `#8DB8AA`、品牌辅色 `#EECC55`、浅色 `#F6F8F4`/深浅色 `#EDF0E9`、深色 `#202226`/浅深色 `#2A2D33`、中间色 `#B6B0AF`）、dark/light 双主题组装规则、文字颜色规则、界面风格四条规则（无毛玻璃/无阴影/小倒角/无边线）、Tailwind v4 令牌映射、对比度校验 |
| `ASSISTANCE.md` | **新建**：待人类协助事项清单 + 开工前铁律（有内容则任何任务立即停止）；当前待办 A1（美术设计）、A2（GPU 验证）、A3（RAW 样本）、A4（中间色浅底下可读性）；已完成 B1（Windows 工具链）、B2（WSLg 目视） |
| `AGENTS.md` | ① 开头加 `ASSISTANCE.md` 开工铁律（IMPORTANT 提示块）② §2 新增第 10 条（单元测试必须齐备：边界覆盖 + 秒级执行）与第 11 条（遇到 ASSISTANCE.md 有内容就停）③ §3 版本基线更新（组件原语 = Ark UI 5.39.x、图标 = Tabler 3.46.0、Rust 工具链 1.98.1）④ §4 目录树补全（ASSISTANCE/DESIGN/THIRD-PARTY-NOTICES/plans/rust-toolchain 等）⑤ §5.3 新增「构建与运行命令（已实测）」⑥ §7.6/§7.7 标注选型已定 ⑦ §10 文档索引补全 |

## 关键结论：Windows UNC 构建路径（Step 13–15）

### 工具链验证（用户已自行安装）

| 项 | 结果 |
| --- | --- |
| VS C++ 工具集 | ✅ `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools`（VS **2026** 版，非 2022——首次按 2022 路径探测显示"未找到"是探测路径错了） |
| Windows SDK | ✅ `10.0.26100.0` |
| Windows Rust | ✅ `1.98.1` MSVC；仓库的 `rust-toolchain.toml` 触发 rustup **自动安装**了同名工具链（锁定机制在 Windows 侧同样生效） |

### UNC 构建实测（踩了两个坑，都已解决）

| 坑 | 现象 | 解决 |
| --- | --- | --- |
| cmd `set` 尾随空格 | `set CARGO_TARGET_DIR=C:\rb-target\raybend & cargo` 把 `&` 前的空格算进值 → `failed to create directory 'C:\rb-target\raybend \debug'` | 弃用 cmd `set` |
| cmd 引号经 WSL 互操作丢失 | 改用 `set "VAR=..." &&` 后变量**根本没生效** → 产物全写进 WSL 仓库 `target/`（9p） | **用 `WSLENV` 从 bash 侧透传** |
| 9p 不支持 rustc 锁文件 | 增量编译在 9p 上报 `could not create session directory lock file: 函数不正确 (os error -2147024895)` | 产物必须落 Windows 本地盘（`C:\rb-target\raybend`） |

### 最终可用命令（已写入 `AGENTS.md` §5.3）

```bash
pnpm build                                # ① WSL 里产出 dist/
export CARGO_TARGET_DIR='C:\rb-target\raybend'
export WSLENV='CARGO_TARGET_DIR'          # ② 关键：跨 WSL→Windows 透传
cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build --workspace'
/mnt/c/rb-target/raybend/debug/raybend-desktop.exe   # ③ 运行
```

### 实测数据

- 首次 Windows 全量构建：**3m14s**（429 个编译单元；crates 已缓存于 Windows 本地 registry）
- 产物：`raybend-desktop.exe` 7.8MB @ `C:\rb-target\raybend\debug\`
- 运行冒烟：进程启动、存活（≈32MB 内存）、taskkill 干净退出 ✓
- WSL 仓库 `target/` 零 Windows 产物污染（中途被污染过一次，已整体清除重建）✓
- **Path B（dev server 在 WSL + Windows 连 localhost:1420）未测**：`cargo run` 不经 tauri CLI 时读的是编译期嵌入的 `frontendDist`，不走 `devUrl`；若后续迭代摩擦大，再评估在 Windows 侧 `cargo install tauri-cli` 走 `cargo tauri dev`。**选定 Path A**。

## 其他决定

- **辅色改色**：`#775588`（紫）→ `#EECC55`（金黄），用户指定，已同步 DESIGN.md 色板、令牌映射与变更记录。
- **中间色可读性问题**（实测 `#B6B0AF` 在浅底 `#F6F8F4` 上对比度仅 2.00:1，低于 WCAG AA 的 4.5:1；深底上 7.45:1 无问题）——未擅自改用户色板，登记 `ASSISTANCE.md` A4 待决。

## 验证方式（冒烟）

| 检查 | 结果 |
| --- | --- |
| `vswhere` / SDK 目录 / `rustc -vV` | 全部通过 |
| `cargo build --workspace`（Windows, UNC 源码 + C: 产物） | ✅ 3m14s |
| exe 运行 → tasklist 存活 → taskkill | ✅ |
| WSL `target/` 无 `.exe`/`.pdb` | ✅（0 个） |
| `grep -rn "775588"` | 仅变更记录一行（有意保留） |

## 遗留问题 / 下一步

1. **M0-2～M0-7 未开始**（渲染 / RAW / 存储 / 缩略图 / 前端 / 架构定稿六个 spike），按用户指示暂停，等回来处理。
2. **M1 之前有美术设计阶段**（`ASSISTANCE.md` A1）：用户提供截图与口述 → Pencil 出 `.pen`。
3. M0-3/M0-5 依赖 RAW 样本（`ASSISTANCE.md` A3），待用户提供或授权下载。
4. GPU 相关验证全部要 Windows 侧跑（`ASSISTANCE.md` A2）。
