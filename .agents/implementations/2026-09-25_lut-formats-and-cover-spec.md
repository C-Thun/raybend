# LUT 格式调研 + 兼容范围与封面规格落档

完成时间：2026-09-25 20:59:52 CST

本次**只改规划与设计文档、不动代码**。起因：人类问“LUT 格式有哪些、哪些自带演示图”，
目的是定**兼容范围**与**界面演示图怎么来**。结论已落进 `PLAN.md`（M3-W6c）与
`design/editor.md`（§3.1.1 新增）。

---

## 一、LUT 格式全景（调研结果）

### 1.1 事实标准（跳软件通用）

| 扩展名 | 出处 | 形态 |
| --- | --- | --- |
| **`.cube`** | Iridas 2003，后被 Adobe 接手并发布规范 | 1D / 3D / shaper，文本。**行业事实标准** |
| `.3dl` | Autodesk（Lustre / Flame / Discreet） | shaper + 3D，整数 0..4095 |
| `.look` | Iridas | 1D / 3D |
| `.csp` | Rising Sun Research CineSpace | spline shaper + 1D / 3D |
| `.cub` | FilmLight Truelight | shaper + 3D |
| `.itx` | Iridas | |
| `.mga` / `.m3d` | Pandora | 3D |
| `.spi1d` / `.spi3d` / `.spimtx` | Sony Pictures Imageworks | 1D / 3D / 矩阵 |
| `.dat` | DaVinci | FFmpeg 支持 |
| `.blut` | Houdini | **二进制** 3D |
| `.cms` | Light Illusion LightSpace | 1D + 3D |

### 1.2 唯一的开放标准

* **`.clf`**（Academy / ASC **Common LUT Format**）：XML，支持任意算子链
  （矩阵 / 1D LUT / 3D LUT / ASC CDL / log 与 exp shaper）。文档号 S-2014-006，
  **2025-09 正在走 SMPTE 标准化**，将成为正式标准。
  规范：https://docs.acescentral.com/clf/specification/
* `.ctf`：ACES 的 Color Transformation Format（CLF 前身 / 相关）。

### 1.3 用图像编码的 LUT

* **HaldCLUT**（`.png` / `.tiff` / `.hdl`）：identity 立方体编码进一张方图。
* **ICC profile**（`.icc` / `.icm`）：A2B / B2A 标签里可以装 LUT。

### 1.4 厂商私有

Panasonic **VLUT**（VariCam）、BMD、各家机内 LUT 等。摄影用户手里基本不会出现。

---

## 二、两处关键核实

### 2.1 `.cube` 规范的全部合法头字段 —— **没有演示图字段**

读了 Adobe 规范原文（https://kono.phpage.fr/images/a/a1/Adobe-cube-lut-specification-1.0.pdf ）。
合法头字段**只有五个**：

```text
TITLE "..."
DOMAIN_MIN r g b
DOMAIN_MAX r g b
LUT_1D_SIZE n
LUT_3D_SIZE n
```

**没有任何 preview / thumbnail / reference image 字段**（注释 `#` 不算字段）。
另有两条对实现有用的约定：

* 3D 表的行序：**Red 变化最快**（等价 C 索引 `r + N*g + N*N*b`）—— 最容易踩反的坑；
* 关键字必须出现在表数据之前，每个最多出现一次。

### 2.2 HaldCLUT 的精确规格 —— **纠正了常见误解**

常见说法是“HaldCLUT 图边长 = level³、表边长 = level²”，但那只是**半个式子**。
原文在 `rtengine/clutstore.cc`：

```cpp
// loadFile()：找 level 使 level³ == 图边长
while (level * level * level < fw) ++level;
if (level * level * level == fw && level > 1) { clut_level = level; }
// load() 里：
clut_level *= clut_level;                 // → 表的边长 = level²
flevel_minus_one = (clut_level - 1) / 65535.0f;
```

完整的对应关系是：**图边长 = level³、表边长 = level²、表项数 = 图像素数**。

| 图尺寸 | level | 表边长 | 表项数 |
| --- | --- | --- | --- |
| **512×512**（FreeHaldClut 常见档） | 8 | 64 | 262 144 |
| 1728×1728 | 12 | 144 | 2 985 984 |
| 4096×4096 | 16 | 256 | 16 777 216 |

对比：**`.cube` 的 33³ 只有 35 937 项** —— 也就是说 **HaldCLUT 在精度上反而是高的一方**。
查表实现是三线性插值＋`strength` 与原值混合（`HaldCLUT::getRGB`）。

### 2.3 “自带演示图”：**不存在这种格式**

`.cube` 规范里没有该字段（2.1）。行业做法一律是**自己生成**：IWLTBAP 的 LUT Previewer 让
用户指定一个图片文件夹，Eagle 的 LUT 插件也是 “Custom Image Preview”。

两个例外，但**都不属于 LUT 格式**：

* **ICC profile** 规范里确实有 `preview0/1/2` 标签（编号 112 / 113 / 114）可内嵌预览图；
* **XMP** 有 `xmpGImg:image`（base64 JPEG）与 `photoshop:Thumbnail` —— Lightroom 的预设缩略图
  靠这个，但 XMP 是“预设”不是 LUT。

实际中最接近“自带演示图”的形态是**伴生文件**：`look.cube` + 同目录同名的 `look.jpg` / `look.png`，
不是内嵌。

→ **结论：兼容范围不受“演示图”约束**，这一层不需要为任何格式做适配；演示图只能自己生成。

---

## 三、人类定的规格（2026-09-25）

### 3.1 兼容范围：**只两种**

1. **`.cube`**（1D / 3D / shaper）；
2. **HaldCLUT**（PNG / TIFF）。

其余（`.3dl` / `.look` / `.csp` / `.cub` / `.itx` / `.mga` / `.spi*` / `.blut` / `.clf`）**不做**；
`.icc` / `.icm` 里面的 LUT 归色彩管理（`FUTURE.md` C1），**不放 LUT 面板**。

### 3.2 导入方式：**按目录**，递归 3 层

选一个目录，递归扫描**该目录及其下最多 2 层子目录**（共 3 层）。
依据是人类描述的常见结构：“总目录 → 若干放 cube 的子目录 → 子目录下有 cube + 可能的演示图”，
3 层足够覆盖。

### 3.3 LUT 封面（效果图）：导入时烘焙

**导入时一次性烘焙并存储，不是运行时生成。** 两种来源：

1. **LUT 自带的演示图** —— 与 `.cube` **主文件名同名**的图片（同目录），
   按 `jpg` → `jpeg` → `png` → `tif` → `tiff` → `avif` → `webp` 取第一张。
   这类图是 LUT 作者已套好效果的展示图，**只做几何处理、不再套 LUT**。
2. **回落：内置固定样片**（`lut-sample`）—— 用固定图**套上该 LUT** 再烘焙。

**几何：两档，按原图尺寸选（人类 2026-09-25 定）**

| 原图条件 | 目标尺寸 |
| --- | --- |
| `w ≥ 768` 且 `h ≥ 576` | **768×576** |
| 否则（小图 / 竖图 / 超宽图） | **384×288** |

算法一律 **cover**：`scale = max(target_w / w, target_h / h)` → 按 scale 缩放 → **居中裁切**。
编码 **WebP 质量 80**。

#### 实现口径的两处修正（Agent 提请，人类口径已覆盖）

* **“先缩到宽 768 再截中间 768×576”对非 4:3 图不成立**：1920×1080 缩到宽 768 只有 432 高，
  裁不出 576。**cover 是等价的正确形式** —— 对 4:3 原图与人类描述的结果完全一致，
  对其它比例也能给出正确结果。
* **判据等价性**：`w ≥ 768 && h ≥ 576` 与“cover 缩放比 ≤ 1（不需要放大）”**完全等价**，
  所以“两档”这条规则没有歧义边界。

### 3.4 存储落点（Agent 定，理由随附）

* **LUT 本体复制进** `%LOCALAPPDATA%\raybend\luts\` —— 自包含，用户移动 / 删除原目录后仍可用；
  与 `AGENTS.md` §6.4 的“全局资源放 `%LOCALAPPDATA%`”一致（LUT 是用户级资源，跨库共享）。
* **元数据**（名称 / 分类 / 原路径 / 封面引用）进 `app.db`，走既有迁移框架（§2.16）；
* **封面** `*.webp` 与本体同目录。

---

## 四、本次改动的文件

| 文件 | 改了什么 |
| --- | --- |
| `PLAN.md` | M3-W6c 的 `.cube` 一句话扩成完整规格：兼容范围（含 `.cube` 要处理对的四件事与明确不做的清单）、按目录递归 3 层的导入、封面口径、存储落点 |
| `design/editor.md` | ① 开头控件对照表的「LUT 预览图」行改为「LUT 封面图（导入时烘焙）」；② §3.1 的 tile 描述改写；③ **新增 §3.1.1 LUT 封面**；④ §3.5 的「Modal / 导入 LUT」改为选目录 + 扫描预览；⑤ i18n `editor.lut.importHint` 文案改为选目录 |

### `.cube` 要处理对的四件事（写进了 PLAN）

1. **Red 变化最快的行序**（2.1）；
2. `DOMAIN_MIN` / `DOMAIN_MAX` —— 输入域不一定是 [0, 1]；
3. **1D shaper + 3D 的组合**（1D 先应用，不是所有实现都处理）；
4. **尺寸不定**：17³ / 33³ / 65³ 都要能读（33³ 是创意 LUT 的常见档）。

---

## 五、验证

* 改的是**文档**，没有跑构建与测试。
* 结构核对：`PLAN.md` 的 M3-W6c 已含兼容范围与封面口径；`design/editor.md` 已含 §3.1.1。
* markdown lint：本次引入了 1 处 `MD036`（`**几何：两档，按原图尺寸选**` 作为独立强调段），
  **已就地修掉**（改为 `**几何**：按原图尺寸分两档，判据如下。`）。

---

## 六、遗留与待人类

1. **固定样片 `lut-sample` 尚未提供** —— 规格 **768×576（4:3）**，人类提供后需要落到
   `src/assets/`（或等价的资源目录）并登记进构建。它同时兼任“没有自带演示图的 LUT”的
   封面底图，所以 **W6c 开工前必须有**。
2. **没有自带演示图的 LUT** 要靠“固定样片套 LUT”生成封面 —— 所以 **LUT 求值引擎必须先能跑**，
   不能反过来。这决定了 W6c 内部的实现顺序（引擎 → 导入 → 封面）。
3. `.clf`（2.1 的开放标准）本次**定性为“第二版再说”**，理由与触发条件尚未写进
   `FUTURE.md` D8 —— 如果人类希望明确登记，下次一并补。
4. 封面生成需要**图片解码 + 缩放 + WebP 编码**的能力；`thumbnail/render.rs` 已有 `encode_avif`，
   WebP 是新增一种编码器（或确认 `image` crate 的 webp 支持是否够用），属于 W6c 的实现细节，
   开工写 `plans/M3-W6c.md` 时定。
