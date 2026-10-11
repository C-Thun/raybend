# 导入全线失败（IO 错误：拒绝访问 os error 5）—— part 文件刷盘用了只读句柄

**完成时间：2026-10-12 02:24:41（+0800）**

## 现象

崔总从本地 SSD 往本地 SSD 的库导入（截图 `C:\Users\andar\AppData\Local\Temp\QQ_1791741537770.png`）：
`已导入 0　跳过 0　失败 164`（该批共 306 张），错误清单每条都是
`IO 错误：拒绝访问。 (os error 5)`，来源/目标都在同一台机器的本地盘上。

## 诊断链（先落证据，再动手）

1. **截图 → 库里的行**：库是 `C:\src\tmp\testrespos`（app.db `repository_paths`），
   catalog.db 的 `import_runs` 第 7 轮 = 源 `C:\src\tmp\pic`、306 条 item、
   **`imported 0 / failed 306`**，每条的 `reason` 都是 `IO 错误：拒绝访问。 (os error 5)`。
   截图里的 `photos/2026-09-12/MY_1000018.JPG`、`photos/2018-04-30/MYAM300135.JPG` 与
   该轮 item 逐条对上 —— 是**这一轮 100% 全败**，不是零星文件的问题。
2. **来源文件本身没问题**：`Get-Item` 显示 `Archive`（非只读），`std::fs::metadata` /
   `std::fs::copy` 在真机上都能正常读它们；目标卷可写（`photos/2018-04-30` 等目录已被创建出来，
   且没有残留 `.part`）。
3. **真机探针**（PowerShell + `CreateFileW` P/Invoke，探针脚本用完即删）：

   | 句柄权限 | `FlushFileBuffers` | `GetLastError` |
   | --- | --- | --- |
   | `GENERIC_READ`（= `File::open`） | `False` | **5（ERROR_ACCESS_DENIED）** |
   | `GENERIC_READ\|GENERIC_WRITE` | `True` | `0` |

   即：**Windows 的 `FlushFileBuffers` 要求句柄带写权限**，只读句柄上直接回 `拒绝访问`。

## 根因

`crates/raybend/src/import/fsops.rs::RepoFs::copy` 的落盘那一步：

```rust
let written = std::fs::copy(source, &part)?;   // 复制到 .part，没问题
...
std::fs::File::open(&part)?.sync_all()?;       // ← File::open = 只读句柄 → os error 5
```

`File::sync_all()` 在 Windows 上就是 `FlushFileBuffers`，`File::open` 给的句柄只有
`GENERIC_READ`，于是**每一个文件**都在这一步失败（复制完了、目标名还没发布，所以磁盘上看不出痕迹）。
Linux 侧测不出来：Linux 的 `fsync` 在只读 fd 上合法。

**引入版本**：`53cc835`（2026-09-29「库存储恢复 W1–W4」）把原来的 `copy + rename` 改成
`copy → 校验 → sync → publish_new`，顺手加了这句只读句柄刷盘。崔总上次成功导入是 9-26 的
第 6 轮，之后第一次导入就是这次 —— 时间线吻合。

连带发现（同一处一起处理）：`std::fs::copy` 在 Windows 走 `CopyFileExW`，会把**只读属性一并复制**到
`.part` 上；那种 part 连写句柄都打不开（同样是 os error 5）。而且 Windows 的 `DeleteFile`
对只读文件也回 `拒绝访问` —— 只读的库内副本将来会让「真删 / 清空回收站」踩同一颗雷。

## 改动

`crates/raybend/src/import/fsops.rs`

- 新增 `sync_before_publish(part)`，`copy` 改调它：
  - 用 `OpenOptions::new().write(true)` 打开 part 再 `sync_all()`（**带写权限**，Windows 语义正确，
    Linux 上等价）；
  - 若写句柄被拒（`PermissionDenied`，即只读来源带过来的属性/写位），先 `set_readonly(false)` 再重试
    —— **库里的副本归库管**，发布前把只读去掉（顺带绕开 Windows「只读删不掉」的坑）。
- 函数头注释写清 Windows 语义与这次事故，防止再被改回去。
- 不改语义的部分：`.part` 中转 + `publish_new`（`MoveFileExW` 无覆盖发布）、失败清理、来源变化校验
  全部原样。

## 验证

| 验证 | 结果 |
| --- | --- |
| WSL `cargo test -p raybend --lib import::fsops` | 16/16 通过（含新增 `repo_fs_copy_survives_a_readonly_source`） |
| **Windows** `cargo test -p raybend --lib import::fsops`（真机语义） | 16/16 通过；`repo_fs_copy_survives_a_readonly_source` 也过 |
| **对照组**：同一条树上还原修复，再在 Windows 上跑 `repo_fs_copies_files_into_place` | **失败**：`Io(Os { code: 5, kind: PermissionDenied, message: "拒绝访问。" })` —— 与崔总截图逐字一致，坐实因果 |
| Windows 真盘端到端 `cargo run --example import-smoke`（源：含 `日本/Kyoto` 层级 + 一对 JPG/RW2 + 只含文档的目录） | ✅ 全部通过（14 项断言）：4 项导入成功、57,689,088 字节、层级透传、RAW 进 `_RAW/`、第二遍 4 项全判重跳过 |

> 隔离说明：共享工作区里另有并行会话的在途改动（`crates/raybend/src/diagnostics/*` 等），
> 当前 `cargo check -p raybend` 编不过，所以上面的编译/测试是在
> `git worktree`（HEAD + 本修复）里跑的，**没有碰共享工作区的其它文件**；验证完即删除。

## 遗留 / 归人类

1. **真机 E2E 归崔总**：重开应用把 `C:\src\tmp\pic` 重新导入 `测试库` 一次，看 306 张是否全进库。
   失败那轮只建了空的日期目录、没有留文件；重导会因判重为空而正常导入，不需要清库。
2. **本轮没做**（等 diagnostics 那批在途改动落地后）：给 `memory/ARCHITECTURE.md` §8 补一节
   「Windows 文件 API 的隐雷」（`FlushFileBuffers` 要写权限；只读文件删不掉），
   该文件此刻正被并行会话修改。
3. 修复要见到效果得重建 exe；共享工作区当前编不过（并行会话的在途改动），
   等那边落地后 `pnpm debug:win` 即可。
