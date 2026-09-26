# 交接文档：接 AVIF 解码器 → 「进来先读 preview」→ 切图优先（2026-09-24）

> **交接时间**：2026-09-24 16:55 CST
> **新会话的任务**：**只做 §2 与 §3 这两步**（人类点名「首重」）；§4 紧随其后（同一套机制）；
> §5 是登记项，**别顺手做**。
> **本文件的性质**：§2/§3/§4 都做过**查证预调研**（读代码 + 实测本机工具链 + 查了上游 crate 的构建方式），
> 不是转述猜测。凡**没查实**的都写明「未查实」。
> **上一段会话交付**：见 §1（六个提交；`IMAGING.md` 立成唯一事实源；编辑基准可切）。
> **人类已经拍板的三个方向**（不要再问一遍）：① 接 `avif-native`（dav1d）；
> ② 总览图下 SOOC/RAW 切换（**已实现**，issue 打标签那部分只登记）；
> ③ §2.5 切图优先「等 AVIF 解码器一起做」。

---

## 0. 开工前必读（现状与命令）

```bash
# 门禁（当前全绿：Rust 950 + 1 ignored、桌面 66、前端 906；tsc / 三条 lint 干净、clippy 仅 1 条既有告警）
cargo test -q -p raybend --lib            # ~13s
cargo test -q -p raybend-desktop --lib    # ~0.01s
pnpm test                                 # ~?s（node --test）
npx tsc --noEmit -p tsconfig.json         # **判据以它为准**（pi-lens 的 LSP 缓存会落后，会报假阳性）
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
cargo clippy -q -p raybend -p raybend-desktop --all-targets   # 只应剩 store/backfill.rs:396 那条
```

* **HEAD** = `0a9968d`（docs: 三项拍板登记…）。工作区干净、**未 push**（推送归人类，`AGENTS.md` §2.1）。
* ⚠️ **本仓不跑 `cargo fmt`**（HEAD 就不是 rustfmt-clean，门禁里也没有它）。
  2026-09-24 有人跑过一次，把 62 个无关文件全重排了，回退 + 逐条重放花了一轮 —— **别重蹈**。
* ⚠️ **pi-lens 的假阳性**：i18n 键名报错（陈旧缓存）、`no-console-except-error`（规则原文允许 catch 里的
  console.error）、hyphenated-svg-attribute（既有）、rust-clippy 延迟 runner（环境噪音）——
  **判据是 `tsc` / 直接跑 clippy 的退出码**，见 `src/i18n/index.ts` 文件头的「编译器诊断纪律」。

### 关键路径速查

| 想知道什么 | 去哪 |
| --- | --- |
| 三种图（thumb / preview / 大图）的规格与现状 | **`IMAGING.md`**（唯一事实源；§1 格式范围、§4.3 编辑基准、§5.1 档位、§8 现状对照、§9 未决点） |
| 格式支持范围 / AVIF 编码代价 | `IMAGING.md` §1、`FUTURE.md` §C8 |
| 编辑视口与显影线程 | `src-tauri/src/editor.rs`（`develop_loop` / `run_develop_job` / `decode_linear_source` / `RenderCommand` / `DevelopedImage`） |
| 取图口（view / 缩略图 / 直方图） | `src-tauri/src/thumbs.rs`（`view_image`、`render_latest_cached`）、`crates/raybend/src/display/`（`pixels.rs`、`full_cache.rs`） |
| 渲染管线（thumb/preview 都走它） | `crates/raybend/src/thumbnail/render.rs`（`render_file_with_edit`、`PIPELINE_VERSION = 6`、`SizeClass`） |
| issue / 编辑基准 / preview 规则 | `crates/raybend/src/store/develop.rs`（`IssueChoice`、`EditBase`、`edit_target`、`needs_preview`、`choose_issue`） |
| preview 刷新命令 | `src-tauri/src/develop.rs::develop_preview_refresh`（进/出编辑各调一次） |
| 前端编辑 store | `src/features/editor/store.ts`（`editBase`、`paramDragging`、`developPayload`） |

### 样本与探针（验证用，别自己造）

```bash
# 真照片（方向、RAW 解码、直方图）
/mnt/c/src/tmp/pic/P1000096.RW2   # worker 报方向 1、文件头是 8（§2.3 那条就是它验的）
/mnt/c/src/tmp/pic/P1000019.RW2   # 方向 1
/mnt/c/src/tmp/pic/P1000001.JPG

cargo run -q -p raybend --example editor-offscreen -- /mnt/c/src/tmp/raybend-w2   # 视口离屏像素证据
cargo run -q -p raybend --example develop-probe                                    # 管线耗时 + 方向证据
cargo run -q -p raybend --example hist-probe -- <照片…>                            # 直方图取数
cargo run -q -p raybend --example avif-probe                                       # AVIF/JPEG 编码耗时与体积
```

---

## 1. 上一段会话交付了什么（六个提交）

| 提交 | 内容 |
| --- | --- |
| `c44117c` | **拖动中只算预览档**：`render::tier::tier_for_params`（手指按着时一律 Preview，松手按缩放补全尺寸）+ 载荷 `interactive` + 滑杆/曲线接线 |
| `ca6c52f` | **preview 生成节点**：`store::develop::needs_preview` + 命令 `develop_preview_refresh`（进/出编辑），复用 `render_latest_cached` |
| `67e2422` | 小图必须放大（`PreviewFrame` 过渡态去掉 `max-*`）+ 载入提示改半透毛玻璃（只遮 view、`pointer-events-none`） |
| `d385144` | **`IMAGING.md` 立项**（格式范围 + 三种图规格 + 现状对照 + 未决点）；FUTURE §C8 / PLAN §M4-W2 改成指向它 |
| `e0b63d8` | **编辑基准可切**（SOOC / RAW，默认 RAW）：`EditBase` + `develop_edit_target(base)` + 总览下 `SegmentedControl` |
| `0a9968d` | 三项拍板登记 + 实施记录 `implementations/2026-09-24_edit-base-switch-and-decisions.md` |

更早一轮（M3-W3 bug 批）见 `implementations/2026-09-24_w3-bug-batch-histogram-orientation-panels.md`
与 `specs/HANDOFF-2026-09-24-w3-bugs.md`。

---

## 2. 【首要·第一步】接 `avif-native`（dav1d）—— 让 Rust 侧能解 AVIF

### 2.1 为什么非要它（三处都卡在这）

1. **「进来先读 preview」**（§3）：preview 是 AVIF（`<库根>/cache/full/<asset>/latest-v6.avif`），
   编辑视口是 Rust 直绘（wgpu 要像素），**浏览器能解 AVIF 但 GPU 纹理那条路绕不开解码器**。
2. **§2.5 切图优先**（§4）：latest 那份就是 AVIF。
3. **导入 avif**（`IMAGING.md` §1.1 的 6 种之一）：现在是占位图（解不开）。

> 注意 **HEIC 不在这件事里**：`image` 完全不支持 HEIC，要另接 libheif 系 —— 另行安排。

### 2.2 已查实的事实（别再从零查）

* 依赖声明在 **`Cargo.toml` 第 49 行**（workspace）：
  `image = { version = "0.25", default-features = false, features = ["jpeg","png","tiff","avif","rayon"] }`
  —— `avif` 这个 feature **只有编码器**（`dep:ravif`，纯 Rust）。
* `image 0.25.10` 另有 **`avif-native`** feature，定义是：
  ```toml
  avif-native = ["dep:mp4parse", "dep:dav1d"]      # dav1d = "0.11.0"
  ```
  （本机已核对 `~/.cargo/registry/src/*/image-0.25.10/Cargo.toml`）
* `dav1d` crate 0.11（上游 `rust-av/dav1d-rs`）**用 `system-deps` 找 dav1d 库**：
  * 走 pkg-config 找系统库，或
  * `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` 时**从源码内部构建** —— 那条路要
    **meson + ninja + nasm**（nasm 需 ≥ 2.14；dav1d 自己的 meson.build 里有这条检查）；
  * Windows 上游 README 给的路径是 **vcpkg**（`choco install pkgconfiglite` + `vcpkg install dav1d`）。
* **本机 WSL 侧工具链现状（已实测）**：`ninja ✓`、`cc/gcc ✓`，**`meson ✗`、`nasm ✗`、`clang ✗`**。
  `dav1d` crate **不在本地 cargo 缓存**里（首次拉取要网络）。
* 另有一个可能的省事路径（**未查实，值得先花 10 分钟确认**）：
  `shiguredo/dav1d-rs` 那份 fork 自称提供 **Windows x86_64 等平台的预编译二进制**
  （另有 `source-build` feature 才走源码构建）—— 若 crates.io 上的 `dav1d 0.11.x` 正是它，
  Windows 侧会轻松很多。**先确认 crates.io 的 `dav1d` 指向哪个仓库、有没有预编译下载**。

### 2.3 要做的动作（建议顺序：先趟链、再动代码）

1. **先只改 `Cargo.toml` 一个 feature**（`avif-native` 加进那一行），在 **WSL 侧**跑
   `cargo check -p raybend`：看它到底要什么（pkg-config？meson？）。
   不通就按提示装（`apt install meson nasm` 之类）或设 `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL`。
2. **紧接着在 Windows 侧把同一条链趟通**（这是真正的风险点，产品环境是 Windows）：
   照 `AGENTS.md` §5.3 的规矩（`pnpm build` → `WSLENV` 透传 `CARGO_TARGET_DIR` →
   `cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\... & cargo build -p raybend-desktop -p raybend --features custom-protocol'`）。
   **`-p raybend` 不能少**（worker 是它的 bin 目标，§5.3 第 7 条）。
   在 Windows 上大概率要装 meson/ninja/nasm（或走 vcpkg dav1d）。
   **这条不通就别硬啃**（`AGENTS.md` §2.11）：登记 + 汇报，改走 §2.4 的备选。
3. **写一条 round-trip 测试**（这是「解码器真的接上了」的唯一证据，秒级）：
   用 `ravif` 编一张**合成小图**（32×32 渐变）→ `image::load_from_memory(&bytes)` 解回来 →
   断言尺寸一致 + 像素近似（AVIF 有损，给容差）。
   放 `crates/raybend/src/thumbnail/` 或 `display/` 下，跟随现有测试风格。
4. **顺手确认一条口径**：我们**自己写**的 AVIF（`thumbnail/render.rs` 的 `encode_avif`，
   q90 / 4:4:4）必须能被新解码器读回来 —— 第 3 步的 round-trip 用同一条编码路径即可覆盖。

### 2.4 风险与备选（绕不过去时按这个顺序退）

| 顺序 | 备选 | 代价 / 适用 |
| --- | --- | --- |
| 1 | Windows 走 **vcpkg dav1d**（或预编译 fork） | 打包要带 `dav1d.dll`（或静态链接），要写进 `THIRD-PARTY-NOTICES.md` |
| 2 | 「先读 preview」改用 **DOM 覆盖层**（浏览器解 AVIF，`<img>` 盖在洞口上） | 零新依赖、UX 一样；但 **avif 导入**仍缺解码器（那条只能靠真解码器） |
| 3 | 换**纯 Rust** 解码器（`rav1d`，memorysafety 的 dav1d Rust 移植） | `image` 不认它，要自己写解 AVIF 的那一层（container 解析 + 解码），工作量大 |

> 人类已拍板「接 avif-native（dav1d）」——备选是**绕不过去时的退路**，退之前要在对话里说一声。

### 2.5 ⚠️ 顺带必须处理的一件事：latest 缓存的键里**没有「编辑基准」这一维**

`FullCache::path_for(asset_id, issue, pipeline)` → `<库根>/cache/full/<id>/<issue>-v6.avif`，
`issue` 只有 `sooc` / `raw` / `latest` 三种取值。而 **`e0b63d8` 让编辑基准可切（SOOC / RAW）** ——
同一张照片在两种基准下的渲染结果是**两张不同的图**，但缓存文件名是同一个 ⇒
**切了基准之后读到的可能是另一基准渲染出来的旧图**。

* 现状：切基准**不作废**缓存（`develop_commit` 才作废）。
* 两个可选做法（**实现 §3 之前先定，否则会做出「preview 显示另一基准」的怪现象**）：
  * 把基准编进缓存名（如 `latest-raw-v6.avif` / `latest-sooc-v6.avif`）—— 干净，但要动
    `full_cache.rs` 的命名与 `render_latest_cached` / `view_image` 的读法；
  * 切基准时**作废**该资产的 latest 缓存（简单，但来回切会反复重渲）。
* 这条**没有登记在任何文档里**（写本文件时才发现）—— 定下来之后请补进 `IMAGING.md` §4/§8。

---

## 3. 【首要·第二步】「进来先读 preview」—— 编辑进图先出图，后台再解 RAW

### 3.1 现状（人类报的体感来源）

* 进编辑 → `RenderCommand::SetPhoto` → 显影线程 `decode_linear_source`（**worker 进程全解码 RAW**，
  实测 1.6–2.4s）→ 管线 → 纹理。**这段时间视口是空的**。
* 载入提示已经是半透毛玻璃、只遮 view、`pointer-events-none`（`67e2422`）——
  **这一条已经就位，不用再做**。

### 3.2 目标行为（`IMAGING.md` §5，人类原话）

> 「接 avif 支持，进来先读 preview，因为本身这个分辨率就只有 1920，所以自然有点糊也正常」

即：**先拿 preview 当过渡帧画上去**（毫秒级），RAW 解码完成后再替换成真帧。

### 3.3 实现要点（这里每一条都是踩过的坑或已定的口径）

1. **过渡帧从哪来**（按 `IMAGING.md` §2/§4 的口径）：
   * 编辑过的照片：`FullCache::read(asset_id, "latest", PIPELINE_VERSION)` —— 命中即用
     （进/出编辑已经保证它被写过，见 `develop_preview_refresh`）；
   * 没编辑过：preview 不存在（`needs_preview` = false），**SOOC 位图 / RAW 内嵌预览就代替它** ——
     取图口已经有这条路（`display::cached_image` 的 Screen 档、`view_image`）。
   * **一条实现、两个来源**：不要把「解 AVIF 上纹理」再写第二份。
2. **过渡帧要走同一条 `Developed` 通道**，别另开命令：
   * `DevelopOutcome` 已经有 `origin: String` 字段，现有取值是 **`"raw-linear"`**（RAW 全解码）、
     **`"bitmap-linear"`**（位图）、**`"panic"`**（异常）—— 过渡帧照这个模式加一个
     （如 `"preview-avif"`），渲染线程照旧画。
   * ⚠️ **任务号（`id`）必须与当前任务一致**：`RenderCommand::Developed` 的处理里
     `if outcome.id != *latest_job { return }` —— 号对不上会被当过期结果丢掉（过渡帧就永远不显示）。
   * 顺序：**先发过渡帧，再解 RAW**（解码在同一个 `run_develop_job` 里，注意别把 decode 阻塞在发帧之前）。
3. **过渡帧的「逻辑尺寸」不能是 1920**（§2.4 那条真 1:1 的延伸）：
   `DevelopedImage.source_width/height` 决定视口的逻辑尺寸（1:1、视野框都按它算）。
   过渡帧若报 1920，真帧回来（例如 3888×5184）会**重新 refit → 画面跳一下**。
   → 过渡帧的 source size 用**原图尺寸**：`catalog` 的 `assets.width/height` 有值
     （`catalog_0001_init.sql` 第 33–34 行）——**口径要在实现时确认**（它可能是位图的尺寸），
     确认不了就退回「过渡帧不 refit、只换纹理」那条路。
4. **档位交互**：`tier_for_params` 的 `interactive` 只影响参数任务；`SetPhoto` 那条本来就是
   `ImageTier::Preview` 起步 —— 过渡帧天然是预览档，不用额外处理。
5. **直方图**：`IMAGING.md` 的实时口径是「像素由显影线程当前帧直接给出」——
   过渡帧期间直方图会是 preview 的直方图，真帧到了再刷新。**这是可接受的**（但要知道，
   否则会被当成「直方图不跟着走」的 bug）。
6. **别把 preview 当正式帧缓存**：过渡帧只是画一下，**不许**写进任何缓存
   （它本来就是缓存读出来的）。

### 3.4 验收（Agent 能做的部分）

* 单元/集成：给「过渡帧先到、真帧后到」写一条状态机测试（任务号、`origin`、source size 三条）。
* 离屏：`cargo run -q -p raybend --example editor-offscreen -- <库根>` 扩一组断言 ——
  过渡帧期间纹理是 1920 档、逻辑尺寸仍是原图尺寸。
* 计时：日志里 `decode_ms` 与「第一帧到达时间」的差（人眼体感由人类验）。

---

## 4. 第三步（紧随其后）：§2.5 切图优先载入

**人类原话**：「editor 中点 film 切图片的时候要优先载入图片的 latest / SOOC / RAW 中存的原片
（按顺序来，能找到哪个显示哪个）」；拍板：**等 AVIF 解码器一起做**（已在 `specs/HANDOFF-2026-09-24-w3-bugs.md` §2.5）。

* **与 §3 是同一套机制**：§3 做的是「先出过渡帧、再出真帧」；这一条只多一个**顺序策略**：
  切图时按 `latest`（缓存里有就读）→ `SOOC`（位图）→ `RAW`（内嵌预览）找第一个能用的当过渡帧。
* 相关既有件：`store::develop::choose_issue`（显示哪个 issue 的规则）、`needs_preview`、
  `develop_edit_target(base)`（编辑基准已可切）、`FullCache::read`。
* 做完请更新 `IMAGING.md` §8 那两行（§2.5 与「先读 preview」）+ 写实施记录。

---

## 5. 其余登记项（**本批别做**，除非上两步全做完且还有余力）

| 项 | 在哪登记 | 备注 |
| --- | --- | --- |
| avif **导入**支持 | `IMAGING.md` §1.4/§8 | 与 §2 同一个解码器；接完解码器后大概率「自然就好了」，但要补一条真文件冒烟 |
| HEIC 导入 | 同上 | 要 libheif 系，**另一件事** |
| issue 打「基于 sooc / 基于 raw」标签 + 点 issue 同步切换按钮 | `FUTURE.md`「issue 与 SOOC」、`IMAGING.md` §4.3 | 等 issue 体系 |
| 1:1 只算可见区域（视口裁切渲染） | `FUTURE.md` §D1.5 | 架构级，别顺手做 |
| develop 分期 + 节点缓存 | `FUTURE.md` §D1.5 | 同上 |
| `design/editor.pen` 补画：缩放控制 / 信息三组 / 编辑基准按钮 / 毛玻璃提示 | `design/editor.md` 已同步文字 | **需要人类把该文件在 VS Code 里打开**（Pencil 才连得上） |
| `REPOSITORY.md` §4.1「编辑落在 RAW 上」的措辞 | — | 现在多了用户可切的基准，下次顺手对齐 |

---

## 6. 纪律与坑（新会话别踩）

1. **不跑 `cargo fmt`**（见 §0）；**不顺手改别的**（handoff 纪律）。
2. **门禁判据是 `tsc` 与直接跑的 clippy**，不是 pi-lens 的缓存报告（§0）。
3. **新增大型依赖**按 `AGENTS.md` §2.9 要先讨论 —— 本次**人类已拍板**，可以动手；
   但要在 `THIRD-PARTY-NOTICES.md` 登记 dav1d（**BSD-2-Clause**，与 AGPL 兼容；
   若走 vcpkg/预编译，还要写清来源与版本）。
4. **Windows 构建三条硬规矩**（`AGENTS.md` §5.3）：产物落 Windows 本地盘、用 `WSLENV` 传环境变量、
   `--features custom-protocol` 必须加、**`-p raybend` 不能少**（worker 是它的 bin）；
   改完前端先 `pnpm build`，构建完 `pnpm check:win`。
5. **E2E 归人类**（§2.8）：Agent 只做冒烟 + 单元测试 + 离屏像素证据；
   「画面看起来对不对」不许自己下结论。
6. **失败别硬啃**（§2.11）：构建链趟不通 → 登记 + 在对话里说，别把已做完的部分污染掉。

---

## 7. 需要人类配合的事

1. **`design/editor.pen` 要在 VS Code 里打开**（Pencil 才连得上）—— 补画上表那四处。
2. **真机 E2E**（我没法替您看）：
   * 总览下 **SOOC / RAW 按钮**的位置观感、切换后画面确实换底图重解、只有一侧文件时的禁用态；
   * 拖杆「偏软跟手、松手变锐」的体感；毛玻璃提示；退出编辑后 preview 立即可用；
   * §2/§3 做完之后的：进编辑**先出 1920 再变清晰**的观感（是否可接受、糊的程度）、
     切图时过渡图是否够快。
3. **Windows 侧的 dav1d 构建**如果卡住（要装 meson/nasm/vcpkg），我会在对话里点名要什么。
