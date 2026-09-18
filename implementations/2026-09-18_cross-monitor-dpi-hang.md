# 跨显示器拖动卡死：根因与修复（WindowFacts 重构接完 + 两个心跳探针）

完成时间：2026-09-18 13:58:32 CST

范围：`src-tauri/src/spike_viewport.rs`、`crates/raybend/src/thumbnail/render.rs`（v5 提交）、
`ASSISTANCE.md`（A′ 判定实验）、`FUTURE.md`（A3 实测证据）
状态：根因已定位并修复；**待人类在新产物上复测**（渲染 spike 的跨屏项）

---

## 一、现象与现场证据（人类实测 + 日志）

`pnpm spike:win` 起窗口，跨显示器拖动（两侧 DPI 不同）：**窗口「未响应」，松手也不恢复**。
（上一轮同样操作是**崩溃** —— 崩溃丢现场，卡死留现场，这次能读到第一手日志。）

我在人还没关窗口时读了现场：

| 证据 | 数值 | 含义 |
| --- | --- | --- |
| 日志事件 | **只有 `Focused` / `Moved`**，最后停在一条 `Moved` 之后 | **全程没有 `Resized` / `ScaleFactorChanged`** |
| 进程状态 | `Not Responding` | 主线程没在泵消息 |
| CPU 累计 | **0:00:05**（且日志 5 秒不增长） | **等待型**，不是空转/死循环 |
| 「等锁」日志 | 0 条 | 我们埋的锁探针没响 |
| 命令路径锁纪律 | `context.resize()` 在锁**外**、锁只包字段回写 | 不是我们持锁卡住 |

## 二、根因

`WindowFacts` 上方那段文档早已写明（上一轮查出的）链条：

1. Tao/Win32 的 `inner_size()` / `scale_factor()` / `current_monitor()` / `is_maximized()` 等
   都要**跨线程回主线程**取窗口状态；
2. 拖动窗口时主线程跑的是 Win32 的**模态移动循环**（`WM_ENTERSIZEMOVE` 之后系统接管）；
3. 在这个循环里从**别的线程**发起的窗口查询**不返回** —— 不是「慢」，是**永久阻塞**；
4. 渲染线程僵住 → `shared` 统计不再更新 → 主线程任何要拿 `shared` 的路径（命令、事件回调）
   跟着僵 → `Not Responding`。

**但修复只做了一半**：`render_loop` 的签名已改成收 `initial: WindowFacts` 与
`pending_facts: Arc<Mutex<Option<WindowFacts>>>`，而**调用点没改**（仍传旧的 `facts_dirty`）、
`Shared::apply_window_facts()` **没有定义**、暂存区**没有创建**、事件回调**仍然直接锁 `shared`**。
⇒ 那个文件**从未编译通过**（`cargo check` 三条硬错），也就是说这轮修复**根本没进过产物**。

> 这是「只记了没做」的同类问题：记录写了「已改」，代码却半截。
> 我发现的方式是编译器 —— 已把更正写进 `implementations/2026-09-18_round3-portrait-cache-dpi-logging.md`。

## 三、修复（`2531bdf`）

1. **补 `Shared::apply_window_facts()`**：只搬**窗口派生**的字段（`surface_size` / `css_size` /
   `monitor_scale` / `maximized` / `fullscreen` / `decorated`）；**不碰 `viewport`** ——
   尺寸/DPR 对渲染数学的影响归渲染线程按自己的 context 走（`SpikeCommand::Resize` 里已做）。
2. **事件回调改成「主线程问、只推纯数据」**：在回调里（主线程，安全）调一次 `read_window_facts`，
   把纯数据塞进暂存区 `Mutex<Option<WindowFacts>>`，同时发 `Resize` 命令；
   **回调里不再锁 `shared`**（那是主线程跟着僵的那条路）。
3. **建窗后、渲染线程启动前**在主线程拿一次初值（`read_window_facts` 的另一个安全时刻）。
4. **删掉 `facts_dirty`**：事实改为「推送 + 暂存区」，脏标记不再需要。

**验收口径**：`render_loop` 体内**零窗口查询**（`awk` 切出函数体 grep `inner_size|scale_factor|
current_monitor|is_*|read_window_facts` → 无命中 ✓）；全文件仅剩 3 处窗口查询，全在主线程
（2 条事件回调 + 1 条初值）。

## 四、诊断心跳（下次若再卡，一句话定性）

- **渲染线程心跳**：每 5 秒报一次「还在转 + 已出多少帧」（**零窗口查询**，不破坏上面那条原则）；
- **主线程回显心跳**：独立小线程每 5 秒往主线程投一次回显（`run_on_main_thread`，
  **投递不等待**）—— 卡住时日志会停在「派发…」而永远等不到「回显」，
  这就是**主线程被原生调用堵住**的直接证据。

两者一起看，就把「日志安静」这个歧义拆成了四种组合（渲染线程活/死 × 主线程活/死）。

## 五、验证

```text
cargo check -p raybend-desktop                0 error（修复前是 3 条硬错）
cargo test  -p raybend-desktop                35 + 2 passed
cargo clippy --workspace --all-targets        0 warning
render_loop 体内窗口查询                       0 处（验收口径）
```

**未验证（归人类）**：跨屏拖动在新产物上是否还卡 —— 需要关掉当前那个「未响应」的窗口
（`Not Responding` 的窗口点关闭键是没反应的，得 `taskkill /F /IM raybend-desktop.exe`），
再跑一次 `pnpm spike:win`。心跳日志这次会直接告诉我们是哪条线程僵住。

## 六、遗留

- `ASSISTANCE.md` A′ 里的「不透明主窗口跨屏拖」判定实验仍值得做（1 分钟）：
  它能区分「透明相关」还是「跨屏 DPI 本身」，决定要不要动 `FUTURE.md` A3 的 CEF 退路。
- 若新产物不再卡，这一项就按「已修复」收；若仍卡，心跳日志会给出下一层证据。
