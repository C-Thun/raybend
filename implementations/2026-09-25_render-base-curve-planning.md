# 渲染基础曲线（base curve）：调研、方案与 W6 重排

完成时间：2026-09-25 19:20:53 CST

**本次只改规划文档、不动代码** —— 把「让 RAW 出图有厂商那种光比」这件事的调研结论、
方案取舍与排期落到 `FUTURE.md` 与 `PLAN.md`。

起因（人类 2026-09-25 口述）：「raw 编辑功能是有，但实际上效果不太好……各家的照片的输出关键是
有一套曲线……要么画面中间调太暗（因为 raw 本来就暗），中间调调亮了反差又低了，这还不是动态反差
能解决的事，要靠曲线；但我发现这样调出来曲线其实跟我们以前调 jpg 时用的 s 形还不一样，
往往是一个抛物线型，即暗调就要往上去。」

---

## 一、本仓现状的源码级诊断

`crates/raybend/src/develop/pipeline.rs` 的实际顺序：

```text
线性 RAW × gain(2^exposure)
  → contrast_curve      ← 线性域，绕 0.18 的分段幂  k = 2^amount ∈ [0.5, 2]
  → highlights_curve    ← 线性域  y + h·y⁴(1−y)
  → blacks_curve        ← 线性域  y + b·(1−y)⁴y
  → linear_to_srgb()    ← 只有 sRGB OETF
  → 用户曲线（RGB 合成 → R/G/B 分通道）
  → 饱和度 / 自然饱和度
```

五条根因（都有代码位置，不是推测）：

1. **缺 base curve** —— 线性到显示之间只有 sRGB OETF，等于「零渲染」。厂商固件与 Adobe
   都在这个位置插一条曲线。
2. `contrast` 在**线性域**（`contrast_curve`，绕 0.18 的分段幂）。物理上正确，
   但暗部被 sRGB 的线性段放大，观感与「反差拉杆」的直觉差得很远。
3. `blacks_curve` 的权重函数 `(1−y)⁴y` 的**峰值在 y = 0.2**（令 `(1-y)³(1-5y)=0` 解出）。
   线性 0.2 ≈ 显示域 0.485 —— **这是一个中间调控制，不是黑场控制**；
   真正的暗部（线性 0.01）权重只有 0.0085。所以「抬黑场」这件事现在无处可放。
4. **没有 baseline exposure** —— Adobe 用 DCP 的 `BaselineExposureOffset` 补偿
   「厂商标准曝光 vs RAW 线性中灰」的差（各家能差 ±1 EV）。**这才是「RAW 本来就暗」的
   技术解释**，不是错觉。
5. **没有 midtone 参数** —— 于是「提中间调」只能拉曝光（线性整体乘），
   而线性域加一个固定量在显示域是**暗部放大 ~2.5×、中间调 ~1.2×、高光 ~0.5×**
   （实算：线性 0.01→0.02 显示域走 0.100→0.152；0.18→0.20 走 0.461→0.485）。
   暗部被抬得最多 ⇒ 观感就是「反差低了」。

---

## 二、RawTherapee 一手调研（`/home/andares/repos/refers/RawTherapee`）

### 2.1 `rtengine/histmatching.cc`（395 行，Alberto Griggio，2018）= Auto-Matched Tone Curve

六步：① 取**内嵌 JPEG 缩略图**（厂商渲染结果）；② RAW 做**中性渲染**（FAST 去马赛克、
无输出 profile、无曲线）；③ 两者缩到同尺寸（`skip` 调在 6~10）；④ 算**亮度直方图 CDF**
（`getCdf` 用 `Color::rgbLuminance`）；⑤ `findMatch` 做直方图匹配 → 256 项 mapping；
⑥ `mappingToCurve` 简化成 8~12 个控制点的样条。

关键细节：

* **只匹配亮度** —— 拿到的是 RGB 合成曲线，不含色彩；
* `mappingToCurve` 会**主动去掉上半段的凹性**，源码注释：
  *"we assume we are matching an S-shaped curve, so try to avoid concavities in the upper part of the S"*；
* 内嵌缩略图太小时跳过，判据 `w * 33 < fw || w * h < 19200`；
* 结果有缓存（`histMatchingCache`，按 `ColorManagementParams` 判同）。

### 2.2 `rtdata/profiles/Standard Film Curve - ISO {Low,Medium,High}.pp3`

曲线控制点是**明文**：

```text
CurveMode=FilmLike
Curve=1;0;0;0.11;0.09;0.32;0.43;0.66;0.87;1;1;
```

即 `(0,0) (0.11,0.09) (0.32,0.43) (0.66,0.87) (1,1)` —— **所有中间点都在对角线上方**，
这条「整体提亮、暗部也往上抬」的形状正对应人类说的「抛物线型、暗调就要往上去」。

### 2.3 `ToneCurveMode::FILMLIKE` = Adobe 的参考实现

`rtengine/procparams.h` 的枚举注释：*"Film-like mode, as defined in Adobe's reference code"*。
应用方式在 `rtengine/curves.h::AdobeToneCurve::RGBTone`（**保色相**）：

```text
maxval = lut[maxval_old]
minval = lut[minval_old]
medval = minval + (maxval − minval) × (medval_old − minval_old) / (maxval_old − minval_old)
```

**结论：官方曲线的「形状」与「施加方式」都跟手拖一条 S 形不是一回事。**
人类说的「跟我们以前调 jpg 时用的 s 形还不一样」，在源码层面确认了。

### 2.4 `rtengine/clutstore.cc` = HaldCLUT（Film Simulation）

`HaldCLUT::getRGB` 是三线性插值 + `strength` 与原值混合；表来自一张 identity 立方体
编码成的二维图。这与 M3-W6c 计划的 `.cube` 引擎是**同一条查表路径**，所以登记进 FUTURE D8。

### 2.5 ❗反直觉发现：RawTherapee 的 ISO 三档曲线**完全相同**

核对 `Standard Film Curve - ISO Low / Medium / High.pp3`：`Curve=` 那一行**一字不差**，
三档的差异全在降噪参数（High 档多了 `[Directional Pyramid Denoising]` 且 `Median=true`）。

→ 所以「ISO 分档」在**曲线**这件事上的作用比想象的小得多。
排期里据此定：**先不做 ISO 分档**，per-photo 拟合里照常记 ISO，等统计显示高 ISO 确有
系统性偏移再说（§5.4 不提前细化）。

### 2.6 许可核查（本次新查实）

| 对象 | 许可 | 能否用于本项目 |
| --- | --- | --- |
| RawTherapee 代码 | GPL-3.0 | ✅ 可移植进 AGPL-3.0（§13），需署名 |
| darktable 代码 | GPL-3.0+ | ✅ 同上 |
| FreeHaldClut 预设（Pat David / pIXELsHAM） | **CC-BY-SA 4.0** | ⚠️ 内容许可、与软件 AGPL 分开管；署名 + 相同方式共享 |
| Adobe DCP 文件 | Adobe Color Profile License / DNG SDK | ❌ **不可再分发** |

DCP 那条特别记一笔：第三方项目 camicc 的 README 也明确写了「从本地 DNG Converter 提取、
不要 commit、不要再分发」。**RawTherapee 打包了 130 个 DCP 是它自担风险，我们不跟。**

---

## 三、机内风格污染的调研（人类 2026-09-25 特别要求考虑）

问题：内嵌 JPEG = 厂商渲染 × **机内风格档** × **机内微调**。
直接拟合会把「风格档」当成「厂商标准」—— 有人用自然（反差小）、有人用 vivid（反差大高饱和）、
还有人在机内设了亮调暗调甚至人工曲线。RawTherapee 那个实现**完全没处理这件事**。

### 3.1 两台目标机器的字段都能读（本次核实）

**松下 RW2** —— `PhotoStyle` 是 tag `0x0089`（int16u），值域：

```text
0=Auto  1=Standard or Custom  2=Vivid  3=Natural  4=Monochrome  5=Scenery  6=Portrait
8=Cinelike D  9=Cinelike V  11=L.Monochrome  12=Like709  15=L.Monochrome D
17=V-Log  18=Cinelike D2
```

> **❗ 更正（2026-09-25 实测，同一日）**：本节初稿写的是「在 **RW2 的 IFD0** 里、解析成本极低」
> —— **这是错的**，它把 exiftool 的两套 tag 表搞混了。实测 DC-G9 的 142 张 RW2（样本见下）：
>
> * **RW2 的 IFD0 里没有 `0x0089`**。RW2 的 IFD0 走的是另一套 “PanasonicRaw” tag 表
>   （`0x0001`–`0x0038`：宽高 / 黑电平 / 内嵌 JPEG 偏移 / 白平衡系数），只有 62 个 entry，无风格字段；
> * **真正的位置是内嵌 JPEG**（IFD0 的 `0x002e`，435 KB）**的 EXIF → MakerNote**：
>   签名 `Panasonic\0\0\0`（12 字节）+ `u16` entry 数（实测 169）+ 标准 IFD entries，
>   值偏移相对 MakerNote 起点。
> * 实测分布：**Natural 138 张 / Standard 4 张** —— 人类自己的习惯是 Natural。
>
> 验证脚本与完整数据见 `implementations/2026-09-25_lut-sample-and-photostyle-verification.md`。

**奥巴 ORF** —— MakerNote `0x010c RawDevPictureMode`（1=Vivid / 2=Natural / 3=Muted /
256=Monotone / 512=Sepia）；更细的一组在 **OlympusCs 子 IFD**：
`0x0520 PictureMode` + `0x0521 PictureModeSaturation` / `0x0523 PictureModeContrast` /
`0x0524 PictureModeSharpness`（各为 `int16s[3]` = value/min/max）；另有 `0x1029 Contrast`
（0=High 1=Normal 2=Low）。
（来源：exiv2 的 Olympus tag 表 https://exiv2.org/tags-olympus.html ）

### 3.2 现有依赖都**不**提供这些字段

已核实 `rawler 0.8.0` 的 `src/exif.rs::Exif` 结构体：只有标准 EXIF 字段
（orientation / lens_spec / exposure_time / iso_speed… 以及 `white_balance`、
`scene_capture_type`），**没有** `photo_style` / `picture_mode`。
`kamadak-exif` 也不解析厂商 MakerNote（它读的是标准 EXIF）。

→ 要自己解析 IFD。**但这不是引入新依赖的理由**（只有 2~3 个 tag）；
真要覆盖更多品牌再评估 `fpexif`，且届时按 §2.9 先登记 FUTURE 讨论。

### 3.3 三条对策（写进 PLAN.md M3-W6b）

* **对策一（先做，通用）**：机型内**逐维中位数**（不用均值）+ **离群检测** ——
  每张的参数向量与机型中位数的距离超容差即标记「特殊风格」，不参与统计、不自动套用。
  **不依赖任何厂商字段，对任何品牌都有效。**
* **对策二（增强）**：读 3.1 的字段 → **给簇命名** + **只从标准档取样**
  （松下的 `Standard or Custom`、奥巴的 `Natural`）。
* **对策三（反直觉但重要）**：**机型档案该学「这个用户在这个机型上的习惯」，不是「厂商的标准」**
  —— 若用户 90% 的照片都用某个风格，中位数给出的就是那个风格，**这是对的**。
  所以「污染」的真问题不是「风格不同」，而是「**同一机型混用多个风格**」与「**黑白照片**」
  这两类会让中位数拉偏 —— 都由对策一的离群检测覆盖。

---

## 四、本次改动的文件

| 文件 | 改了什么 |
| --- | --- |
| `FUTURE.md` | 新增 **D8「3D LUT 与胶片模拟」**（+62 行中的主体）、**D9「DCP 相机配置文件」**；均为「第一版不做」，含许可红线、参考实现、触发条件 |
| `PLAN.md` | M3-W6 由单波重排为 **W6a / W6b / W6c** 三段，并明确 **W6a 先做** |

### M3-W6 的新排法

* **M3-W6a　渲染基础曲线与机型档案（先做）**
  base curve 进管线（显示域、用户曲线之前、共用现有求值器、烘进 `ChannelLuts` 零成本）；
  内置通用 base curve（toe + shoulder + 中灰锚点，参考 darktable 的 sigmoid / filmic / AgX）；
  `app.db` 机型档案表；编辑器曲线页签加**只读浅辅色叠加线** + 曲线下的**只读机型名小组件**；
  SOOC 并排参照 + 命令注册表接入。
* **M3-W6b　SOOC 反推机型曲线 + 机型库积累**
  照 `histmatching.cc` 六步；产物是**低自由度参数向量**（不是 256 点曲线）；
  在**导入/缩略图生成时算一次落 catalog**（不放在打开编辑器时）；风格污染三对策；
  四层回退链（本图拟合 → 机型档案 → 品牌默认 → 内置通用，**最后一档必须永远可用**）。
* **M3-W6c　CUBE LUT + issue + 数据结构重整**（原 W6 内容，不变）

拆成三段是按 §5.4「一个波次 = 一个工作单元」，每段单独写 `plans/M3-W6x.md` 并走评审。

---

## 五、关键决策与理由

1. **顺序必须是「先光比、后色彩」** —— LUT 是乘在光比之上的；base curve 没做对之前上 LUT，
   只会把错的光比再染一层色。这是 D8/D9 标「第一版不做」的唯一理由（人类当场拍板）。
2. **base curve 与用户曲线分成两个概念**（选 Lightroom / darktable 式，**不选** RawTherapee 式）
   —— 机型库的全部价值在「跨照片一致性」；一旦 base curve 落进编辑栈（per-photo），
   这个价值就没了（「重置」回到哪、机型库升级老照片跟不跟，都会变成无解的问题）。
   人类 2026-09-25 追加：**不给用户选择权** —— 只在曲线下加一个只读的机型名小组件，
   画布上加一条**浅辅色、只读**的叠加线（与现有直方图底纹同层）。
3. **base curve 放显示域** —— ① SOOC 反推出来的控制点可以直接画给用户看、与厂商曲线同形；
   ② 复用现有 `develop/curve.rs` + `lib/curve.ts` 求值器，**不写第三份**（§2.12）；
   ③ darktable 的 basecurve 本来就是 display-referred。
   代价：toe 只能从显示域 0 往上抬（厂商在线性域抬），观感差异可忽略。
4. **拟合产物存「低自由度参数向量」而非 256 点曲线** —— 抗噪、可统计、可判离群、存储小。
   这是把「污染」从不可知变成**可检测**的关键设计。
5. **拟合放在导入/缩略图生成时算一次，不放在打开编辑器时** —— 人类问「实时拟合贵不贵」：
   不贵（缩略级渲染 + 256 桶直方图，微秒级），但没必要每次算。算一次落 catalog，
   编辑器只读 —— **效果与「每次实时算」相同，但快得多**。
6. **不做「搬运网上的机型数据」** —— 可搬的只有 darktable `basecurve.c::_basecurves[]`
   （社区近似、不是厂商数据；且 darktable 自己已把它降级为 legacy，现默认 `sigmoid` / `AgX`），
   RawTherapee 的 DCP 则受许可限制不能分发（D9）。
   **用户自己的照片库才是最好的数据源** —— 几千张学出来的比网上抄的准得多。
   （回退链里保留「品牌级默认」这一档，但改为「我们内置几条通用曲线」。）
7. **ISO 分档先不做** —— 依据是 2.5 的核实结果。

---

## 六、验证

* 本次改的是**文档**，没有跑构建与测试（不涉及代码）。
* 已核对结构：
  * `grep '^### M3-W6' PLAN.md` → `M3-W6a` / `M3-W6b` / `M3-W6c` 三段都在，`M3-W7` 未受影响；
  * `grep '^### D[0-9]' FUTURE.md` → D1~D9 齐全，D8 / D9 落在 D7 之后、`## E` 之前。
* 编辑器的 markdown 校验通过（两次 edit 都返回 `Markdown clean`）。

---

## 七、遗留与待人类

1. **松下的 tag 已实测核实并纠正了位置**（见 §3.1 的更正框）—— 它在**内嵌 JPEG 的 MakerNote**，
   不在 RW2 的 IFD0。**奥巴的 `0x010c RawDevPictureMode` 仍未验证** ——
   本机样本目录（`/mnt/c/src/tmp/pic`）只有 142 个 RW2 + 158 个 JPG，**没有 ORF**。
   开工 W6b 前需要人类提供 E-M5 Mark II 的 ORF（或指向一个本机目录）。
2. `THIRD-PARTY-NOTICES.md` 的两条（FreeHaldClut 的 CC-BY-SA 4.0、DCP 不可再分发）
   **等真正接入时再登记**；本次只在 `FUTURE.md` 里写了红线。
3. M3-W6a 开工前需要按 §5.4 另写 `plans/M3-W6a.md` 并过 plannotator 评审 ——
   本次只做到路线级排期，没有细化到实现方案。
4. 「基础曲线」的具体曲线族参数化（几个参数、各自的取值范围、与 `BaselineExposure`
   怎么分工）还没有定案，属于 W6a 的详细规划内容。
