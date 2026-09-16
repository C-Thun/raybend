# 真机事故：导入 panic 与「卡死没有出口」

完成时间：2026-09-16 14:50:07 CST

## 事故现场（人类在 Windows 上点的）

```text
thread 'tokio-rt-worker' panicked at tauri-2.11.5/src/lib.rs:734:7:
state() called before manage() for raybend_desktop_lib::import::ImportBatches
```

表现：点「导入」→ 进度弹窗**开了但是空的**（没有进度条）、暂停/取消全是禁用态；
点「取消导入」→ 确认 → **又弹回同一个窗口**，出不去。

## 根因（两层，都得修）

### ① 直接原因：状态没注册

`src-tauri/src/lib.rs` 的 builder 只 `.manage()` 了 `db::DbState` 与 `thumbs::SourcesThumbs`，
**漏了 `import::ImportBatches`** ✗。于是任何 `app.state::<ImportBatches>()` 一调用就 panic，
而**编译期不会报** —— 只有真机上点到那条命令才炸。

修：`.manage(import::ImportBatches::default())` ✓

### ② 为什么「连取消都不行」：前端没有出口

Tauri 命令 panic 之后，**前端那个 promise 永远不会 settle**（不是 reject，是静默悬着）。
于是 `begin()` 卡在 `await start()` 上：`busy` 永远是 true（按钮全禁用）、
`progress` 永远是 null（弹窗空白）、连「取消」也因为同样原因发不出去 ✗✗。

修（三件事）：

1. **命令时限**：新增 `src/lib/timeout.ts`（`withTimeout` + `TimeoutError`），
   导入 store 的每条命令都套上（默认 15 秒；演示里 300ms）——
   时限的作用不是「让慢的变快」，而是**把「卡死」变成「报错」**；
2. **错误态可退出**：`RequestCancel` 在 `store.error() !== null` 时**直接关窗**，
   不再去「取消」一个可能已经死掉的批次；底部按钮这时只给「关闭」（不给「取消导入」）；
   暂停/继续在错误态禁用（点了一定失败）；
3. **预检也限时**：`import_precheck` 同样会挂住按钮，一起套时限。

## 防复发（两道，都是可执行的）

1. **状态注册守门测试** `cover` `every_state_type_is_managed`（`src-tauri/src/lib.rs`）：
   扫描本 crate 源码里所有 `app.state::<T>()` 与 `.manage(...)`，要求前者都有后者。
   **反向验证过**：删掉 manage 那一行，测试立刻红并指名 `ImportBatches` ✓。
   （写这个测试时自己踩了两个坑：按「第一个右括号」切会把 `Type::default()` 切坏 → 改成括号配平；
   `Filter` 不是 `DoubleEndedIterator` → 先 collect ✓。）
2. **逃生路径的冒烟断言**：画廊演示加「模拟后端挂掉」（返回一个永远不 settle 的 promise
   + 300ms 时限），`pnpm smoke:ui` 断言：**报错文案出现 + 底部变「关闭」+ 点它能关掉** ✓。

## 验证

```text
cargo test -p raybend-desktop        → 22 通过（含新的守门测试）
pnpm test                            → 398 通过（含 withTimeout 的 6 条单测）
pnpm smoke:ui                        → problems: []，其中：
   deadBackend: { showsError: true, closeOffered: true, cancelHidden: true, closed: true }
   contentText: "导入中 导入命令没有在 0 秒内回应（后端可能已经挂了） … 关闭"
Windows 产物：/mnt/c/rb-target/raybend/debug/raybend-desktop.exe（14:49 构建）
```

## 遗留

+ 时限是 15 秒：**正常的慢操作**（比如超大目录的预检）如果超过 15 秒也会被判定成错误。
  目前所有命令都是毫秒级，所以这条线够宽；将来若真出现「合法地慢」的命令，
  要给它单独放宽而不是取消时限。
+ `import_precheck` 失败（含超时）时仍**静默继续**（原设计：预检失败不该拦住导入）——
  现在有了时限，这条路径至少不会卡住按钮了。
