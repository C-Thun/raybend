# AVIF 解码器（dav1d）+「进来先读 preview」+ 切图优先

完成时间：2026-09-24 21:02:37 CST

对应交接文档：`plans/HANDOFF-2026-09-24-avif-decoder-and-preview.md` 的 **§2 / §2.5 / §3 / §4**（四步一次做完）。
本记录按「做了什么 → 关键决策 → 验证 → 遗留」组织；口径类结论已同步进 `IMAGING.md`（§1.4 / §4.2 / §4.3 / §5 / §8）。

---

## 1. 做了什么

| # | 交付 | 一句话 |
| --- | --- | --- |
| 1 | **接 `avif-native`（dav1d）** | Rust 侧第一次能解 AVIF：我们自己的缓存图、导入的 avif 文件都能解 |
| 2 | **latest 缓存名加编辑基准**（§2.5 那个没登记的坑） | `latest-v6.avif` → `latest-<base>-v6.avif`；切基准不会再读到另一基准的旧图 |
| 3 | **进编辑先出过渡帧**（§3） | 进编辑/换照片先拿一张已存在的图顶上（毫秒级），真帧（RAW 全解码 + 管线）随后替换 |
| 4 | **切图优先**（§4） | 与 3 同一套机制，只多一条顺序策略：`latest` → `SOOC` → `RAW` |

§4 不需要第二份实现：胶片带换照片 → browse store 锚点变 → 编辑器那个 effect 重新解析
`develop_edit_target` 并调 `editor_set_photo` —— 过渡帧计划就在那一条命令里生成。

---

## 2. 涉及文件

#### Rust 库（`crates/raybend/`）

| 文件 | 改动 |
| --- | --- |
| `Cargo.toml`（workspace） | `image` 的 features 加 **`avif-native`** |
| `src/display/pixels.rs` | 新增 `pixels_from_avif`（AVIF 字节 → GPU 纹理用的 RGB8）+ 4 条测试（round-trip / 坏字节 / 尺寸校验） |
| `src/display/full_cache.rs` | `path_for` / `read` / `write` 增加 `base: EditBase` 一维；新增「两个基准各有各的文件」测试 |
| `src/display/mod.rs` | 导出 `pixels_from_avif` |
| `src/thumbnail/render.rs` | `PixelOrigin` 加 `AvifCache`（过渡帧的像素出处） |
| `src/store/develop.rs` | `EditBase::of_file`（**从源文件推基准**，不靠调用方传） |

#### Tauri 壳（`src-tauri/`）

| 文件 | 改动 |
| --- | --- |
| `src/editor.rs` | `TransitionPlan` / `TransitionSource` / `TransitionKind`；`transition_plan`（命令层算计划）、`transition_outcome`（显影线程试候选）；`SetPhoto` 带计划；`DevelopOutcome.transition`；`merge_jobs` 保住计划；`editor_set_photo` 改 async + `blocking`；5 条新测试 |
| `src/develop.rs` | 新增 `source_path_of`（进程内取某个基准的源文件，给过渡帧计划用） |
| `src/thumbs.rs` | `render_latest_cached` 按源文件推基准，读写带 base 的缓存 |
| `Cargo.toml` | dev-dependencies 加 `image` / `tempfile`（测试要真文件） |

#### 脚本与文档

| 文件 | 改动 |
| --- | --- |
| `scripts/build-dav1d-win.cmd`（新） | **一次性**在 Windows 上构建 dav1d 静态库（meson + ninja + nasm + VS Build Tools） |
| `scripts/lib/dav1d-win.mjs`（新） | Windows 侧构建要的 dav1d 环境变量 + `WSLENV` 拼装（`debug-win` / `spike-win` 共用） |
| `scripts/debug-win.mjs`、`scripts/spike-win.mjs` | 改用上面那个 helper（不再各写一份环境变量） |
| `AGENTS.md` §5.3 | Windows 构建原命令补 dav1d 环境变量；硬规矩从八条变**九条**（第 9 条 = dav1d 前置） |
| `IMAGING.md` | §1.4 / §4.2 / §4.3 / §5 / §8 / §9 按实现结果更新 |
| `THIRD-PARTY-NOTICES.md` | 登记 **dav1d 1.5.0（BSD-2-Clause）** 与 `mp4parse`（MPL-2.0） |

---

## 3. 关键决策与理由

### 3.1 Windows 侧 dav1d：**一次性静态库 + 环境变量**，不装 pkg-config、不打 DLL

`dav1d-sys` 用 `system-deps` 找库，Windows 上既没有 pkg-config 也不该要求别人装。
已读 `system-deps` 源码确认这条路成立：`SYSTEM_DEPS_DAV1D_NO_PKG_CONFIG=1` 会跳过探测，
再用 `SEARCH_NATIVE` / `LIB` / `LINK` 三个变量把库喂给它（`from_env_variables` + `override_from_flags`）。

* 静态库（`LINK=static`）⇒ **安装包里不需要带 `dav1d.dll`**，主程序与 worker 都不用改打包；
* 库不进仓库（3MB 二进制），用 `scripts/build-dav1d-win.cmd` 一次性构建到 `C:\rb-deps\dav1d-1.5.0\`；
* 版本固定 **1.5.0**（= `dav1d-sys` 内部构建用的 tag；上游 README 写明「>=1.3.0，>1.5.0 不保证」）。

**没有走** `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always`（让 `dav1d-sys` 自己构建）的原因：
它的 `build_from_src` 只跑 `meson setup -Ddefault_library=static`（**buildtype 默认是 debug**，
出来的是 `/MDd` 未优化库），而且 `ninja` 是它单独 spawn 的进程 ——
meson 自动激活的 VS 环境传不到 ninja（实测：`CreateProcess failed: cl`），
必须整个 cargo 进程都跑在 `vcvars64` 里。自己构建一次、固定路径引用更省事，也更快（日常构建不再碰 meson）。

WSL 侧相反：直接用发行版包（`sudo apt install libdav1d-dev`，1.4.1）走 pkg-config，**不需要**任何环境变量。

### 3.2 过渡帧的逻辑尺寸用**原图尺寸**（不是 1920）

`GpuContext::set_image` 在**逻辑尺寸变化**时会 `refit`（`Free` 模式下会把用户拖过的视角拉回中心）。
过渡帧若报 1920，真帧（如 3888×5184）回来就会重摆一次。
原图尺寸由 `media::meta::read_photo_meta` **只读文件头**得出（RAW 走 TIFF 家族兜底 + EXIF，
位图走 `image::image_dimensions`），并且**已按方向摆正**（`oriented_size`）——
与真帧那条路（`from_raw16` / `from_srgb8` 之后）同一个口径。
报原图尺寸时真帧回来 `set_size` 直接早退，**连 refit 都不会发生**。

查不到尺寸时退回用过渡帧自己的尺寸 —— 两者都已摆正、宽高比一致，
`Fit` 档下 `zoom_for_fit` 只取决于宽高比，refit 结果逐像素相同（看不出区别）。

### 3.3 过渡帧走**同一条 `Developed` 通道**，任务号必须一致

不另开命令：`DevelopOutcome` 加一个 `transition: bool`，渲染线程据此只做三件事
（换纹理 + 逻辑尺寸 + `origin`），**不清 `wanted_tier`、不置 `decode = ready`、不动 `applied_params_rev`、
不调 `ensure_output`** —— 真帧还在路上，界面上的「载入中」必须继续挂着。
任务号用 `job.id`：`outcome.id != *latest_job` 那条检查会丢掉过期结果，号对不上过渡帧就永远不显示。

### 3.4 latest 缓存名加基准（§2.5 的坑）

同一张照片在 SOOC / RAW 两种基准下渲染出的是**两张不同的图**，而缓存名里没有这一维 ⇒
切了基准会读到另一基准渲染的旧图，且**看不出是错的**（`develop_preview_refresh` 会命中它、
preview 就一直停在对面的那张）。两个候选做法里选了「**编进缓存名**」：

```text
<库根>/cache/full/<asset>/latest-raw-v6.avif
<库根>/cache/full/<asset>/latest-sooc-v6.avif
```

理由是它同时修掉两个面：过渡帧读到的一定是当前基准那一侧；preview 也不会被对面的旧文件卡住。
作废（`develop_commit` / `develop_reset`）仍然**整个资产目录一起删** —— 编辑栈是共用的，两种基准都过期了。

**基准从源文件推**（`EditBase::of_file`：RAW → `raw`，其余 → `sooc`），不靠调用方传：
「渲染用的哪个文件，就是哪个基准」是唯一不会传错的口径。

---

## 4. 验证（Agent 侧，已做）

### 4.1 门禁（全绿）

| 命令 | 结果 |
| --- | --- |
| `cargo test -q -p raybend --lib` | **954 passed**（+4）/ 0 failed / 1 ignored |
| `cargo test -q -p raybend-desktop --lib` | **71 passed**（+5）/ 0 failed |
| `pnpm test` | 906 pass / 0 fail |
| `npx tsc --noEmit` | 干净 |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 干净 |
| `cargo clippy -q -p raybend -p raybend-desktop --all-targets` | 只剩既有那一条（`store/backfill.rs` 的 `type_complexity`） |

### 4.2 解码器真的接上了（§2.3 要的 round-trip）

* `avif_round_trip_decodes_our_own_cache_format`：32×32 渐变 → `encode_avif`（ravif q90 / 4:4:4）
  → `pixels_from_avif` → 尺寸一致、**单点最大偏差 ≤8、平均偏差 <1**（有损但无整体偏移）。
* `avif_garbage_fails_quietly`：空字节 / 纯垃圾 / 只剩 ftyp / **截断一半**都安静返回 `None`，
  **不 panic 也不挂住**（截断那条特意验：`image` 的 `read_until_ready` 是个重试死循环，
  若 dav1d 不报错就会把显影线程卡死）。

### 4.3 真 AVIF 文件冒烟（导入那条路）

从 libavif 官方测试集取 4 个**真文件**（`/mnt/c/src/tmp/avif/`），跑应用同一条管线：

| 文件 | 覆盖 | `thumb-probe` 结果 |
| --- | --- | --- |
| `fox.profile0.8bpc.yuv420.avif` | 8bit 4:2:0（最常见） | ✅ 出图 384×255，占位图=false |
| `fox.profile1.8bpc.yuv444.avif` | 8bit 4:4:4（与我们自己写的一致） | ✅ 出图 384×255 |
| `fox.profile0.10bpc.yuv420.avif` | **10bit** | ✅ 出图 384×255 |
| `abc_color_irot_alpha_irot.avif` | **alpha + irot 旋转元数据** | ✅ 出图 384×192 |

`hist-probe` 跑 4:4:4 那个：86 桶、峰值 16132（像素口那条路也通）。

### 4.4 Windows 侧

* `pnpm debug:win`（含 `pnpm build` → Windows cargo → `check:win`）**全绿**：
  `✓ Windows 产物与前端一致`，worker 含 `raybend-worker-proto-v2`。
* **静态链接证据**（不是「进程起来了」那种）：`dumpbin /dependents` 里**没有 `dav1d.dll`**；
  exe 字节里能找到 dav1d 的版本串 `1.5.0`。
* Windows 侧工具链就位：meson 1.12.0 / ninja 1.13.2 / nasm 3.02 + VS Build Tools 18 的 `vcvars64`。

### 4.5 状态机（过渡帧三条口径）

`src-tauri/src/editor.rs` 的测试覆盖：任务号一致、`origin` 报得出来、
逻辑尺寸用原图尺寸（含「查不到时退回自己的尺寸」）、候选全不可用时返回 `None`、
AVIF 缓存字节能变成过渡帧像素、`merge_jobs` 不会把计划丢掉（参数任务紧跟换照片那条）。

---

## 5. 未验证（**归人类真机**，Agent 不下结论）

1. 进编辑「先出 1920 再变清晰」的观感 —— 糊到什么程度可接受、第一帧够不够快；
2. 胶片带换照片时过渡图是否够快（尤其 RAW：内嵌预览要走 worker 一趟）；
3. 切 SOOC / RAW 基准后，过渡帧显示的是否确实是当前那一侧（缓存名改了，但**观感**得人看）；
4. Windows 真机上 AVIF 照片导入与显示。

---

## 6. 遗留 / 登记项（本批没做）

| 项 | 去哪 |
| --- | --- |
| **HEIC 导入**（`image` 完全不支持，要 libheif 系） | `IMAGING.md` §1.4 |
| avif 导入的**真机**冒烟（人类自己的 avif 文件） | `IMAGING.md` §8 已标注「待补」 |
| 「存 issue」节点的 preview 生成 | `IMAGING.md` §4.1 / §8（等 issue 体系） |
| 1:1 只算可见区域（视口裁切渲染） | `FUTURE.md` §D1.5 |
| RAW + JPG 成对时 preview 从哪个文件渲染（现状 = 前端给的显示路径） | `IMAGING.md` §9 未决点 2 |
| WSL(1.4.1) 与 Windows(1.5.0) 的 dav1d 版本差异 | 只影响解码细节，不影响缓存口径；要统一时改 `build-dav1d-win.cmd` 的 tag + WSL 侧自己编 |
| `design/editor.pen` 补画（缩放控制 / 信息三组 / 编辑基准按钮 / 毛玻璃提示） | 需要人类在 VS Code 里打开该文件（Pencil 才连得上） |

> **没有 commit**：本仓当前有另一个会话在推进 M3-W4（`develop/lens.rs`、`develop/mod.rs`、`plans/M3-W4.md`
> 都在它手里），共用同一个工作区 —— 提交会把它的半成品一起带走。改动全部留在工作区，等人类发话。
