# 内置样片集成 + 松下 PhotoStyle 实测验证

完成时间：2026-09-25 21:11:06 CST

本次做了两件事，都是为 M3-W6a/W6b/W6c 铺路：

1. **内置样片 `lut-sample` 入仓**（人类 2026-09-25 提供）—— 含一次**元数据剥离**；
2. **松下 `PhotoStyle` 实测核实** —— 顺手把 `PLAN.md` M3-W6b 里一条**我自己写错的事实**纠正了。

---

## 一、内置样片入仓

### 1.1 人类提供的文件

`/home/andares/repos/c-thun/lut-sample.webp`，**768×576（4:3）**，与
`design/editor.md` §3.1.1 定的「大封面」档完全一致。

### 1.2 ❗ 发现的问题：原件带 42 KB 的拍摄元数据

文件是 RIFF/WebP 容器，解析 chunk 后：

| chunk | 大小 | 说明 |
| --- | --- | --- |
| `VP8X` | 10 | 扩展容器，flags `0x08`（= 含 EXIF） |
| `VP8 ` | 85 486 | **有损** VP8 图像数据 |
| `EXIF` | **41 974** | **占全文 32%** |

那个 EXIF 里读出来的是：

```text
2024:07:09 14:39:16
OLYMPUS IMAGING CORP. / E-M5MarkII / Version 2.3
LEICA DG 12-60/F2.8-4.0
```

这是人类自己的实拍片（机身 + 镜头 + 拍摄时间）。两个理由要去掉它：

* **拍摄隐私不该进开源应用的安装包**；
* 白占 32% 体积。

### 1.3 处理：纯字节级 chunk 剥离（不重编码）

用 RIFF 容器结构直接删 chunk，**解码→编码一次都不做**，所以**零质量损失**：

* 从 `VP8X` 的 flags 里清掉 `0x08`（EXIF）与 `0x04`（XMP）两个标志位；
* 删掉 `EXIF` chunk；
* 重写 `RIFF` 的长度字段（并保持每个 chunk 的偶数字节对齐）。

```text
127,506 字节  →  85,524 字节   （省 32%）
```

校验：`RIFF` size 字段 = 文件长度 − 8 ✓；chunk 序列变成 `VP8X` + `VP8 ` ✓；
画布仍是 768×576、比例 1.3333 ✓。

### 1.4 落点与代码接入

* 文件 → `crates/raybend/assets/lut-sample.webp`（**Rust 侧**，不是前端 `src/assets/`）。
  理由：封面烘焙在 Rust 侧做（套 LUT + 缩放 + 编码），前端拿它没用；
  且 `include_bytes!` 比走 Tauri 资源 API 简单，不把 `crates/raybend` 与 Tauri 绑在一起
  （`AGENTS.md` §6.2 的分层纪律）。**这是本仓第一处 `include_bytes!`。**
* 新模块 → `crates/raybend/src/develop/lut.rs`，**目前是落点占位**：
  只有 `SAMPLE_WEBP` / `SAMPLE_WIDTH` / `SAMPLE_HEIGHT` 三个常量和两个测试；
  解析 / 求值 / 封面烘焙随 `plans/M3-W6c.md` 落地。
* `crates/raybend/src/develop/mod.rs`：登记 `pub mod lut;` 并更新模块目录树注释。
* **`Cargo.toml`：给 `image` 加 `webp` feature** —— 原来没开，连这个样片都解不了
  （用户自带的演示图按规格也会含 webp）。纯 Rust（`image-webp`），无系统库。

### 1.5 两个测试（其中一个守着隐私回归）

| 测试 | 断言 |
| --- | --- |
| `sample_is_a_decodable_webp_of_the_declared_size` | 能解码、是 768×576、且确实是 4:3（`W×3 == H×4`） |
| `sample_carries_no_metadata_chunks` | chunk 序列含 `VP8 `、**不含 `EXIF` / `XMP `** |

第二个测试**刻意不用「在字节流里搜 `EXIF` 四个字母」** —— 图像数据里碰巧出现这四个字节是
低概率但非零的事件，那就是个会偶发变红的假测试。它遍历 RIFF chunk 列表来判定。

---

## 二、松下 PhotoStyle 实测验证（并纠正一处文档错误）

### 2.1 起因

翻测试列表时看到 `media::tiff::tests::parses_a_real_rw2_when_the_sample_is_present`，
注释写着「样本在这台机器上」—— 于是**本机其实有真实 RAW 样本**，
足以把 2026-09-25 那份实施记录里「tag 尚未在真实样本上验证」这条遗留直接消掉。

样本目录：`/mnt/c/src/tmp/pic`（**142 个 RW2 + 158 个 JPG**，无 ORF）。
样本机型实测为 **`Panasonic` / `DC-G9` / 固件 Ver.2.7** —— 正是人类的机器。

### 2.2 ❗ 纠正：`0x0089` **不在 RW2 的 IFD0 里**

初稿（`implementations/2026-09-25_render-base-curve-planning.md` §3.1）写的是
「`PhotoStyle` 是 **IFD0** 里的 tag `0x0089`、解析成本极低」——**这是错的**，
根源是把 exiftool 的**两套** tag 表搞混了：

* **PanasonicRaw**：RW2 / RWL 的 `IFD0` 用的一套（`0x0001`–`0x0038`）；
* **Panasonic MakerNote**：另一套，`PhotoStyle = 0x0089` 属于这里。

实测 DC-G9 的 RW2 结构：

```text
IFD0 @24，62 个 entry
  ├─ 0x0001..0x0038   PanasonicRaw 私有组（宽高 / 黑电平 / 白平衡系数 / 内嵌 JPEG 偏移）
  │                   —— **没有 0x0089**
  ├─ 0x002e  UNDEF 435355 字节  ← 内嵌 JPEG（完整！）
  ├─ 0x02bc  UNDEF 396 字节     ← XMP
  └─ 0x8769  LONG → EXIF IFD @3964
                       └─ 16 个标准 EXIF entry，**没有 0x927c MakerNote**
```

**RW2 的 EXIF IFD 里没有 MakerNote。** 真正的 MakerNote 在 **内嵌 JPEG**（`0x002e`）里：

```text
内嵌 JPEG @0x002e，435 355 字节，头 ffd8ffe1（APP1/EXIF）
  └─ APP1 长度 50 686 → TIFF 段
       ├─ IFD0 @8，13 个 entry
       └─ 0x8769 → EXIF IFD @680，42 个 entry
                     └─ 0x927c MakerNote：28 272 字节 @1314
                          「Panasonic\0\0\0」+ u16 entry 数 = 169 + 标准 IFD entries
                          值偏移相对 **MakerNote 起点**
                              └─ 0x0089 PhotoStyle ✓
```

### 2.3 实测值

单个文件（`P1000019.RW2`）：

```text
0x0089 PhotoStyle = 1  → Standard/Custom
0x0026 FilterEffect = b'0415'   （4 字节二进制）
0x0014/0x0015/0x0016/0x0017/0x0018/0x0019（Contrast* / Saturation / Sharpness）**都没有**
```

**批量 142 个 RW2 的分布（这才是关键数据）**：

| 照片格调 | 张数 |
| --- | --- |
| **Natural** | **138** |
| Standard/Custom | 4 |

→ **人类自己的习惯是 Natural**。这条直接影响了 W6b 的取样规则：
「只从标准档取样」的措辞已改成「只从**常规档**取样（松下的 `Standard` / `Natural`）」，
把 Monochrome / Cinelike / V-Log 这类明显不同的档排除在统计外。

### 2.4 对成本判断的影响（好消息）

* 读 PhotoStyle 需要**内嵌 JPEG**，而 **W6b 反推曲线本来就要取内嵌 JPEG** ——
  所以「顺便解析它的 EXIF MakerNote」是**叠加成本，不是新增的一次性成本**。
* 本仓**已有 TIFF IFD 遍历可复用**（`crates/raybend/src/media/tiff.rs`，它连 RW2 的
  `IIU\0` magic 与私有 tag 都处理了）—— 写 Panasonic MakerNote 的小解析器不用从零开始。
* `rawler 0.8.0` 的 `Exif` 结构与 `kamadak-exif` **都不解析厂商 MakerNote 内部**
  （后者只给到 MakerNote 的原始字节），所以确实要自己写。

---

## 三、待人类决定：封面用的**有损 WebP** 编码

人类定的规格是「转成 **80% 质量**的 webp」。但查证后发现一个障碍：

> `image` 0.25 的 WebP 编码器**只有无损**
> （`src/codecs/webp/encoder.rs` 顶部注释：*"Right now only lossless encoding is supported.
> If you need lossy encoding, you'll have to use `libwebp`."*，API 只有 `new_lossless`）。

所以「有损 80%」要么引入 `libwebp`（`webp` crate），要么换一个等效方案。三个候选：

| 方案 | 依赖成本 | 768×576 封面体积（估） | 质量 |
| --- | --- | --- | --- |
| **A. `webp` crate（libwebp 绑定）** | **新依赖 + 系统库**。Windows 上属 dav1d 同类风险（要专门构建，或源码编译需要 cc/nasm） | ~60–100 KB | 有损 80，符合原规格 |
| **B. `image` 的无损 WebP** | 零（已开 `webp` feature，纯 Rust） | ~300–600 KB（照片类内容无损压不动） | **无损**，比有损更好 |
| **C. AVIF**（`image` 的 ravif 编码器，**已在用**） | 零 | ~40–80 KB（通常比 WebP 80 还小） | 有损，质量等效或更好 |

**判据补充（三条，都是事实）**：

1. 封面**只在本产品自己的 UI 里显示**，不导出、不外传 —— 格式选择纯粹是内部权衡；
2. 封面是**导入时烘焙一次**，不是每帧 —— **编码速度无所谓**，体积与质量才是考量；
3. 项目已有 AVIF 规格约定（`AGENTS.md` §6.4：质量 90 / 4:4:4），
   `thumbnail/render.rs::encode_avif` 现成可用。

**Agent 建议：选 C（AVIF）**，理由是零新依赖 + 体积最小 + 编码器已在用；
次选 **B**（若坚持「WebP」这个名字）；**不建议 A** —— 为一个封面引入 libwebp 的
跨平台构建负担，性价比不成立（dav1d 那套 `scripts/lib/dav1d-win.mjs` 的成本是有先例的）。

这条已写进 `Cargo.toml` 的注释与 `PLAN.md` M3-W6c，**等人类拍板**。

---

## 四、改动文件

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/assets/lut-sample.webp` | **新增**（剥离 EXIF 后的 85 524 字节版本） |
| `crates/raybend/src/develop/lut.rs` | **新增**：落点占位（3 个常量 + 2 个测试） |
| `crates/raybend/src/develop/mod.rs` | 登记 `pub mod lut;` + 更新模块目录树注释 |
| `Cargo.toml` | `image` 加 `webp` feature（含为何需要 + 有损编码待定的注释） |
| `PLAN.md` | M3-W6b 的对策二**重写**（纠正 PhotoStyle 位置 + 实测分布 + ORF 未验证的标注 + 成本合流说明） |
| `implementations/2026-09-25_render-base-curve-planning.md` | §3.1 加更正框；遗留 1 改写（松下已核实、奥巴待样本） |

---

## 五、验证

* `cargo test -p raybend --lib` → **1059 passed / 0 failed / 2 ignored**，14.37 s
  （其中新增 2 个：`develop::lut::tests::*`）。
* RIFF 结构程序化校验：size 字段自洽、chunk 序列正确、画布 768×576。
* **未做**（也做不到）的验证：这张封面样片在真实 UI 里的观感 —— 属人类目视范畴（§2.8）。

---

## 六、遗留与待人类

1. **有损 WebP 编码方案待拍板**（见 §三，建议 C：AVIF）。
2. **奥巴 E-M5 Mark II 的 ORF 样本仍缺** —— 本机 142 个 RW2 全是松下的，没有 ORF。
   奥巴的 `0x010c RawDevPictureMode` 位置（MakerNote 主 IFD）**尚未实测**，
   甚至不能排除奥巴与松下一样存在「RAW 里没有、只有内嵌 JPEG 里才有」的情况。
   W6b 开工前需要一份实拍 ORF。
3. `crates/raybend/src/media/tiff.rs` 已有 IFD 遍历 —— 但它是**给缩略图管线读基础信息**用的，
   不暴露任意 tag。W6b 要读 MakerNote 时，是**扩展它**还是**在 `develop/lut.rs` 旁边新写**
   一个只读 Panasonic MakerNote 的小解析器，留到 `plans/M3-W6b.md` 定
   （判据按 §2.12：先看现有那份能不能一般化）。
4. 人类提供的原件（带 EXIF 的 127 KB 版本）仍在 `/home/andares/repos/c-thun/` ——
   **没有动它**，入仓的是处理后的副本。
