# 修：spike 窗口起不来（启动标记根本没接线）+ 一条会误导人的日志

完成时间：2026-09-17 18:39:19 CST

范围：`src-tauri/src/lib.rs`、`scripts/spike-win.mjs`
状态：已修并验证；人类现在可以走那张 GPU 清单了

---

## 一、现象与真因

**人类跑 `pnpm spike:win`，起起来的却是正常主界面**（标题 `RayBend`），没有那个「中间镂空、wgpu 直绘」的
调试窗口，于是 `plans/M2-W1-windows-gpu.md` 那张表一步都走不下去 —— 而脚本还在印「已经开了」。

真因**不是环境问题，是接线漏了**：

| 事实 | 说明 |
| --- | --- |
| `spike_viewport.rs` 的 `open_window()` 注释写着「**同步实现：Tauri 的 setup 钩子里（启动参数）也要用**」 | 设计上就是给 setup 用的 |
| `lib.rs` 的 `setup` 里**从来没调用过它** | 线没接 |
| `RAYBEND_SPIKE` 这个环境变量**在 Rust 侧没有任何消费者** | 只有我的脚本在设 |
| `--spike=1` 只在 `main.rs` 的一个单测里出现过 | 从没被当成真标记读过 |

结果：那个窗口**只能在开发页**（`src/dev/SpikeViewport.tsx` 的按钮）里打开，打包版够不着。

## 二、改了什么

1. **接上线**：`setup` 末尾（闪屏逻辑之后）判定启动标记，是就把窗口开出来。
   放在闪屏之后是刻意的 —— 它是调试设施，正常启动路径不该受影响。
2. **两个标记都认**（`spike_requested_from`，纯函数）：
   `--spike=1` 命令行参数 **或** `RAYBEND_SPIKE` 环境变量（非空且不为 `0`）。
   为什么两个都要：脚本经 WSL→Windows 起进程，环境变量能不能透传取决于互操作层，
   命令行参数则是硬的 —— 两个都给，哪个到了都行。
3. **单测** `spike_window_opens_only_on_an_explicit_marker`：两路各自能触发、不给标记就不开、
   形近参数（`--spike` / `--spike=0`）与 worker 标记不误判。
4. **脚本**：启动时两个标记都给；子进程 stdout/stderr 落到 `/tmp/raybend-desktop.log`
   （上一版是 `stdio: "ignore"`，失败原因只能靠猜）；提示里加了一行「窗口没出来时看哪儿」。
5. **顺手修一条会误导人的日志**：兜底线程不管实际情况都印「前端仍未报就绪」，而实际上
   前端**报过**就绪（日志里同时出现「主窗口已就绪」与「仍未报就绪」，自相矛盾）。
   现在按 `UI_READY_REPORTED` 判定后才印 —— 排障时被它带偏过一次。

## 三、验证

```text
cargo test -p raybend-desktop       35 passed（含新增的启动标记单测）
cargo clippy --workspace --all-targets   0 warning
pnpm spike:win                      构建 + 核对 + 启动，退出码 0
```

启动后**应用自己的日志**（这是「窗口到底开没开」的唯一外部证据）：

```text
[raybend] 数据底座就绪：C:\Users\andar\AppData\Local\com.cthun.raybend\app.db（位置：local）
[raybend] spike 调试窗口已打开
[raybend] 主窗口已就绪，等闪屏露满 3 秒后显示
```

**一条踩坑记录**：我最初用 `tasklist /v` 查窗口标题，只看到 `RayBend` 就判定「窗口没开」——
那是**错的**：`tasklist /v` 只显示进程的**主窗口**标题，看不见第二个窗口。
所以让程序自己写一行「已打开」，判定才有依据。

## 四、下一步

人类现在可以照 `plans/M2-W1-windows-gpu.md` 走那张表（窗口已开在桌面上）：

- 能程序算的项，spike 窗口自己会算；
- 主观项（撕裂、跟手程度、内存是否持续增长）在窗口右栏填，点**写报告**；
- 报告落在 `/mnt/c/rb-target/spike-report/spike-report.md` —— 发回来即可勾掉 W1 的最后一步（38）。
