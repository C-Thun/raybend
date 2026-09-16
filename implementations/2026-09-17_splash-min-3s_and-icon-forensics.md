# 闪屏最短停留 3 秒（+ 图标「没换」的取证）

完成时间：2026-09-17 01:48:34 CST

承接：`implementations/2026-09-17_branding-and-splash.md`（同日上一批）。人类在验收中提了两件事，本文件对应第二件；
第一件（「编译后产物的 logo 没换」）经查**产物本身没问题**，是 Windows 图标缓存，取证见下。

---

## 1. 人类提出的两条

| # | 原话 | 结论 |
| --- | --- | --- |
| 1 | 「为什么编译后产物的 logo 没换掉？」 | **产物三个面全是新的**（逐字节证据见 §2）——看到的是 Windows 的图标缓存 |
| 2 | 「给 splash 设置 3 秒的保底时间，就算后台准备好了也等 3 秒后再显示」 | 已实现（§3） |

---

## 2. 图标取证（结论：`raybend-desktop.exe` 里的图标就是新 logo）

查了三处，全部是新的：

| 面 | 「新」的证据 |
| --- | --- |
| **exe 的文件图标** | 解析 PE 资源段：`RT_ICON` 6 张，尺寸/哈希与 `src-tauri/icons/icon.ico` 里 6 张**逐字节一致**（2357/790/1458/4405/6894/46999 字节） |
| **窗口 / 任务栏图标** | tauri-codegen 取 `icons/icon.ico` 的**第 1 个条目**（`entries()[0]` = 32×32）解码成 RGBA 嵌进 exe（读自 `tauri-codegen-2.6.3/src/image.rs`）。把新旧 `icon.ico` 的首条目各解一遍去 exe 里搜：**新 3/3 命中，旧 0/3** |
| **界面里的 logo** | exe 里含**当前 dist** 的全部哈希资源名（`logo-small-Dm-StJAv.png`、`logo-BYYe_1eT.png`、`index-Czfu6-eG.js`…），且 `pnpm check:win` 通过（exe 01:48 > dist 01:28） |

**那为什么看着没变**：同一路径的 exe 被覆盖时，资源管理器/任务栏会继续用缓存里的旧图标（已固定的任务栏图钉把图标缓存在 `.lnk` 里，更顽固）。这与产物无关。

**下次怎么判定**（已写进 `AGENTS.md` §5.3.2 第 4 条）：

- 看 exe 的 `RT_ICON` 资源 / `icon.ico` 首条目的 RGBA（本文件上面那两招）；
- 想眼见为实：把 exe **复制成新文件名**再看（新路径不撞缓存），或 `ie4uinit.exe -show` / 删
  `%LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache_*.db` 后重启资源管理器；任务栏图钉要取消固定再固定。

---

## 3. 闪屏最短停留 3 秒

### 3.1 改了什么（`src-tauri/src/lib.rs`）

```text
                     ┌─ ui_ready（前端首屏+字体就绪）──┐
t0 = splash.show() ──┤                                  ├─→ reveal_main()（显示主窗口 + 关闪屏，幂等）
                     └─ 兜底线程（前端一直不就绪）──────┘
   ↑ 两条路径都要等 t0 + 最短停留；兜底再多等 1 秒
```

- 新增 `SPLASH_MIN_VISIBLE = 3s`（人类要求的最短停留）与 `SPLASH_SHOWN_AT: OnceLock<Instant>`
  （在 `setup()` 里 `splash.show()` **成功之后**记，dev 不显示闪屏 ⇒ 永远为空）。
- `ui_ready` 不再直接收尾，而是走新的 `reveal_main_after_splash_min()`：算还差多久，睡完再收尾。
- `SPLASH_TIMEOUT` 3s → **4s**（刻意比最短停留多 1 秒）：这样
  ①「前端说好了」和「兜底放行」在计时/输出上能分辨；②兜底不会抢在「等满 3 秒」前面关掉闪屏。
- 两条路径都打一行 `[raybend] …`，日志能看出是哪条腿放的行。

`reveal_main()` 仍然幂等：前端早就绪 + 兜底同时到也不会出错（窗口 show/focus/close 都只问「还在不在」）。

### 3.2 边界怎么测的（不靠 sleep）

把时钟抽成入参：`remaining_until_min_visible(shown_at: Option<Instant>, now: Instant) -> Duration`，
于是单测可以拿合成时间戳打边界，测试耗时 0.00s：

- `None`（dev，没露过闪屏）⇒ `ZERO`，主窗口立刻显示；
- 刚露出来 ⇒ 3s；露了 1 秒 ⇒ 2s；**正好够 ⇒ 0**；**超了一天 ⇒ 0**（`saturating_sub`，不 panic）。
- 另一条不变量：`SPLASH_TIMEOUT > SPLASH_MIN_VISIBLE`（防止有人把兜底调到最短停留之前，让 3 秒变成摆设）。

### 3.3 为什么兜底不是「也 3 秒」

最早一版想让两条路径都落在 3s。但那样 `ui_ready` 实际上永远不生效（兜底线程先到），
代码里留一条永不执行的路径比多等 1 秒更糟：**「前端坏了」和「前端好了」在这套时序里应该是两件可分辨的事**。
代价只有 1 秒，而且只在真出问题时付。

---

## 4. 验证

| 项 | 结果 |
| --- | --- |
| `cargo test -p raybend-desktop` | **26 passed**（24 + 新增 2 条闪屏时序边界） ✅ |
| `cargo clippy --workspace --all-targets` | 0 警告 ✅ |
| Windows 产物重建 | `raybend-desktop.exe` 01:48:08 ✅ |
| `pnpm check:win` | ✅ exe 比 dist 新，4 个引用资源全命中 |
| 图标取证 | PE `RT_ICON` ×6 逐字节等于新 `icon.ico`；窗口图标（首条目 RGBA）新 3/3 / 旧 0/3 ✅ |

**未验证（人类）**：闪屏实际停多久、观感是否更好（`PLAN.md` 验收清单 3f 已更新为「至少 3 秒」）。

---

## 5. 遗留

1. 图标的**眼见为实**要人类做一次（清缓存或复制成新名字后看）——若清了缓存**仍是旧图标**，那才是我漏了什么，请告诉我。
2. 上一批的其它遗留（`main.pen` 画布未同步 `AppIcon`、字体度量 0.5–0.8px 偏移）不变。
3. 两批改动都**未提交**（plannotator 流程期间不 commit）。
