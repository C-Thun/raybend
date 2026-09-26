# 色彩管理 —— 主题规格与执行方案

> **性质**：这是**主题级规格**（跨多个波次的一条线），不是单工作单元的方案。
> 按 `AGENTS.md` §5.4，每个波次开工前另写 `specs/CM-W<n>.md`（一次一个、走 plannotator 评审），
> 本文件只定**方向、边界、分层、验收判据与坑**，不提前细化未开工波次的实施步骤。
>
> 建立：2026-09-26（人类要求把方案落成可排期的执行方案）｜状态：**方案待拍板，未排期**
> 相关：`AGENTS.md` §6.1（渲染架构）、`FUTURE.md` C1/C2（原登记）、`IMAGING.md`（格式与三种图）、
> `DESIGN.md` §14.9（编辑右栏页签）/ §14.10（拉杆）、`BROWSE.md` §6（右栏信息区）、
> `REVIEW.md` R1-01（本条的来源）

---

## 1. 目标与不做

**一句话目标**：**让「看到的」与「存下的」颜色都是对的** —— 输入认得出、工作空间装得下、
显示器显示得准、导出说得清。

| 做 | 不做（明确出局，写进 `FUTURE.md` 即可） |
| --- | --- |
| 读位图**内嵌 ICC** 并正确解释 | 自研显示器校准引擎（需要色度计硬件，交给 ArgyllCMS / DisplayCAL） |
| **显示器 profile** 的获取与应用（Windows 优先） | 内嵌 Adobe 的 DCP 出厂档（专有许可，只支持用户自备） |
| 工作空间升级到**线性广色域** | 打印机驱动 / 拼版 / 印刷流程 |
| 每相机**输入 profile**（矩阵 → DCP → ICC 三级） | HDR / 10bit（登记为 L5，不排期） |
| 导出嵌 profile + 渲染意图 + 黑场补偿 | macOS / Linux 的实装（登记为远期，不预设框架） |
| **软打样 + 色域警告** | 显示器**硬件校准**的度量化（点一下「去校色」的引导即可） |

---

## 2. 现状（全部有源码证据，别凭印象）

| 环节 | 现在怎么做的 | 文件 |
| --- | --- | --- |
| RAW 解码 | rawler `LINEAR_STEPS`：`… → WhiteBalance → Calibrate → CropDefault`（**不含 `SRgb`**），输出**线性 sRGB f32** | `crates/raybend/src/raw/rawler_backend.rs` |
| 位图 | **一律当 sRGB**：`LinearImage::from_srgb8(...)`，内嵌 ICC 被忽略 | `crates/raybend/src/thumbnail/render.rs:604` |
| 工作空间 | **线性 sRGB**（`develop/color.rs` 自称「这一份是唯一的色彩数学」） | `crates/raybend/src/develop/color.rs` |
| 显示 | 纹理 `Rgba8UnormSrgb`（硬件做 sRGB 解码/编码）；surface 策略 `SurfaceColorSpace::Auto` | `crates/raybend/src/render/gpu.rs:248,856` |
| 导出 | 只有 5 种格式的约定，**没有** 输出 profile / 渲染意图 / 嵌不嵌 profile | `IMAGING.md` §1.1 |

**后果（两类，都不是理论）**：

1. **广色域位图静默偏色** —— Adobe RGB / ProPhoto / Display P3 的 JPG/TIFF 被当 sRGB 解释，
   颜色明显发灰发闷，而界面不会有任何提示。**这是现存 bug，不是缺功能。**
2. **广色域/校色显示器上看到的颜色是错的** —— 微软文档原文：Advanced Color 未激活时
   「Windows **不对 app 的输出做任何色彩管理**……如果你的 app 想要准确的色彩复现，
   **必须自己做色彩管理**」。
   https://learn.microsoft.com/en-us/windows/win32/wcs/advanced-color-icc-profiles

---

## 3. 分级：能做到什么程度（L1–L4 = 排期范围，L5/L6 = 登记）

| 级 | 内容 | 用到的资源 | 代价 | 交付后能说什么 |
| --- | --- | --- | --- | --- |
| **L1** | 位图**内嵌 ICC** 读取 + 输入变换；导出**嵌 profile** | `img-parts` + `lcms2` | 小（1–2 周） | 「广色域图片不再偏色」「导出的文件带得上 profile」 |
| **L2** | **显示器 profile** 获取 → 烘 3D LUT → GPU 应用 | `lcms2` + Windows `ColorProfile*` API + wgpu | 中（2–3 周） | 「在校色屏/广色域屏上，画面与 UI 是准的」 |
| **L3** | 工作空间 → **线性广色域**；每相机**输入 profile**（矩阵 → DCP → ICC） | `lcms2` + `dng`（DCP） | 中偏大（3–5 周，**会作废全部 issue 哈希与 latest 缓存**） | 「编辑不吃 sRGB 的色域上限，相机色彩可校准」 |
| **L4** | **软打样 + 色域警告** | `lcms2` 第二个 3D LUT + 现有 `overlay.wgsl` | 中（1–2 周） | 「打印/输出前能预演」 |
| L5 | HDR / 10-bit / 交给系统（Advanced Color） | wgpu `SurfaceColorSpace::ExtendedSrgbLinear`（fp16） | 大 | 登记不排期 |
| L6 | 显示器硬件校准工作流（引导用 ArgyllCMS / DisplayCAL） | 外部工具 | 小 | 登记不排期 |

**推荐顺序：L1 → L2 → L3 → L4。**
L1+L2 合起来是「**看到的颜色是对的**」，**应当早于导出工作区**做 —— 它们修的是现存错误。
L3 是唯一会**动数据**的一级（哈希与缓存），越早定工作空间越便宜。

---

## 4. 资源与许可

| 资源 | 用途 | 许可 | 备注 |
| --- | --- | --- | --- |
| **lcms2**（Rust 绑定）https://github.com/kornelski/rust-lcms2 | ICC 解析 / transform / 烘 3D LUT | **MIT** | 最新 **6.2.0**，MSRV 1.65；`FUTURE.md` C1 已选它 |
| Little CMS 本体 https://github.com/mm2/Little-CMS | 底层 C 引擎 | MIT | 由上面那个 crate 内置 |
| **Compact-ICC-Profiles** https://github.com/saucecontrol/Compact-ICC-Profiles | **内置色彩空间**：sRGB / scRGB(线性) / Display P3 / Adobe RGB / ProPhoto / Rec.2020 | **CC0-1.0** | ⭐ 单个 **372–456 字节**，可直接编进二进制，不必装数据文件 |
| Elle Stone 的 well-behaved ICC https://github.com/ellelstone/elles_icc_profiles | 参考用标准空间 | ⚠️ **CC BY-SA 3.0** | **不捆绑**：Share-Alike 与 AGPL-3.0 组合有解释风险（CC BY-SA **4.0** 才是单向兼容 GPLv3 的那版）→ 用上面那份 CC0 替代 |
| **img-parts** https://github.com/paolobarbolini/img-parts | 读写 JPEG APP2 / PNG iCCP / WebP ICCP 里的 ICC 与 EXIF | MIT/Apache-2.0（**登记前按 `THIRD-PARTY-NOTICES.md` 流程核实一次**） | ⚠️ **TIFF tag 34675 不在它范围内**，要自己走 IFD |
| **`dng` crate** https://docs.rs/dng/latest/dng/ | 读 **DCP**（DNG Camera Profile） | 与 dnglab 同源（需登记核实） | 官方声明 DCP 支持是 **best-effort** |
| **ArgyllCMS** https://www.argyllcms.com/ | 显示器/打印机 profile 生成 | **AGPL-3.0** | 与本项目一致，**可**捆绑为可选组件；建议只做「引导用户自行安装」（要硬件） |
| DisplayCAL https://displaycal.org/ | ArgyllCMS 的 GUI | GPL-3.0 | 同上，只引导 |

**Windows 侧接口（权威文档已核实）**：

| 用途 | API | 注意 |
| --- | --- | --- |
| 取显示器当前 ICC | `ColorProfileGetDisplayDefault` / `ColorProfileGetDisplayList`（Win10 2004+，WCS 新 API） | 比旧的 `GetICMProfile` 更对口；类型子类要看清 `CPST_STANDARD` / `CPST_EXTENDED` |
| 是否支持新色彩管线 | `ColorProfileGetDeviceCapabilities` | |
| **ACM/HDR 已激活时** | **ICC 查询会返回「无 profile」**（无论实际装了什么）→ **约定按 sRGB 解释** | 🔴 关键陷阱，见 §8-2 |
| 显示器属性（ACM 下） | `IDXGIOutput6` | |
| profile 变更通知 | **WCS 注册表键**（微软明说：桌面 app 应监听注册表变更） | **不要轮询** |
| ⚠️ **句柄泄漏** | `GetICMProfile` / `WcsGetDefaultColorProfile` / `EnumICMProfiles` 在 Windows 10 的 `mscms.dll` 里有泄漏 | **只在启动 / 显示器变更时读一次并缓存** |

**GPU 侧（本地 wgpu 30.0.1 源码已核实）**：
`wgpu_types::SurfaceColorSpace` = `Auto / Srgb / ExtendedSrgbLinear / ExtendedSrgb / DisplayP3 /
Bt2100Pq / Bt2100Hlg / ExtendedDisplayP3`。
⚠️ **`DisplayP3` 明确标注「Not reported on DX12」** —— Windows 上唯一可用的广色域路径是
**`ExtendedSrgbLinear`（scRGB，fp16）**，对应 DXGI `RGB_FULL_G10_NONE_P709`。这决定了 L5 的走法。

---

## 5. 架构落点（方案的核心，界面只是壳）

### 5.1 管线位置

```text
源（RAW: 相机矩阵 / DCP / ICC  ┃  位图: 内嵌 ICC 或手动覆盖）
   ↓ 输入变换（lcms2）              ← 属于「编辑」：进 DevelopStack，参与 issue 哈希
工作空间（线性广色域）
   ↓ 全部 develop 处理
   ↓ 输出 / 软打样变换（lcms2）      ← 预览级 / 导出级
显示变换（lcms2 → 3D LUT）          ← 🔴 设备级：绝不进编辑栈、绝不进缓存
GPU 呈现
```

### 5.2 三条硬纪律（建议同时补进 `AGENTS.md` §6.1）

1. **显示变换只在呈现路径**（最后那次 blit），**不进 `develop::pipeline`**。
2. **缓存（`thumbs.db` / `cache/full/`）一律存工作空间像素，绝不做显示变换** ——
   否则换一台显示器整库缓存全废。
3. **直方图仍按工作空间统计**，不随显示器变 —— 否则同一张照片在两块屏上直方图不同。

### 5.3 呈现方式：从 `Rgba8UnormSrgb` 改成 `Rgba8Unorm`（**必须在写 shader 之前定**）

ICC 变换的输出是**设备编码值**（已含显示器 TRC）。当前 surface 纹理是 `*-srgb`，
硬件会再叠一次 sRGB OETF ⇒ **显示器 TRC 被应用两次**。

| 方案 | 做法 | 精度 | 代价 |
| --- | --- | --- | --- |
| **A（推荐）** | LUT 输出**设备编码值** → `Rgba8Unorm` surface（标 `Srgb`，字节直通） | 准确 | 要改 `render/color.rs` 里「清屏值预先线性化」那套（它现在依赖 `-srgb` 格式） |
| B（省事） | LUT 只输出**线性显示器空间** → 保留 `-srgb` 格式靠硬件编码 | 显示器 TRC ≈ sRGB 时才准 | 对校准到 2.2 / 自定义曲线的屏不准 |

**选 A。** 另外：33³ 的 LUT 贴图约 **143 KB（fp32）/ 72 KB（fp16）**，成本可忽略；
**采样用四面体插值**（比三线性插值精度高、性能相近）。

**两种 profile 形态要分开处理**（性能与精度的关键）：
* **shaper-matrix 型**（绝大多数校色软件产出的显示器 profile）= 逐通道 1D 曲线 + 3×3 矩阵
  → **直接在 shader 里精确实现**（1D LUT + 矩阵），不必烘 3D LUT；
* **LUT 型**（`A2B0`/`mft2` 等，打印机 profile、软打样目标）
  → lcms2 烘 **3D LUT**，shader 里四面体插值。

### 5.4 覆盖层要用同一个变换

`--overlay-line` / `--overlay-halo`（`DESIGN.md` §14.8）是铺在照片上的 sRGB UI 颜色。
照片走了显示变换、覆盖层没走 ⇒ 两块颜色不再可比，裁剪框在广色域屏上会偏。
**做法**：这几个 token 的颜色在 CPU 侧过一次同一个变换，作为 uniform 传给 `overlay.wgsl`
（颜色数量极少，成本为零）。

### 5.5 与 WebView（DOM UI）的一致性 —— 这本身就是验收判据

**Chromium / WebView2 在 Windows 上会读显示器 ICC 并做色彩管理**
（源码证据：`ui/display/win/color_profile_reader.cc`、`ui/gfx/icc_profile.cc`）。
⇒ **DOM 那层 UI 已经是「对」的**，因此**原生洞口必须做同样的事**，否则洞口内外的同一块 UI
颜色会不一样。Chromium 还**主动拒绝没有 D50 白点的显示器 profile**
（crbug.com/847024：Windows 会返回这类 profile 但不用于系统绘制）——
**建议我们采用同一规则**，否则洞口与 DOM UI 会分叉。**这条只能用真机验。**
另：WebView2 在 `--force-color-profile` 上有已知 bug（WebView2Feedback #3013），**不要依赖强制开关**。

---

## 6. 界面方案

### 6.1 三类设置三种归属（先定归属，再谈控件）

| 层 | 是什么 | 放哪 | 为什么 |
| --- | --- | --- | --- |
| **设备级** | 显示器 profile（自动跟随系统）、ACM 状态、默认工作空间、默认渲染意图、软打样默认目标 | **全局设置**（`Ctrl+,` 的第二个页签） | 跟机器/屏幕走，与照片无关 |
| **文件 / 编辑级** | **输入 profile**（RAW 的相机 profile / 位图的 ICC 覆盖） | **editor「色彩管理」面板** + 存进 `DevelopStack` | 改它会改画面 ⇒ **它就是编辑的一步**，必须进 issue 哈希 |
| **输出级** | 导出 profile、渲染意图、黑场补偿、是否嵌 profile | **导出预设**（`design/export.md` §5.1 的卡片） | 天然是「每预设一份、批量生效」 |

### 6.2 Editor：第三个页签组加一个页签

现状（`DESIGN.md` §14.9）：右栏三组 = ① `总览 · 定稿 · 信息` ② `影调 · 色彩 · 清晰度 · 镜头` ③ `曲线`。
→ 第三组变成 **`曲线 · 色彩管理`**。

> ⚠️ **命名冲突**：第二组已经有一个 **`色彩`**（色温/饱和度那组）。新页签**不能**也叫「色彩」。
> 用 **`色彩管理`**。

| 块 | 内容 |
| --- | --- |
| **输入**（随当前编辑基准变） | 基准标签（SOOC / RAW，已有）＋ profile 选择器：SOOC 侧为 `内嵌（自动）` / `sRGB` / `Adobe RGB` / `ProPhoto RGB` / `Display P3` / `Rec.2020` / `自定义 ICC…`；RAW 侧为 `相机矩阵（元数据）` / `相机 profile（DCP）` / `自定义 ICC…`。另加「被手动覆盖」标记与「恢复自动」 |
| **预览目标** | `显示器` ⇄ `软打样…`（选输出 profile）；渲染意图；黑场补偿；**色域警告**开关 |
| **工作空间** | 只读显示（`线性 Rec.2020`）＋ 一句「改工作空间会作废缓存与定稿匹配」＋ 跳设置 |
| **只读信息** | 源色域 / 内嵌 profile 名 / 是否含 vcgt / 当前显示器 profile 名（带「跟随系统」标识） |

**导入自定义 profile**：一个 `导入 ICC…` 按钮。
**必须复用 LUT 导入那条路**（`crates/raybend/src/store/luts.rs`：文件复制到 `luts/<id>/`、
`file_hash` 去重、`cover_rel_path` 封面、`hidden` 软删除、被引用时不真删）。
ICC 是**同一件事的第二个 kind** —— 按 `AGENTS.md` §2.12 把「文件型资源导入」抽成
**一份实现 + 一层数据适配**（`profiles` 表），**不要新写一套导入**。
（ICC 的封面可以是自动画的小色域图。）

### 6.3 Browse：右栏加一个只读分组 + toolsbar 一个批量入口

**右栏（`BROWSE.md` §6）新增 `色彩` 分组**，与「标签信息」同级，tiles 与看图两种模式都显示：

| 行 | 内容 |
| --- | --- |
| 源色域 | `Adobe RGB (1998)` / `相机矩阵` / `内嵌 sRGB` |
| 内嵌配置文件 | 有 / 无 ＋ 名字（无时显示「按 sRGB 解释」） |
| 工作空间 | `线性 Rec.2020`（只读） |
| ⚠️ 异常行 | **仅在可疑时出现**：`内嵌 profile 缺失` / `profile 与扩展名不符` |

**批量（回答「要不要批量化」）：要，但只能复用既有范式，不能新造。**

* **批量设输入 profile**：`browse toolsbar` 加一个按钮 `设置色彩配置` → 弹窗选一个 →
  对**选中照片**批量写。这条**天然复用**现有的 `mark(action)` 通道
  （`store/marking.rs` + 撤销栈 + `refreshMarkings`），三态规则照旧。
* **为什么批量只能在 browse**：**editor 没有多选模型**（编辑器的「选择」就是 browse store
  的主选目标，永远只有一张）⇒ editor 面板**只做单张**，批量一律回 browse。
  这不是偷懒，是现有结构的必然结论。
* **软打样不做批量**：它是预览级、设备级，全局一份。
* **导出 profile 天然是批量**：属于预设，不需要额外机制。

**tile 角标：建议不加。** `AGENTS.md` §2.12 明令「RAW 角标已经有一个，不许在文件名条上再造一个」。
真要加也只加**异常**一种（`?` 角标 = profile 可疑），且要单独确认角位预算
（右下已有 `RAW`/`+RAW`、左下预留给 issue 数）。

### 6.4 全局设置

把现在的 `Ctrl+,`（快捷键设置）扩成**页签式设置对话框**：`快捷键` · `色彩管理` ·（将来 `常规`/`缓存`）。
色彩管理页签：显示器 profile（自动 / 手动指定 ICC）、「显示器已由系统加载校准曲线」开关、
默认工作空间、默认渲染意图、软打样默认目标、内置空间列表。
**不要**再开第二个设置入口。

---

## 7. 波次划分与验收判据

> 判据分两栏：**Agent 可验（冒烟/单测）** 与 **人类真机**（`AGENTS.md` §2.8 的分工）。

### CM-W1（L1）内嵌 ICC + 导出嵌 profile

* 范围：`img-parts` 接入（JPEG/PNG/WebP；**TIFF 自己走 IFD tag 34675**）；`lcms2` 接入；
  位图输入变换接进 develop 管线；导出可选 profile + 嵌 profile。
* 可验：单测「Adobe RGB 的三原色 → 数值对了」（期望值**外部给定**，不许被测函数自己生成）；
  round-trip 测试；AVIF/JPEG 导出的 ICC 块能读回。
* 真机：一张 Adobe RGB / ProPhoto 的图，与 PS/系统看图对比不再偏色。

### CM-W2（L2）显示器 profile

* 范围：Windows profile 获取（含 ACM 探测与「无 profile = sRGB」）、变更监听（注册表）、
  shaper-matrix 与 LUT 两条烘制路径、WGSL 应用（+四面体插值）、
  **surface 格式改 A 方案**、覆盖层同变换、baked LUT 的缓存与失效。
* 可验：LUT 烘制的纯函数单测（同输入同输出、矩阵型走精确路径）；
  离屏像素回读（期望值外部给定）；`WGPU_BACKEND` 之类的诊断覆盖仍在。
* 真机：广色域/校色屏上「洞口内外的同一块 UI 颜色一致」（这条同时覆盖 §5.5）；
  两块不同 profile 的屏来回拖窗口应当变；系统换 profile 后不重启也应生效。

### CM-W3（L3）工作空间 + 相机输入 profile

* 范围：工作空间 → **线性 Rec.2020**（人类拍板后）；RAW 输入 profile 三级
  （元数据矩阵 → DCP → ICC）；**issue 哈希与缓存的一次性迁移**。
* 可验：迁移演练（`cargo run -p raybend --example migration-drill` 那条路）；
  哈希版本化后老 issue 仍能匹配；缓存版本位 bump 后能重建。
* 真机：同一条编辑栈在 sRGB 与 Rec.2020 工作空间下的**色阶连续性与肤色**对比。

### CM-W4（L4）软打样 + 色域警告

* 范围：组合 LUT（工作空间 → 打样 profile → 显示器 profile）；渲染意图；
  色域警告（走 `overlay.wgsl` 或专门的 pass）。
* 可验：LUT 链的纯函数单测；色域警告的判定边界。
* 真机：打样预览与导出成图的观感对照。

### CM-W5 界面（可与 W2/W3 并行）

* 范围：editor 页签 + browse 右栏 + toolsbar 批量 + 设置页签 + ICC 导入（复用 LUT 通道）。
* **前置硬规矩**：`AGENTS.md` §5.1 —— **先出 `.pen` 稿**（`design/editor.pen` 补帧、
  `design/browse.pen` 补右栏分组），人类定案后再写界面代码。
* 命令体系（`AGENTS.md` §2.15）：`设置色彩配置` 这类要登记进命令注册表并**当场决定默认键或写明留空理由**。

---

## 8. 风险与坑（按危害排序）

| # | 坑 | 表现 | 处置 |
| --- | --- | --- | --- |
| 1 | **vcgt 双重应用** | 画面过暗/过亮，且「换了校色软件才正常」 | 显示器 profile 常内嵌 `vcgt`（或新的 `MHC2`）校准曲线，**Windows 的 inbox loader 已经把它写进 GPU LUT**。默认**只取 profile 的色彩数据、忽略 vcgt**，并给一个「显示器已由系统加载校准」开关。微软文档明确 `SetDeviceGammaRamp` 在 HDR 下行为未定义 |
| 2 | **ACM 激活时 ICC 查询返回「无 profile」** | 误判成「没校色」→ 我们自己做 sRGB 变换 → 与系统双重管理 | 先探测 ACM 状态再决定策略；「无 profile」= sRGB（微软明文约定） |
| 3 | **输入 profile 进 issue 哈希会作废现有定稿** | 升级后所有命名定稿都「不匹配 latest」 | 哈希里带**版本号**；新增字段对「默认值」做特殊处理（`as_shot_k` 已有同类先例）；配一次性迁移 |
| 4 | **工作空间变更作废缓存** | 全部重渲染 | 缓存名已有 `-v<pipeline>` 版本位 → bump 即可；**越早定工作空间越便宜** |
| 5 | profile 无 D50 白点 | 洞口颜色与 DOM UI 不一致 | 与 Chromium 采用同一规则（拒绝 + 按 sRGB 处理），并**真机核对** |
| 6 | WCS API 句柄泄漏 | 长时间运行后资源耗尽 | 只在启动/显示器变更时读一次并缓存；禁轮询 |
| 7 | 8-bit 显示编码在广色域屏上的色阶 | 渐变出现色带 | L3 之后评估 10-bit / fp16 surface（顺带进 L5） |
| 8 | TIFF 的 ICC（tag 34675）不在 `img-parts` 范围 | TIFF 输入漏读 | 自己走 IFD 或另找 crate（小工作量，别忘） |

---

## 9. 待人类拍板（开工前必须定，越早越便宜）

| # | 问题 | 建议 |
| --- | --- | --- |
| 1 | **工作空间选哪个**：线性 **Rec.2020** / 线性 ProPhoto / ACEScg | **线性 Rec.2020**（darktable 的选择、与 BT.2100 同源）。它决定 W3 的一次性缓存与哈希作废 |
| 2 | **输入 profile 是否进 issue 哈希** | **进**（改它会改画面，本来就是编辑的一步） |
| 3 | **软打样第一版做不做** | **放 W4，不挡前三级** |
| 4 | **ACM 策略**：只做「SDR + 自己做显示变换」，还是同时做 `ExtendedSrgbLinear` 交接给系统 | **先只做前者**（后者要 fp16 管线，成本高） |
| 5 | **批量入口放 `browse toolsbar`** | 同意即可（§6.3） |
| 6 | **ICC 导入复用 LUT 导入那条路**（抽成「文件型资源导入」一份实现） | 同意即可（§2.12 的直接适用） |
| 7 | **要不要 tile 上的「profile 可疑」角标** | **不加**（角位预算已被 `RAW`/issue 数占住） |

---

## 10. 与其它文档的关系

| 文档 | 记什么 |
| --- | --- |
| **本文件** | 方向、边界、分层、验收判据、坑、待拍板项 |
| `specs/CM-W<n>.md` | **各波次开工前**写的实施步骤（一次一个，走 plannotator） |
| `AGENTS.md` §6.1 | 渲染架构与本方案的三条硬纪律（落地时补） |
| `FUTURE.md` C1/C2 | 只剩「登记」职能（L5/L6 与 macOS/Linux）；正文指向本文件 |
| `IMAGING.md` | 三种图的规格；本方案给「缓存绝不做显示变换」这条加约束 |
| `DESIGN.md` §14.9 | editor 右栏三组页签 → 第三组增加 `色彩管理` 页签 |
| `BROWSE.md` §6 | 右栏信息区新增 `色彩` 分组 |
| `REVIEW.md` R1-01 | 本条规格的来源 |
