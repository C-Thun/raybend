# rb-target 瘦身 + 编辑「卡在正在载入照片」的定位手段

完成时间：2026-09-25 11:19:44 CST

## 1. target 目录清理：一次性 + **常态化工具**

### 1.1 结论：确实攒了

`C:\rb-target\raybend\debug`（Windows 侧）清之前：

| 目录 | 大小 | 说明 |
| --- | --- | --- |
| `deps/` | **5.8 GB**（3444 文件） | 同一个 crate 有多份 hash 副本：`libtoml` **11 份**、`libtauri_utils` 11、`libtoml_datetime` 9、`libwindows` 4（每份 rlib+rmeta 各 ~90MB）……这是依赖图/feature 组合变化后**旧副本从不回收**的结果 |
| `incremental/` | **3.7 GB**（60 条目） | **只有我们自己的 crate**：`raybend_desktop_lib-*` 单独就有 ~500MB × 10 个不同 unit-hash 目录、`raybend-*` ~300MB × 3 —— cargo 只清「同一 unit」的旧会话，**换 feature/配置产生的新 unit 会各自留一份目录** |
| `build/` | 580 MB（205 条目） | 构建脚本 out 目录，同名多份（`webview2-com-sys` ×3 等） |
| 顶层 | ~250 MB | exe 47MB + worker 13MB + dll 3MB + 3 个 pdb（117/44/25MB） |

处理（当时只有这一次性手段）：`cargo clean` → **Removed 23768 files, 11.6GiB** → 重跑
`pnpm debug:win` 冷构建（~12 分钟）→ 复核全绿。清完：`deps` 2.7G + `incremental` 479M +
`build` 216M = **3.4 GB**。

### 1.2 人类随后要求：**集成进 `debug:win`，每次编译后自然清掉失效内容**（不再全删）

新增 `scripts/clean-target.mjs`（`pnpm clean:win` / `pnpm clean:wsl`），规则三条：

1. **判据是 cargo 自己的构建图** —— `cargo build --message-format=json` 会把**每个单元**都报出来
   （已经是新的也报，带 `"fresh":true`）⇒ 这就是权威存活集合，不用猜；
2. `deps/` 只留「文件名主干 ∈ 存活集合」的（`-` ↔ `_` 归一，否则 `deps/raybend_desktop.exe`
   这类**最终产物副本**会被误判）；`build/` 只留存活集合里出现过的 `<包>-<hash>/`；
   `incremental/` 每 crate 保留 K 份（K = 存活集合里该 crate 的 hash 数；目录名用的是另一套 hash，映射不上，只能按「数量 + 时间」保）；
3. **宽限期 `--keep-days`（默认 3 天）**：**不在我的图里**但最近碰过的**不删** ——
   同一台机器上**另一个会话可能用另一套构建参数**，那些产物在我的图里就是“不存活”的，但它在用。

接法：`debug:win` 第 ② 步给 cargo 加 `--message-format=json > C:\rb-target\raybend\last-build.json`
（进度照旧走 stderr），第 ④ 步调清理脚本复用它 —— **不额外编译**。实测：

| 跑法 | 结果 |
| --- | --- |
| `pnpm debug:win`（④ 步） | ✓ 本次“无需清理”（target 刚重建，全在宽限期内）—— 行为正确 |
| `pnpm clean:win --dry-run --keep-days 0` | 准确报出 66 个**属于另一套参数**的 `build/` 目录（±72 MB） |
| 合成目录上真实删除 | 活着的产物 + 它的 `.d` 伴陏文件都保住，垃圾被删 ✓ |
| `pnpm clean:wsl`（真实清理） | **7234 项 / 15 GB**（0.9s）；清后 `cargo test -p raybend --lib` 1006 条全绿 ✓ |

> 代价要知道：被删的测试/示例产物会让**下一次**那个用途的构建慢一点（实测清完後跑一次
> `cargo test` 用了 3m40s，之后恢复秒级）—— 这是缓存，不是产物。

清理后：

| 目录 | 大小 |
| --- | --- |
| `deps/` | 2.7 GB |
| `incremental/` | 479 MB（一次构建后） |
| `build/` | 216 MB |
| **合计** | **3.4 GB** |

### 留给下次的经验

* 这个 target 目录是**只进不出**的：每次依赖升级/换 feature 就多一份 artifact，
  而 cargo **不回收**（实测 `deps/` 里每个文件都还有 `fingerprint` 目录 —— 靠指纹辨不出过期的）。
* 现在不用靠 `cargo clean` 了：**每次 `pnpm debug:win` 会自动清一次**（只消那些既不在构建图里、
  又已经凉了几天的）；WSL 侧手动 `pnpm clean:wsl`。
* 别在下一次 Windows 构建正跑着的时候删 target —— 会把别人的构建打断（同一份 target 目录）。

## 2. 「卡在正在载入照片」：先加定位手段（本次没定论）

人类 2026-09-25 报：点一张**没编辑过**的照片 → 编辑 → 一直卡在「正在载入照片…」；
换到**编辑过**的那张 → 卡很久后出来了；再切回第一张 → 又卡。

Agent 侧能验的都验了，**都是好的**：

| 验的东西 | 方法 | 结果 |
| --- | --- | --- |
| Windows worker 内嵌预览 | `RAYBEND_RAW_WORKER=<win exe> thumb-probe` | ✅ 0.57s 出图 |
| Windows worker 线性解码 | `develop-probe` 同一张真 RAW | ✅ 1.56s（5184×3888） |
| 管线（含 W4 的镜头/降噪/锐化阶段） | `develop-probe` | ✅ 预览档 87ms、24MP 全阶段 640ms |
| 过渡帧 + 真帧那条链 | `cargo test -p raybend-desktop --lib -- --ignored real_raw`（新加的复现测试） | ✅ 1.55s 跑完，过渡帧先到 |

⇒ 卡的地方不在「解码/管线慢」，而在**某一步没往下走**。为了让下一次复现自己说话，
在显影线程与命令层加了三条诊断（`src-tauri/src/editor.rs`，**未提交**）：

* `[editor] 过渡帧：<origin> W×H（逻辑 …）耗时 …ms；候选 N 个` —— 过渡帧真的做出来了；
* `[editor] 过渡帧候选 <origin>：读不到缓存 / 解不出来 …` —— 逐个候选的失败原因；
* `[editor] 真帧：job #N 档 … 解码 …ms 管线 …ms（整条 …ms）` —— 真帧交回渲染线程了。

**怎么看**（这三种组合各自指向不同环节）：

| 日志里看到 | 说明卡在哪 |
| --- | --- |
| 什么都没有 | `SetPhoto` 没送到显影线程（命令层 / IPC 层） |
| 只有「过渡帧」 | 卡在过渡帧之后、真帧之前（worker 请求 / 解码 / 管线） |
| 「过渡帧」+「真帧」都有 | 显影线程没问题 —— 卡在渲染线程或前端（比如状态没被轮询到） |

复现命令（**保留 stderr**，AGENTS.md §5.3 那条默认是 `>/dev/null 2>&1`，会把线索丢掉）：

```bash
(cd /mnt/c/rb-target/raybend/debug && ./raybend-desktop.exe > /mnt/c/src/tmp/rb-editor.log 2>&1 &)
```

**一个只需要观察、不用看日志的判断**：卡住的时候，界面的其它部分（胶片带、面板、按钮）
还有反应吗？

* 还有反应 ⇒ 卡在显影线程或渲染线程（Rust 侧某一步没往下走）；
* 整个界面都不动了 ⇒ **主线程/IPC 被占住**了 —— 目前最可疑的是
  `editor_set_params`（W4 的镜头那一步）**在 IPC 线程上**做库查询 + lensfun 解析
  （`src-tauri/src/lens.rs::render_correction` → `crates/raybend/src/lens/`），
  它每次参数载荷都会跑一遍；一旦某次卡住，后面排队的 `editor_set_photo` / 状态轮询全部跟着停，
  界面就停在最后一帧状态（= 「正在载入照片」）。

## 3. 本次改动的文件

* `scripts/clean-target.mjs`（新）：上面那个清理器；`package.json` 增 `clean:win` / `clean:wsl`；
  `scripts/debug-win.mjs` 第 ② 步带 `--message-format=json`、第 ④ 步自动清。
* `src-tauri/src/editor.rs`（**未提交**）：卡顿定位用的三条诊断日志 + 一条 `#[ignore]` 的真 RAW 复现测试
  （`real_raw_transition_then_real_frame_terminates`）。
  ⚠️ 这个文件**另一个会话（M3-W4）也在改**，所以没有提交、也没动别人的行。
* `C:\rb-target`：清了（不属于仓库）。
