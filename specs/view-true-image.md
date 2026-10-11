# 看图界面 = 真身像素（preview 只做载入占位）

> 崔总 2026-10-11 定。本文件同时是**这一轮的方向说明书**（原本的目标 / 走到哪里错了 /
> 正确方向 / 性能预算）与**实施验收清单**；「取图档位」这件事的唯一事实来源就是它。
>
> 一句话：**看图（browse view / compare / fullscreen / editor 视口）显示的一定是那个变体自己的
> 真实像素；1920 预览只允许作为「载入期间的临时占位」，且必须在真身到达后立刻被替换。**

---

## 0. 综合理解（原本的目标 / 错在哪 / 往哪走）

### 0.1 原本的目标（按时间顺序，三条互相叠着的任务）

1. **把「这张照片现在显示的到底是多大」收敛成一个来源**（`specs/display-size-source.md`，2026-10-11）：
   以前有三个互不同源的口径（catalog 基础尺寸 / 变体全尺寸渲染 / 渲染出来的字节），
   flowinfo、tiles 比例、view 布局各读各的。→ 收敛成库里的 v19 三组列 + 一个解析函数
   `store::sizes::display_size` + 一个前端读口 `lib/display-size.ts`。**方向正确，已完成。**
2. **修「view 里 RAW 的尺寸没有跟着 RAW 自己走，而是缩进了 SOOC 的尺寸里」**（崔总 2026-10-11 叙述的原始症状）。
   这是**本轮真正的 bug**。
3. **看图显示真身**：view / compare / fullscreen / editor 视口看到的必须是该变体的真实像素
   （崔总 2026-10-11 的口径；preview 只做载入占位）。

### 0.2 错在哪（一步步）

| 时间 | 做了什么 | 判断 |
| --- | --- | --- |
| 2026-09-18（M2-W2） | view 的取图口定为 `grid / strip / screen(1920) / original`；view 用 **screen** | 当时的目的正当（连续翻图别每次真解码）。**但要说准确**：1920 不是「永远不够」—— 只要**显示盒子 ≤1920px**（对比的每一格、正常窗口下的适配）它看起来就是像素级 ✓；真不够的是「盒子比它大」的时候（单张 1:1、超大窗）。逐路径表见 §0.2.1 |
| 2026-09-23（M3-W2） | **editor** 引入「预览档 ≤1920 / 全尺寸档」+ 迟滞（理由：RAW 完整解码秒级） | 合理：它只作用于**编辑器像素**，且档位之间永远是「同一张图、同一套参数」的分辨率差，不是换一张图。人类 2026-09-24 还补了「拖动期间只算预览档、松手再补全」。**与 view 无关** |
| 2026-10-11（`display-size-source.md`） | 尺寸收敛进库 ✓；view 改成「按变体尺寸布局」 | 布局口径**正是任务要的**，但它把「尺寸」变成**看得见的倍率**：尺寸一旦取错，立刻表现为「比例不对」；而当时的尺寸回退链**跨变体**，懒回填还会在用户眼前把值改掉 → 同一张图两次打开不一样大 |
| 2026-10-11/12（上一个会话 + 本会话） | 把 editor 的「按缩放换档」当成通用需求，提出「**view 也按 zoom 在 1920 / 全尺寸之间换档**」 | ❌ **凭空发明的需求**。崔总原话：「我这辈子没提过在看图时（view/edit）随着缩放档位选择最近的尺寸充数这种需求，如果有，全部删干净，禁止再出现拿假图来充看图界面的功能。」→ 已删除，并在本文件 §1 记为禁止项 |
| 同段 | 真身那件事（第 2、3 条）还没做完，先被档位话题岔开 | 结果是「真正的 bug 没修完 + 观感上多了一层假图」 |

**根因（已用真库数据核对，MYP1000037 / asset 3：SOOC 3712×2784、RAW 5264×3904）**：
每个变体的尺寸在「自己的那一行没值时」被允许**退到另一个变体**：

* 前端 `lib/display-size.ts`：`choice === "raw"` 时 `?? base`（base = SOOC 的尺寸）——**就是崔总那句话的代码**；
  定稿（`issue:N`）同理，缺值退 base。
* Rust `store/sizes.rs::preferred_size`：`raw` 行没回填时走 `base_size` → 位图行 / `assets`（SOOC 的尺寸）；
  反方向 `bitmap` 行没回填时又会拿到 RAW 的尺寸（100% 下按 5264 放大）。
* 前端的 `store.naturalOf` 用的还是**变体盲**的默认口径（`displaySizeOf(item, undefined, undefined)`：
  latest 优先，没编辑过就是基准）——RAW 变体拿到 SOOC 的尺寸，第二条同源的路。
* 老库（v19 之前）这三组列本来就是 NULL，而懒回填发生在**用户点开这张时** ⇒ 同一个变体的尺寸
  会在「回填前 / 回填后」之间跳 —— 「越捅越大」的观感就是这么来的。

### 0.2.1 逐路径像素表（2026-10-11 崔总：「view 只给 1920 我不信，我测 compare 看到过像素级」）

两者都对 —— 差别在**每格盒子要多少像素**，以及 **editor 的对比走的是另一条路**：

| 界面 | 像素来源（代码） | 实际分辨率 | 看起来 |
| --- | --- | --- | --- |
| browse 单张 view | `getViewImage(path,'screen')` / `getExportVariantImage(…,'screen')` | 1920 档 | 盒子 ≤1920px 时像素级 ✓；1:1 时放大 2.7× → 发虚 ✗ |
| browse compare（DOM 多格） | 同一个 viewer store 的 `ensureImage` → `screen` | 1920 档 | 每格只有几百 px ⇒ **降采样显示 = 像素级** ✓（崔总看到的就是这一条） |
| fullscreen（F11） | `screen` → 随后 `full` | 1920 占位 → 真身 | 同单张 view |
| **import view** | 384 小图占位 → `full` | **没有 1920 这一档** | import 不生成也不请求 1920（`specs/thumbnail-tiers-and-import-memory.md`）：占位用 384 顶着，终态是原文件本身（位图）/ 完整解码（RAW） |
| **editor 视口** | `display::pixels(Screen/Full)`（Rust 纹理） | 进视口 / 松手 = **全尺寸**；拖动参数中 = 屏幕档（真实显影的低清）—— **缩放不换档、不重出图**（崔总 2026-10-11） | ✓ |
| **editor 的对比** | `entry.reference.get_sooc(path, …)`（**SOOC 原文件自己的帧**）/ `get(source, …)`（完整解码的线性源） | **真像素** | ✓ 崔总看到的像素级对比就是它 |
| 网格 / 胶片带 / 右栏预览 | `thumb_get` grid | **384**（只剩这一档；192 已取消） | 缩略图界面，本来如此 |

结论：这不是「谁对谁错」，而是同一条规律 —— **给的像素少于盒子需要就发虚，多于就正好**。
所以本轮不改这条规律，改成：**view 一律给真身**（崔总口径），1920 只在载入期间当占位。

### 0.3 正确方向

1. **每个变体的尺寸只认自己**（`raw` ← RAW 行；`sooc` ← 位图行；`latest` ← 栈输出；`issue:N` ← 该定稿）；
   缺值 = **还没拿到**，由读侧懒补（读一次文件头）补上，**绝不退到别的变体**。
2. **view 的像素只有一种终态：真身**（preview 仅载入占位、尽快替换）。
3. **editor 的档位保留**（它不是 view；人类口径里的「拖动期间只算预览档、松手补全」就是它），
   但档位只允许在「同一张图、同一套参数」的分辨率之间切换 —— 禁止换成另一张图充数。

### 0.4 性能与缓存（崔总的预算与模型，写死在验收里）

> 崔总 2026-10-11：「双击打开一个 RAW、从 SOOC 切到 20M 的 RAW 解析速度大约 **3 秒**，
> 我认为这都是可以接受的；加了无损编辑 **4–5 秒**内也应该能完成（可能也就是降噪锐化有点成本）。」
> 「cache 你要做可以做，但**一定不能把这个当成是必须有的东西**，设置一个范围，滚动清除更新即可；
> 要考虑我们的 issue 系统：方便清理、也能高效更新（比如 latest 必更新），同时也要兼容 sooc 这种类型。」

**定位：缓存 = 纯派生的工作缓存，不是正确性的依赖。**

* 它只负责「来回切要快」（同 fullscreen 预载的思路），**首次打开就该老老实实现算** ——
  一张几十 MB 的 RAW 等 3–5 秒是正常的（想要更快 → 库里把默认显示变体设成 SOOC，见 §2.4）；
* 任何读取路径 cache-miss = 现算；写缓存失败只记日志（**已有行为，保持不变**）；
* 删掉整个 `cache/` 目录 ⇒ 功能降级但不坏（`AGENTS.md` §6.5 红线）。

时间轴（两段式，任何一段超预算都不许用降档掩盖）：

| 段 | 目标 | 手段 |
| --- | --- | --- |
| ① 占位可见 | **毫秒级**（≤100ms） | 缓存里已有的 1920 档 / 内嵌预览；叠「正在载入原图…」，可交互 |
| ② 真身到位 | 冷缓存 **≤3s（无编辑）/ ≤5s（带编辑）**；**命中缓存毫秒级** | 完整解码 / 全尺寸成片 + JPEG 编码，落库内 `cache/full/<资产>/` |

**实测底数（2026-10-11，WSL release，真库 MYP1000037.RW2 / 5184×3888 / 20MP）：**

| 环节 | 耗时 | 来源 |
| --- | --- | --- |
| RAW 线性解码（完整） | **1165 ms** | `develop-probe` |
| 显影（全部调性 / 加降噪锐化镜头） | **142 ms / 763 ms** | `develop-probe` |
| 预览档显影（1920） | 16–32 ms | `develop-probe` |
| 20MP → 1920 缩放 | 173 ms | `develop-probe` |
| **JPEG q92 编码（20MP）** | **835 ms · 8.9 MB** | `zz-full-tier-probe` |
| 写盘 / 读盘（8.9MB） | 35 ms / **8 ms** | `zz-full-tier-probe` |

⇒ 冷缓存真身 ≈ **2.1s**（无编辑）/ **2.8s**（全阶段），命中缓存 ≈ **10–50 ms**（浏览器解码另算 ~0.1–0.2s）——
两者差 **~10–20×**，这就是缓存值得留的定量理由（崔总 2026-10-11：「cache 总是要有的，
只要维护的没问题，它的存在会让用户觉得舒服一些」）。
附带结论：真身档用 **JPEG**（20MP 的 AVIF 是秒级），单张 **~9MB** ⇒ 容量上限常量有必要
（5GB ≈ 500–600 张真身档）。

**滚动缓存规则（有界 + 按变体更新 —— 崔总点名的那套）**：

| 变体 | 何时写 | 何时清（关键：latest 必更新） |
| --- | --- | --- |
| `sooc` | **不写缓存**（直接读原文件，这就是最快的那条路）；它有编辑时的成片才进缓存 | 无条目 |
| `raw`（未编辑基准） | 首次看时写；源签名变了换键 | LRU |
| `latest` | 每次看都写；**编辑参数一变旧键立即失效**（键含栈指纹）—— 同一资产只留最新 1 份 | 换参数即扔旧的；LRU |
| `issue:N` | 定稿是**冻结**的 ⇒ 生成一次长期有效，最值得留 | LRU；定稿删除时连带删该资产对应键 |

* **范围**：`cache/full/` 有总量上限（默认 **5GB 或可用空间 5%，取小者**，一处常量），
  超出时按**最后访问时间 LRU** 滚动删除；打开库 / 闲置时跑一次清扫（不进看图热路径）。
* **连带清理**：删资产 / 清空回收站 → 连带删该资产的 `cache/full/<资产>/`（已有 hook）。
* **预取**：邻图预取也走真身档（否则翻到下一张又要等），但**限量 + 低优先级 + 可取消**
  （Rust 侧 RAW 解码是串行单例 worker，`DecodePriority` 已有优先级机制）。
* ❗**待拍板**：上面那个 5GB / 5% 的具体数字（我先按这个默认实现，一处常量随时改）。

**失效与更新 —— 维护成本的真正一块**（崔总 2026-10-11 指出我漏算了：键里含栈指纹
只保证「读不到旧图」，**不释放空间**，也不管「编辑后更新」这条线）：

| 触发 | 动作 | 现状 |
| --- | --- | --- |
| 编辑栈变了（含撤销/重做、批量色调） | `FullCache::invalidate(asset)` 删该资产的 `latest-*` | ✅ 已有（`browse.rs::invalidate_edited_previews`、`color_batch.rs`） |
| 源文件变了（修复/替换） | `invalidate_source(asset)` 删该资产全部缓存文件 | ✅ 已有（`repo.rs`） |
| 删定稿 | `remove_issue(asset, id)` 删 `issue-<id>-*` + thumbs.db 键 | ✅ 已有（`issues.rs::delete`） |
| 删资产 / 清回收站 | `remove_dir_all(cache/full/<资产>)` | ✅ 已有（`store/recycle.rs`） |
| 写完新快照 | `gc_latest(asset, keep)` 保留最新 N 份 | ✅ 已有（写后顺手做） |
| 容量超限 | 全局 LRU 清扫 | 🆕 **唯一的新代码** |

**新增成本（不藏着）：真身档会让现有失效函数漏删，必须改三处 + 补测试：**

1. `invalidate_source` 按 `ext == "avif"` 过滤 ⇒ 真身档是 **JPEG**，**漏删** ⇒ 改成不限扩展名；
2. `invalidate` / `gc_latest` 按 `latest-` 前缀过滤 ⇒ 真身档还会有 `raw-` / `sooc-` / `issue-` 前缀
   ⇒ 改成按「变体前缀集合」判；
3. 保留份数：真身档 ~9MB/张 ⇒ `keep` 从 2 降到 **2 仍可**（latest 的撤销/重做命中靠它），
   但容量清扫的优先级要按「真身档 > 1920 档」降级删除（一处常量）。

**取舍**：编辑后**不立即全删**旧的 `latest` 档，而是保留最新 2 份 —— 撤销 / 重做 / 来回调参数
会直接命中缓存（毫秒级），这正是「让用户觉得舒服」的来源；空间交给容量清扫兜底。

---

## 1. 口径（永久，写代码时按这个判）

| 场景 | 允许的像素 | 不允许 |
| --- | --- | --- |
| browse view / compare / fullscreen | **该变体的真身**（SOOC=原文件；RAW=全尺寸解码；latest/定稿=全尺寸成片） | 任何长期停留的低分辨率替身 |
| 上述界面载入中 | **库内**给 1920 占位、**import 给 384 小图**，都明示「正在载入原图…」，真身一到立刻替换 | 占位停留过久、占位被当成成品 |
| editor 视口（非交互） | 真实显影结果，分辨率必须够 1:1 看（屏幕需要多少像素就给多少） | 用另一张图（机内 JPEG / 别的变体）冒充 |
| editor 视口（拖动参数期间） | 允许低分辨率显影（真实显影的低清），松手**静默**升回 | 松手后仍停在低清；松手时弹「载入中」|
| editor 视口（缩放 / 平移 / 适应窗口 / 100%） | 复用同一张已画好的图，纯视口变换 | **任何重出图 / 载入提示**（崔总 2026-10-11：不存在按时按需升档） |
| 网格 / 胶片带 / 右栏预览 | 缩略图（grid **384**，胶片带共用同一档）—— 它们本来就是缩略图界面 | — |

**禁止项（写代码时见到就删）**：任何「按缩放档位/容器大小选一个凑合尺寸当看图成品」的逻辑；
任何「这个变体的尺寸缺失 → 拿另一个变体的尺寸顶上」的回退。

**「真身」的定义**：把 `尺寸(该变体)` 的图像整幅拿出来，100% 时 1 屏幕像素 = 1 图像像素。

### RAW 的尺寸 = **出图尺寸**（崔总 2026-10-11 定案）

> 「原则上 rawler 出图是多少，那就应该是多少，怎么能对不上呢？我现阶段不想改 rawler 的算法，
> 但是我们对外展示的分辨率不能对不上。」

文件头给的是**整帧**（MYP1000037.RW2 实测 5264×3904，含边距），而 rawler 出图裁到官方 4:3
（5184×3888）—— 两者差 1.5%。**展示尺寸取后者**：

* `raw` 变体的尺寸 = **rawler 显影管线的输出尺寸**（`RawlerBackend::output_size`）；
* 算法照抄 rawler `CropDefault` 那一段（`rawler/src/imgop/develop.rs`，2026-10-11 逐行对照）：
  目标矩形 = `crop_area` → 无则 `active_area` → 都无则不裁；有 `active_area` 时 `adapt`
  （我们只在包含时适配，rawler 越界会 panic）；四色 CFA 走 superpixel ⇒ 半分辨率；
* **唯一口径**：`store::sizes::apply_display_size_source` / `raw_output_size`，三处读盘
  （导入登记 / 懒回填 / 批量回填）都走它；
* 读不到（坏文件 / 未支持机型）→ 退回文件头尺寸（宁可差一圈也不能没有尺寸）。

真库核对（2026-10-11）：`asset_files` → `raw 5184×3888` / `bitmap 3712×2784`；
真身渲染出来也是 `5184×3888` —— **数字与出图严格一致** ✓。

**域外像素是另一回事**（画质，不是尺寸）：那圈 80×16 的边距像素对镜头校正的挪移有用，
登记在 `memory/FUTURE.md`（改法：镜头校正早于裁切）。做那件事时输出尺寸仍应保持 5184×3888。

## 2. 实施清单

### 2.1 尺寸（本轮真正的 bug，先行）

- [x] RAW 行的值 = **出图尺寸**（`RawlerBackend::output_size`）：文件头整帧 5264 会让展示尺寸比像素大 1.5%
      —— 崔总 2026-10-11 定案「原则上 rawler 出图是多少，那就应该是多少」。
- [x] Rust `store::sizes::preferred_size`：`raw` 只认 RAW 行（只有「没有位图行」的 RAW-only 资产才退到
      `assets`）；`bitmap` 只认位图行 / `assets`（没有位图行时退到 RAW）。**跨变体回退已删**
      （`cargo test` 有矩阵钉住，含 `sooc_never_borrows_the_raw_size_when_the_bitmap_row_is_unfilled`）。
- [x] 前端 `lib/display-size.ts::displaySizeOf`：`raw` / `issue:N` 缺值返回 `null`（= 还没拿到），
      唯一保留的 `?? base` 是 `latest`（没编辑栈时 latest 的定义就是基准 —— 同一个变体）。
- [x] `features/browse/display-variants.ts::adapt`：选了变体就只认该变体的尺寸（没有 → `null`），
      不再用默认变体（latest/SOOC）的尺寸顶上；`aspectOf` 同步。
- [x] 读侧懒补覆盖到「用户点开任意变体」：`export_snapshots`（≤8 张）会补 `asset_files.display_*`；
      补完再取尺寸 ⇒ 正常的第二次读取就是准的。

### 2.2 view 的像素：真身（本轮主要工作）

- [x] `SizeClass::Full`（`crates/raybend/src/thumbnail/render.rs`）：长边 = 图像自身（不缩放）；
      `render_sig` / `all()` / `parse("full")` 同步。
- [x] `ImagePurpose::Full`（`crates/raybend/src/display/mod.rs`，`"full"`）：
      位图 + 没编辑 → 原文件字节；位图 + 有编辑 → 全尺寸渲染；RAW → **完整解码**（不许内嵌预览）。
- [x] 编码：真身档用 **JPEG（q92）**（20MP 的 AVIF 编码是秒级到十几秒；JPEG 编码 ~0.3s、浏览器原生解码）。
- [x] 落盘：`<库根>/cache/full/<资产>/…-full-…`（复用 `FullCache` + `gc_latest`）；
      未编辑的 SOOC 不落盘（直接给原文件）。
- [x] 命令：`view_image(path, "full")`、`export_variant_image(reference, "full")`；
      `screen` 语义收窄为**载入占位**（网格/胶片带仍 `grid`）。
- [x] 前端 viewer：载入两段（占位 → 真身），真身到位**原地替换**（zoom/pan/布局不变），
      期间的角标「正在载入原图…」不拦输入；失败保留占位 + 可重试。
- [x] 邻图预取改走真身档（限量 + 低优先级，见 §0.4）。

### 2.3 editor：提示与画面对齐

- [x] 「正在载入照片…」的结束条件 = **真帧已画出**（`decode=ready` 且 `paintedPath` 是当前照片）；
      过渡帧只负责「洞口透明 + 有画面可看（软化）」，**不得**结束提示
      （现状：`present_transition` 一画上，`RenderOutcome::Drawn` 就把 `painted_path` 标上 ⇒
      `editorViewportNotice` 立刻不再提示 —— 三步的根因）。
- [x] 换图（胶片带、进编辑器）都先给过渡帧 + 提示，再出真帧。
- [x] 100% 必须是全尺寸（`render/tier.rs` 的规则保留；修掉「双击到 100% 仍是预览」那条路）。
- [x] 登记（不在本单元做）：崔总 2026-10-11 提的「拖动时只渲染画面里实际可见的那块区域」→
      `memory/FUTURE.md`。

---

## 3. 验收清单

- [x] 真库 drill（`examples/full-tier-probe.rs`，复制库跑）：逐变体尺寸 = 各自那一行
      （sooc 3712×2784 / **raw 5184×3888** / **latest 5184×3888** / issue:8 3712×2784）；
      真身渲染 = 源自己的像素（5184×3888 · JPEG · 8.3MB）—— **展示尺寸与出图像素一致**。
- [x] `latest` 的输出尺寸**现算优先**：`develop_stacks.output_*` 是当年按源尺寸存的缓存，
      源尺寸一修正就旧了（本轮 RAW 从 5264 改成 5184 时实测到）；现算拿不到才退回存值，
      回归测试 `latest_recomputes_when_the_stored_output_is_stale`。
- [x] 载入两段：占位先上屏（+「正在载入原图…」）、真身到达后**原地替换**（zoom/pan/natural 不变，
      单测钉住）；真身取不到时保留占位、提示落下。
- [x] `sooc` 的真身 = **原文件字节**（位图 + 没编辑的短路；单测钉住「一个字节不改」）。
- [x] 时间轴（WSL release 实测）：真身冷缓存 8.0s（这条是 20MP RAW + 完整解码 + 8.3MB JPEG 编码；
      其中解码 1.2s、编码 0.8s、develop 余下 —— **超 3s 预算**，Windows 真机要复测，
      见 §0.4 的分解与 `memory/FUTURE.md` 的镜头校正条目）；命中缓存 **1ms**；写缓存 0.9s。
- [x] editor：过渡帧不算载入结束（`editorViewportNotice` 改判定 + 单测）；
      「提示消失 ⇔ 真帧上屏」。
- [x] 单测：`SizeClass::Full` / `ImagePurpose::Full` / 缓存分档与容量清扫 / 尺寸只认自己那一份 /
      「占位→真身不改几何」。
- [x] `cargo test --workspace`（1504+134）、`pnpm typecheck && pnpm test`（1287）、
      `lint:colors|arch|i18n`、`build`、`smoke:ui`（problems: []）全绿。
- [ ] **人类（Windows 真机）**：四个变体逐一目视比对 editor / browse；两段载入的观感（占位→真身
      的替换、提示位置）；100% 的锐度；冷缓存首次打开 20MP RAW 的实际秒数（WSL 上是 8s，
      Windows 有 GPU/更快的盘，需要真数）。
- [x] RAW 尺寸 = **出图尺寸**（`RawlerBackend::output_size` 照抄 rawler `CropDefault`）；
      `raw_output_size` / `apply_display_size_source` 是唯一口径，导入/懒回填/批量回填三处都走它。
- [x] 域外像素给镜头校正的改进项已登记 `memory/FUTURE.md`（画质，不是尺寸）。

---

## 4. 代码指针

| 位置 | 作用 |
| --- | --- |
| `crates/raybend/src/store/sizes.rs` | 尺寸唯一解析口（`display_size` / `preferred_size` / `latest_size` / `issue_size`；RAW 出图尺寸 `raw_output_size` / `apply_display_size_source`） |
| `crates/raybend/src/raw/rawler_backend.rs` | `output_size`（照抄 rawler `CropDefault` 的出图尺寸，唯一读法） |
| `crates/raybend/src/thumbnail/render.rs` | `SizeClass`（加 `Full`）、`render_file_with_profiles`、`encode` |
| `crates/raybend/src/display/mod.rs` | `ImagePurpose`（加 `Full`）、`cached_image`、`takes_original_shortcut` |
| `crates/raybend/src/display/full_cache.rs` | 库内 `cache/full/<资产>/`（`FullCache`，写/读/GC） |
| `crates/raybend/src/render/tier.rs` | editor 的两档迟滞（**只属于 editor**） |
| `src-tauri/src/thumbs.rs` | `view_image`（占位 / 真身两条路） |
| `src-tauri/src/export.rs` | `export_variant_image`、`export_snapshots`（尺寸懒补） |
| `src/lib/display-size.ts` | 前端尺寸唯一读口（`displaySizeOf` / `sourceBaseOf`） |
| `src/components/ui/viewer/store.ts` | 载入两段（占位 → 真身）、`natural`/zoom 不变量 |
| `src/features/editor/viewport.tsx` · `src-tauri/src/editor.rs` | 提示与过渡帧 / 真帧的对应 |
