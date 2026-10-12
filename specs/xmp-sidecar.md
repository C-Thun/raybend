# XMP sidecar：自有 rb: 表达 + 标准层输出 + 自家导入

**状态**：**已实施完成**（崔总 2026-09-30 批准「按照 spec 实施」，Agent 同夜完成——单测/编译/lint 门全绿，真机验收待崔总，见 §10 与 `implementations/2026-09-30_xmp-sidecar-w1.md`）。
**范围已冻结（崔总 2026-10-01）**：XMP 功能到此为止，**不适配其他家的导入**（不是延后，是不做）；
本规格即**全部**实现方案，**没有分波次**——本文件就是一次性做完的那一份（原文件名中的「W1」已随更名移除）。
**2026-10-09 增补（崔总问答拍板）**：只重开原 §9 的「无设置开关」一条——设置里新增**唯一一项**「写入 XMP 文件」开关（设备级、**默认开**、可关不删）；
语义见 `specs/xmp-write-toggle.md`，验收工具 `pnpm check:xmp` 与 §10 详细清单同日落地；其余范围仍按 2026-10-01 冻结。
**一句话**：编辑/标记照片时在照片旁自动写 `<主体名>.xmp`；自家导入（新登记资产）自动发现并读回；不读别家方言、不做历史补写。
**关联**：
`specs/issue-xmp-contract.md`（rb: 表达契约，本文件细化并小幅修订它）、`memory/PLAN.md` §4 #2、
`memory/REVIEW.md` R6-01/02/03、`memory/FUNCTION-REPOSITORY.md` §4.1（`_RAW/` 布局）、
`memory/FUNCTION-BROWSE.md` §7（标签）、`todos/2026-09-30-xmp-export-first.md`（生态查实材料）。

---

## 0. 定位（崔总 2026-09-30 口径；写进实施记录与文档的措辞）

- **XMP 不是第一公民**：真相源永远是 `catalog.db`；sidecar 只是互操作副本，不承担备份与完整性职责。
- 它的两个目的：① **宣示开放**——用户的数据资产归用户、随时可带走；② **照片 + sidecar 离开库时能恢复可观信息**（best-effort，**不承诺完整**）。
- **备份不是 XMP 的事**：整库备份/迁移的正路是**直接拷贝 repos 目录**（`catalog.db`、`photos/`、`cache/` 都在仓内）；
  崔总 2026-09-30：`catalog` 结构**可能连导出都不需要做**，顶多后续在「创建库」流程加一个「引用已有目录」的入口（归 `memory/FUTURE.md` J 节的备份概念，与本规格无关）。
- 因此全部取舍服从「够用即可」：宁可少映射、少承诺，不多做界面与自动化。

## 1. 范围

### 1.1 做

1. **写出**：编辑或标记照片时，在照片旁写 `<主体名>.xmp`：
   - `rb:` 域：**全部 issue**（不可变定稿，含 `sourceBase` raw/sooc）＋ `latest` 工作副本；
   - 标准层：`crs:` 调整（**仅当 latest 基于 RAW**）＋ 基础元数据（评级/色标/关键词/作者/说明/地点）。
2. **自家导入**：**新登记资产**（导入落库、重扫/重建）自动发现同名 sidecar，读回 `rb:` 与标准层元数据；无需用户操作。
3. 生命周期（写出时机、清空删除、随照片回收站）与外来同名文件的保护（§6、§3.3）。

### 1.2 不做（边界，防蔓延）

- 不读别家方言（Lightroom `crs:`/`lr:`、darktable 等）——**不做**（崔总 2026-10-01 定：不适配其他家的导入；`memory/REVIEW.md` R6-01/R6-05）。
- 不为历史数据补写 sidecar（升级不回溯；「编辑了才有」）。
- 不映射几何/裁切/旋转、镜头手动项与配置文件、LUT、动态反差、降噪方式、基准曲线（理由见 §5.1）。
- 不写旗标/喜欢/锁/桶/集合/目录标签/GPS/版权（无标准意义、无 DB 字段或已有 EXIF 承载，见 §5.2）。
- 不做 XMP 相关界面，除唯一一项写出开关（**无命令**；见 §9 与 `specs/xmp-write-toggle.md`）。
- 不承担备份职责（§0）。
- 不复制/搬运源目录里的 sidecar 进库（只读采纳；§7.1）。

## 2. 现状与复用（2026-09-30 代码核对）

| 事实 | 位置 |
| --- | --- |
| issue（不可变定稿）：`name / source_base / created_at / schema_version / profile_json / profile_hash / ordinal`；`profile_hash` = FNV-1a 64 位十六进制，**剔除 `auto_adjust`** | `store/issues.rs`、`store/develop.rs` |
| `latest`：`develop_stacks`；空栈（`is_empty` 且 raw 基）会被删除 | `store/develop.rs` |
| `EditBase` = `raw` / `sooc`；`DevelopStack` 字段：params / curves / as_shot_k / lens_profile / base_curve_* / lut_* / lens_enabled / nr_method / geometry / auto_adjust | 同上 |
| 曲线通道 = `rgb` / `r` / `g` / `b`，控制点归一化 0..1 | `develop/curve.rs` |
| 资产 =「同目录 + 同主体」；RAW 与位图配对时进同级 `_RAW/`，RAW-only 直接在本目录 | `import/plan.rs`、`store/assets.rs` |
| 标签：**词典在 `app.db`**，关联 `asset_tags` 在 `catalog.db`（跨库引用无外键） | `store/tags.rs` |
| 可编辑文字：author / description / country / province_state / city / sublocation；标记：rating 0..5、color_label 六色（red/yellow/green/cyan/blue/purple） | `store/marking.rs`、migration `catalog_0002` |
| 既有 XMP 工具：写（dc: 子集，手写字符串 + `escape()`）在 `export/metadata.rs::xmp()`；读（roxmltree）在 `merge_xmp()`；原子写 `fs_atomic::write()` / `write_new()` | 同上 |
| 扫描跳过 `.xmp`（`kind::is_sidecar`），文件监听也忽略 `.xmp`（不会自触发重扫） | `media/kind.rs`、`media/scan.rs`、`media/watch.rs` |

**复用要求（`AGENTS.md` §2.12）**：XMP 的**转义与 dc: 列表构造**抽成共享函数，供 sidecar 与导出共用——**不写第二套**；
sidecar 侧只新增 `rb:` / `crs:` / `photoshop:` / `xpacket` 部分。既有 `export/metadata.rs` 的测试是这次抽取的回归网。

## 3. 文件位置与命名（崔总 2026-09-30 定）

### 3.1 路径规则（唯一一处实现）

```text
sidecar_path(asset) = <照片目录>/<主体名>.xmp
```

- 「照片目录」= **位图**所在目录；没有位图时用 RAW 所在目录；**若该目录是 `_RAW/`，折算回其父目录**（复用 `store::assets` 的 `_RAW` 折算规则，不另写一套）。
- 主体名 = 文件名去扩展名：`IMG_0001.CR3` → `IMG_0001.xmp`；位图 + `_RAW/IMG_0001.CR3` 也共用 `IMG_0001.xmp`。
- **绝不写进 `_RAW/`**。
- 查找时大小写折叠匹配（Windows/macOS 天然不敏感；Linux 需按目录项折叠比较）。

### 3.1.1 库内移动（删除 / 还原 / 真删）

照片进回收站（`specs/library-recycle-bin.md`）时文件**真实移动**到 `recycle/<photos 内相对路径>`，
**sidecar 跟着走**（同目录镜像，与位图/RAW 同一批操作）：

| 动作 | sidecar |
| --- | --- |
| 删除（移入回收站） | 跟照片一起移入 `recycle/…`（仅**我们的**：`is_ours` 守卫；移不动只记日志） |
| 还原 | 跟照片一起回原位（改名时主体名跟随新的名字） |
| 真删 / 清空 | 与照片一起删除 |
| 巡检（`pnpm check:xmp`） | 只扫 `photos/`；回收站内的 sidecar 不在巡检范围（也不误报为孤儿） |

### 3.2 与 `_RAW/` 的关系（已知且接受的代价）

位图 + RAW 配对时，sidecar 通常在 RAW 的**上一层目录**：别的软件按「同目录同名」去找 RAW 的 sidecar 会找不到。
崔总 2026-09-30：这是**导入软件要处理的事**——我们不做兼容变通（不双写、不放 `_RAW/`）。

### 3.3 文件归属与保护

判据：**内容含 `rb:` 命名空间 = 我们的文件**。写入前若同名文件已存在：

| 情况 | 处理 |
| --- | --- |
| 含 `rb:`，版本已知且所有条目完整可读 | 正常覆盖（原子替换） |
| 含 `rb:`，版本未知/非法、编码或 XML 损坏、条目校验失败/未知资源 | **不覆盖**，跳过并记录（契约：保留原 XMP 并报不支持） |
| **不含 `rb:`**（别家软件的文件） | 先重命名为 `<名>.xmp.bak`（`.bak` 已存在则覆盖该 `.bak`），再写我们的——不静默毁掉别人的数据 |

## 4. rb: 域（自有 schema）

命名空间 `https://raybend.app/ns/issue/1.0/`，前缀 `rb`。

| 属性 | 形式 | 说明 |
| --- | --- | --- |
| `rb:latest` | 资源（`rdf:parseType="Resource"`） | 工作副本：`rb:latestSchemaVersion`、`rb:sourceBase`、`rb:profileHash`、`rb:profileJson` |
| `rb:profiles` | `rdf:Seq` | 全部不可变定稿；每个 `rdf:li` 含：`rb:name`（UTF-8）、`rb:sourceBase`（raw/sooc）、`rb:createdAtMs`、`rb:ordinal`、`rb:schemaVersion`、`rb:profileHash`、`rb:profileJson`。**即使没有定稿也写出空的 `rb:profiles`**——它是 §3.3「我们的文件」判据的载体，纯元数据 sidecar 靠它不被自己误判为外来文件 |

- `rb:profileJson` = `DevelopStack` 的规范 JSON，**剔除 `auto_adjust`**——与 `profile_hash` 同一口径，任何读者可**直接重算哈希**校验（本规格同步修订契约，见 `specs/issue-xmp-contract.md`）。
- 版本由核心 `issues::profile_schema_version` 决定；没有色彩状态的旧 profile 继续写 1，新色彩状态写 2，不因程序支持上限提升而统一升级旧配置。色彩管理和照片标签来源的新增字段按各自规格同步接入，沿用本文件的损坏/未知保护与事务读回。
- `rb:profileHash` = 现有实现（`issues::profile_hash`）的 FNV-1a 64 位十六进制。 JSON 解析必须保持浮点精确往返，现有 serde_json 启用 float_roundtrip；不改哈希算法或旧 profile 的格式。
- `rb:ordinal` = 定稿的导出尾号（0–99），**提示性、不是跨库身份**；导入时优先沿用，冲突/超限由现有分配器另取（`allocate_ordinal` 本就会跳过已占用的号）。
- 未知 `schemaVersion` / `latestSchemaVersion`：**跳过该条并记录**，不按旧版本硬解析。
- 体积上界：100 个定稿 × 每个 profile JSON 约 2–5 KB ⇒ 单文件最坏数百 KB；不压缩（保持人可读）。

**样例**（只写非空项；缩进仅为可读）：

```xml
<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="RayBend 0.1.1">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
        xmlns:xmp="http://ns.adobe.com/xap/1.0/"
        xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
        xmlns:dc="http://purl.org/dc/elements/1.1/"
        xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
        xmlns:Iptc4xmpCore="http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/"
        xmlns:rb="https://raybend.app/ns/issue/1.0/"
        xmp:CreatorTool="RayBend 0.1.1"
        xmp:MetadataDate="2026-09-30T21:31:00+08:00"
        xmp:Rating="4"
        xmp:Label="Green"
        crs:ProcessVersion="11.0"
        crs:HasSettings="True"
        crs:RawFileName="MYP0001.ORF"
        crs:Exposure2012="0.35"
        crs:Contrast2012="12"
        photoshop:City="杭州"
        Iptc4xmpCore:Location="西湖">
      <dc:subject><rdf:Bag><rdf:li>旅行</rdf:li></rdf:Bag></dc:subject>
      <dc:creator><rdf:Seq><rdf:li>崔总</rdf:li></rdf:Seq></dc:creator>
      <dc:description><rdf:Alt><rdf:li xml:lang="x-default">雨后的湖</rdf:li></rdf:Alt></dc:description>
      <crs:ToneCurvePV2012>
        <rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq>
      </crs:ToneCurvePV2012>
      <rb:latest rdf:parseType="Resource">
        <rb:latestSchemaVersion>1</rb:latestSchemaVersion>
        <rb:sourceBase>raw</rb:sourceBase>
        <rb:profileHash>0f3a…</rb:profileHash>
        <rb:profileJson>{"sourceBase":"raw","params":{…}}</rb:profileJson>
      </rb:latest>
      <rb:profiles>
        <rdf:Seq>
          <rdf:li rdf:parseType="Resource">
            <rb:name>暖调</rb:name>
            <rb:sourceBase>raw</rb:sourceBase>
            <rb:createdAtMs>1759…</rb:createdAtMs>
            <rb:ordinal>0</rb:ordinal>
            <rb:schemaVersion>1</rb:schemaVersion>
            <rb:profileHash>ab12…</rb:profileHash>
            <rb:profileJson>{"sourceBase":"raw","params":{…}}</rb:profileJson>
          </rdf:li>
        </rdf:Seq>
      </rb:profiles>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
```

## 5. 标准层

### 5.1 调整（`crs:`）——**仅当 `latest.sourceBase == raw`**

崔总 2026-09-30：兼容层只针对**基于 raw 的 latest**；**latest 基于 sooc 就不写任何 `crs:` 调整**。
`latest` = editor 的最后工作状态（可能来自最后选中的某个 issue）。**只写能直映的项**，每个值都是近似，不承诺别的软件结果一致：

| 我们的参数 | crs: 属性 | 换算 |
| --- | --- | --- |
| `exposure`（−2..+2 EV） | `crs:Exposure2012` | 原值 |
| `contrast` | `crs:Contrast2012` | 原值 |
| `highlights` | `crs:Highlights2012` | 原值 |
| `blacks` | `crs:Blacks2012` | 原值 |
| `saturation` | `crs:Saturation` | 原值 |
| `vibrance` | `crs:Vibrance` | 原值 |
| `lumaNr` | `crs:LuminanceSmoothing` | 原值 |
| `colorNr` | `crs:ColorNoiseReduction` | 原值 |
| `sharpenAmount` | `crs:Sharpness` | 原值 |
| `curves["rgb"]` | `crs:ToneCurvePV2012` | 控制点 ×255 四舍五入取整 |
| `curves["r"/"g"/"b"]` | `crs:ToneCurvePV2012Red/Green/Blue` | 同上 |
| 常量 | `crs:ProcessVersion` | `"11.0"`（告知是 PV2012 参数） |
| 常量 | `crs:HasSettings` | `"True"` |
| RAW 文件名 | `crs:RawFileName` | RAW 的文件名（不含路径） |

- 曲线规范化（Lightroom 可接受的形状）：值域 0–255 整数、x 严格递增（同 x 去重）、首末补 `(0,0)` / `(255,255)`（缺失时）、点数 >32 时按 x 均匀抽样到 32（保端点）。
- **一个可映射项都没有时不写 `crs:` 块**（不写空的 `HasSettings`）。
- **明确不映射**（写进实施记录的理由）：
  `temperature`（我们的绝对 K 与 Adobe 白平衡模型不同尺度，硬写会误导）、`dynamicContrast`（无对应）、
  镜头手动项与 `lensProfile`（LR 有自己的 profile 体系与身份）、`lutId`（无标准对应）、
  `baseCurve`（角色不同于 LR 的色调曲线）、`nrMethod`、几何/裁切/旋转（另有表示，先不做）。
  **2026-10-10 复核（编辑会话管道 W4）**：白平衡改成「预设里存 Δmired 偏移量」后，
  sidecar 侧无任何变化 —— `temperature` 仍不写、`as_shot_k` 也不写；预设是设备级内部资产
  （`app.db`），不进 XMP。本条声明在本期后依然成立。

### 5.2 元数据（与 base 无关；只要有值就写）

| DB 来源 | XMP | 形式 |
| --- | --- | --- |
| `assets.rating`（**>0 才写**） | `xmp:Rating` | 整数属性 |
| `assets.color_label` | `xmp:Label` | 首字母大写：Red / Yellow / Green / Cyan / Blue / Purple（`Cyan` 无 Adobe 对应值，别家可能忽略——已知） |
| `asset_tags` → 词典名（`app.db`） | `dc:subject` | `rdf:Bag`，标签名 UTF-8 |
| `assets.author` | `dc:creator` | `rdf:Seq`（单值） |
| `assets.description` | `dc:description` | `rdf:Alt` + `xml:lang="x-default"` |
| `assets.country` | `photoshop:Country` | 简单文本 |
| `assets.province_state` | `photoshop:State` | 简单文本 |
| `assets.city` | `photoshop:City` | 简单文本 |
| `assets.sublocation` | `Iptc4xmpCore:Location` | 简单文本 |
| （卫生字段） | `xmp:MetadataDate` / `xmp:CreatorTool` | ISO 8601 / `RayBend <版本>` |

不写：旗标、喜欢、锁、桶/集合、目录标签、GPS（EXIF 本来就有）、标题/版权（应用无此字段）。

## 6. 生命周期

### 6.1 写出触发点（覆盖全部会改变 sidecar 内容的写入路径）

| 命令 | 触发的资产 |
| --- | --- |
| `develop_commit` | 该资产 |
| `issue_create` / `issue_rename` / `issue_delete` | 该资产 |
| `browse_mark`（评级 / 色标 / 标签 / 文字） | 受影响资产集合（`marking::apply` 的 ops；喜欢/锁不进 sidecar） |
| `browse_undo` / `browse_redo` | 补丁里受影响的资产（develop 与标记 op 都要覆盖） |

- **统一判据**：触发后重算「该资产有没有可表达内容」= `latest` 有调整或明确选择 SOOC ∥ 有 issue ∥ 任一映射字段非空。空 SOOC 栈仍须记住 sourceBase，只有空 RAW 栈可省略。
  有 ⇒ 写出；全空 ⇒ 见 §6.2。注意：崔总原话「创建时机仅限于编辑」因**元数据纳入**扩展为「编辑或标记」。
- 写出在 **DB 事务提交之后**、`blocking` 线程里做（不阻塞界面）；批量标记（一次几百张）按资产排队。
- 复用 `fs_atomic::write`（临时文件 + 原子替换）。
- 失败：只记录（日志 + 计数），**不回滚已落库的编辑**；下一次触发自然重试。不做独立重试队列（够用即可）。
- 同一资产短时间多次触发：后写覆盖前写；可选做同资产合并（实现自定）。

### 6.2 清空与删除

- 内容全空（无 issue、latest 无需表达、所有映射字段为空）：**删除**同名 sidecar（**仅当含 `rb:` 且本版本完整可读**）；版本未知/损坏文件依 §3.3 保留——「没有就没有」。
- 照片的全部文件成功进回收站（`store/delete.rs`）后：同名 sidecar 一并进回收站（仅当含 `rb:`；外来文件不动）。任一照片文件回收失败则保留 sidecar，回收前核对库会话。
- 库内移动/改名：应用当前没有该功能，不在范围内；将来若有，由 §3.1 同一路径函数跟随。

### 6.3 不补写

- 不为历史编辑/标记补写；升级后旧的编辑数据不回溯生成 sidecar（崔总 2026-09-30）。
- 不提供「全库生成 sidecar」的批量操作与命令。

## 7. 自家导入（自动发现）

### 7.1 触发与范围

- **仅新登记资产**时探测同名 sidecar：`import/sink.rs`（导入落库）与 `store/rebuild.rs`（重扫/重建）。
- **2026-10-12 增补：导入侧另有两道门（崔总拍板）** ——
  1. **开关**：右列底部「避免重复导入」下方新增勾选 **「导入 xmp 配置」**（设备级 `app.db` 键
     `import.import_xmp`，`src/api/db.ts` 的 `SETTING_KEYS.importXmp`；**默认开**）。关掉 = 导入整批不读 sidecar；
  2. **整组完整导入**：该文件所在**组**（位图 + 同组 RAW）的每个成员都真的导入了、
     而且**没改名**（无 `_01` 后缀）、没跳过、没失败（`import/plan.rs` 的 `PlannedItem.complete`）。
     拆开的情况（例：位图改名 `_01`、同组 RAW 被判重跳过）**不采纳** ——
     那种情况下源目录里的 `.xmp` 未必描述的就是这一份。
  * 重扫/重建路径（`store/rebuild.rs`）不受这两道门影响：那里的资产已经在库里，
    采纳的是「库里这份旁边那一个 sidecar」。
- 已存在的资产**不读、不覆盖**（DB 为真相源）。
- 探测路径：源文件目录（`_RAW/` 折算后）的 `<主体名>.xmp`；大小写折叠匹配。
- **源目录里的 sidecar 不复制、不移动**进库（只读采纳）；库内 sidecar 由编辑/标记动作自然生成。
- 导入流程内不得在 DB 写事务中做文件 IO：sidecar 在登记前读取、随登记批次落库（或登记提交后独立小事务）。

### 7.2 读取与落库

- 只认 `rb:` 域与 §5.2 的标准元数据字段；别家命名空间**不读**（不构成「跨家导入」）。
- 校验：`schemaVersion` 已知（沿用 `issues::profile_schema_version`：旧配置为 1，含色彩状态的配置为 2）→ 继续；`profileJson` 可解析回 `DevelopStack` → 继续；重算 `profileHash` 与文件内一致 → 继续；否则**跳过该条并记录**（不影响导入本身）。
- 编辑栈同时复用 `DevelopStack::validate` 完整校验参数、曲线控制点、LUT/基础曲线和几何；非法编辑条目跳过，不影响有效元数据。声明 sourceBase 在原始 JSON 的哈希通过后对齐，latest 与 issue 同一口径。
- latest、issue、元数据及关键词关联在同一 catalog 事务中采纳，事务内确认资产仍无用户编辑/标记/标签；缺省元数据不清空已有字段，说明同步刷新 FTS。任何数据库写失败整体回滚。
- 落库：
  - `rb:latest` → 写 `develop_stacks`（`auto_adjust` 置空）；
  - `rb:profiles` → 逐条建 issue（保留 name / sourceBase / createdAtMs；`ordinal` 优先沿用，冲突则重新分配；**同哈希 + 同配置的重复定稿跳过**）；
  - 元数据 → rating / color_label / author / description / country / province_state / city / sublocation 直接落列；
  - 关键词 → 先在 `app.db` 词典按名 `ensure`（折叠名相等即复用），再挂 `asset_tags`（跨库词典对齐，走既有 `store::tags` 函数）。
- 报告：导入摘要/日志计数（如 `sidecars_adopted`；失败逐条记录）——**不新增界面、不弹窗**。
- 幂等：同一 sidecar 重复导入时「资产已存在」⇒ 不再采纳；文件内重复定稿按哈希跳过。

## 8. 工程接入（实现指引）

- 新模块：`crates/raybend/src/xmp/` —— `mod.rs`、`path.rs`（§3.1 规则）、`packet.rs`（build/parse）、`mapping.rs`（§5 映射）。
- 复用：`export/metadata.rs` 的 `escape` 与 dc: 构造抽到 `xmp` 共享；导出侧改为调用（行为不变，既有测试即回归网）。
- 读：`roxmltree`（已有依赖；**不新引 XML 库**）。写：手写序列化（结构固定、转义集中一处）。
- 命令层接入：`src-tauri/src/develop.rs`、`issues.rs`、`browse.rs`（mark / undo / redo）；`store/delete.rs`（§6.2）。
- 导入接入：`import/sink.rs`、`store/rebuild.rs`（§7.1）。
- **DB 迁移：不需要**（无新表/新列、无 sidecar 状态跟踪——每次触发重算）；实现中若确需，走 `store/migration.rs` + `store/migrations/`（`AGENTS.md` §2.16）。
- 并发：DB 仍走单写者；sidecar 文件写在事务外、`blocking` 线程。

## 9. 界面、命令与设置（`AGENTS.md` §2.15 合规声明）

- **本功能无任何用户可触发命令**，不进命令注册表（也就没有默认热键问题）；实施记录须写明此结论与理由。
- **设置开关（2026-10-09 修订，取代原「无设置开关」）**：设置里有唯一一项 **「文件 → 写入 XMP 文件」**
  （设备级 `app.db` 键 `xmp.write_sidecar`；前端 `SETTING_KEYS.xmpWrite`），**默认开**。
  关闭 = 不写、也不删盘上文件，编辑照常落库，读回不受影响；重新打开从下一次编辑/标记起写（**不补写历史**）。
  完整语义、UI 与验收见 `specs/xmp-write-toggle.md`。
- **其余界面零新增（2026-10-12 修订）**：新增的唯一界面元素是导入右列底部那个勾选
  **「导入 xmp 配置」**（在「避免重复导入」下方，同为设备级偏好、默认开）；
  导入摘要/日志里的采纳计数照旧（`[sidecar] 新资产 sidecar 采纳：N 成功`）。
  * 按 `AGENTS.md` §2.15 核过：**它是设置项不是命令**，与「避免重复导入」同口径 ——
    不进命令注册表、不需要默认热键（实施记录里写明此结论）。

## 10. 测试与验收

#### 单元测试（必须齐备、秒级；覆盖边界：空/单元素/上下限/非法值/中文与 Unicode/超长路径/大小写）

- 路径：位图 + `_RAW`、RAW-only、`_RAW` 折算、中文主体名、超长路径、大小写折叠、`.xmp` 不被当照片。
- 序列化：XML 转义（`& < > " '`、中文、emoji、控制字符）、dc: 各形状、`rdf:Seq` 顺序、空字段省略、100 个定稿上限。
- 往返：build → parse → 与内存模型相等；哈希重算一致；空 SOOC 基准、换行/制表符文本与属性、多 Description、未知版本与非法编辑条目。
- 映射：每个 `crs:` 项的换算与舍入（0 与边界）、曲线 0..1→0..255 与规范化（去重/补端点/降采样）、无可映射项时不写 `crs:`、sooc latest 无 `crs:`。
- 元数据：rating=0 不写、六色映射、`dc:subject` Bag、地点字段、空值省略。
- 生命周期：全空 → 删除已知版本；未知/损坏文件清空仍保留；大小写折叠文件复用/备份/删除；照片回收失败与会话失效保留 sidecar；外来文件 → `.bak` 接管；未知版本 → 不改写；注入写失败不回滚 DB。
- 导入：新资产采纳（latest + issues + ordinal + 元数据 + 关键词词典 ensure）、重复跳过、哈希不符跳过、未知版本跳过、已存在资产不采纳。
- 竞态：同资产连续两次写（后写覆盖）、批量标记后写出集合正确。

#### Agent 冒烟

`cargo test`、`pnpm test`、`cargo check --workspace`；不跑 GUI E2E（`AGENTS.md` §2.8）。

#### 人类真机验收（GUI/视觉归人类；2026-10-09 版清单）

**先跑巡检工具**：`pnpm check:xmp "<库根>"`（`specs/xmp-audit-tool.md`）——文件层（路径规则/孤儿/哈希/元数据/`crs:` 规则/大小写折叠）由它一次核对；
人只做下面的 GUI 动作。跑之前**关掉应用**（跨系统同时读写同一个 SQLite 库不安全）。

**准备**：新建一个验收专用库（要删照片/清空/手改文件）；源目录备 6–8 张——RAW-only、JPG-only、RAW+同名 JPG、中文名（导入模板用 `:FILENAME`）、一张旁边预置不含 `rb:` 的「别家」xmp。
ExifTool 官网 https://exiftool.org/ （WSL：`sudo apt install libimage-exiftool-perl`；Windows：`winget install --id=OliverBetz.ExifTool -e`，第三方封装安装器）。

##### A 核心链路（必过）

| # | 操作 | 期望 |
| --- | --- | --- |
| A1 | 导入素材 | 源目录**没有**新 xmp（只读不写）；库内也还没有（没编辑没标记）；别家 xmp 不被采纳 |
| A2 | 编辑 RAW 基照片（曝光/对比/RGB 曲线）→ 确认 | 库内 `photos/<日期>/<主体名>.xmp` 出现，`_RAW/` 里没有；含 `rb:latest`（sourceBase=raw、profileHash、profileJson）与 `crs:Exposure2012`、`crs:ToneCurvePV2012`（端点 0,0 / 255,255）；**拖拽中途看目录：文件不变**（快照只在 confirm） |
| A3 | 定稿命名 | `rb:profiles` 多一条（name/sourceBase/createdAtMs/ordinal/schemaVersion/profileHash/profileJson）；同配置再定稿不重复 |
| A4 | 标记 4★、Green、标签、作者、说明（带换行）、城市/省/地点 | `xmp:Rating="4"`、`xmp:Label="Green"`、`dc:subject` Bag、`dc:creator` Seq、`dc:description` Alt（换行保留）、`photoshop:City/State`、`Iptc4xmpCore:Location` |
| A5 | latest 切 SOOC 再编辑；另测 JPG-only | `crs:` 整段消失，`rb:` 与元数据仍在；切回 RAW 基 → 回来。JPG-only 只有 `rb:`+元数据 |
| A6 | 重置全部编辑 + 清空所有标记文字 | `.xmp` 被删除（不留空壳） |
| A7 | `exiftool -G1 -a -s <照片或 .xmp>` | 评级/色标/关键词/作者/说明/地点/调整值与界面一致 |
| A8 | 删除该照片 | 回收站里同时有照片与 `.xmp`；库内两者皆无 |

##### B 防御与边界（必过）

| # | 操作 | 期望 |
| --- | --- | --- |
| B1 | 库内照片旁手放无 `rb:` 的 `.xmp` → 编辑 | 原文件变 `.xmp.bak`（内容原样），新 `.xmp` 是我们的 |
| B2 | 手改 `.xmp`（`rb:latestSchemaVersion` 改 999，或破坏 XML）→ 编辑 | 该文件**不被覆盖/删除**；编辑照常成功；日志有记录 |
| B3 | `.xmp` 改名成大写 `.XMP` → 编辑；中文/emoji/超长文本 | 复用同一文件，不产生第二份；文本读回无误 |
| B4 | 全选 200+ 张批量评级/色标 | UI 不卡；文件数量与内容齐；日志无 `[sidecar] …失败`；中途杀进程不伤库 |
| B5（可选） | 只读目录 / 磁盘满 | 写失败只记日志，编辑仍成功落库 |

##### C 恢复与互操作（C1 必过——这是 XMP 的立身之本）

| # | 操作 | 期望 |
| --- | --- | --- |
| C1 | 把 `photos/`（**带 `.xmp`**）拷到新位置 → 新库重建（或当导入源导入） | 编辑/定稿/评级/色标/标签/说明/地点全回来；日志 `[sidecar] 新资产 sidecar 采纳：N 成功`。注意：采纳是**只读**，导入后库内不会立刻多出 xmp（下次编辑才自然生成） |
| C2 | Bridge / Lightroom Classic（有就做）打开带 sidecar 的照片 | 评级/色标/关键词/作者/说明/地点可读；基本调整近似；`rb:` 为未知命名空间（预期） |
| C3（可选） | 「别家先写、我们后写」（B1 已覆盖） | 不静默吞掉 |

##### D 联动抽查

| # | 操作 | 期望 |
| --- | --- | --- |
| D1 | 给照片指定色彩状态（schemaVersion 2） | 往返一致 |
| D2 | AI 给照片打标 | `rb:tagState` / `dc:subject` 落盘，读回不丢来源标记 |

##### E 写出开关（默认开）

| # | 操作 | 期望 |
| --- | --- | --- |
| E1 | 设置里关掉「写入 XMP 文件」→ 编辑/标记一批照片 | 库内**不产生**新 `.xmp`；编辑全部照常保存；已有 `.xmp` 内容不变（清空到全空也不删） |
| E2 | 重新打开开关 → 再编辑一张 | 该照片 `.xmp` 出现；关期间编辑的那些仍没有（符合「不补写」） |
| E3 | 关闭状态下导入带自家 sidecar 的照片（新资产） | 编辑/定稿/评级**照旧读回** |

**出问题报什么**：现象 + 库内文件路径与实际内容（或 exiftool 输出）+ 终端 `[sidecar]` 行 + 源照片类型。

**巡检工具的定位**：它把上面 A/B 两组的**文件层**判据自动化；GUI 交互、视觉、回收站与别家软件读取仍由人确认（`AGENTS.md` §2.8）。

## 11. 实施中采用的设计决定（原「自主决定与待审项」，已随批准实施生效）

以下是写规格时**替崔总定**的七项（崔总批准实施时一并生效；如需反悔，改动点均在对应小节）：

1. **内容全空 ⇒ 删除 sidecar**（「没有就没有」的一致化；仅删含 `rb:` 的文件）。
2. **外来同名文件（无 `rb:`）⇒ 备份为 `.xmp.bak` 后接管**（不静默毁掉别家数据）。
3. **源目录 sidecar 不复制进库**（只读采纳）。
4. **曲线纳入 `crs:` 映射**；温度/几何/镜头/LUT/基准曲线不映射（理由 §5.1）。
5. 地点字段用 `photoshop:` + `Iptc4xmpCore:Location`；`Cyan` 无 Adobe 对应值（照写）。
6. ~~**无设置开关**~~ → **2026-10-09 修订：有唯一一项写出开关，默认开**（`specs/xmp-write-toggle.md`）；无命令、其余界面零新增（§9）。
7. 导入采纳同时覆盖**重扫/重建**路径（灾难恢复语义）。

## 12. 范围冻结（崔总 2026-10-01）

- **本规格的全部范围 = §1.1 的三项，已一次性实施完成；没有分波次，也没有 W2。**
- **适配其他家的导入（Lightroom / darktable 等方言读取）不会做**——不是延后，是不做（崔总原话）。
- 同样不在范围内：`crs:` 扩展映射（几何/裁切/温度）、GPS/版权字段写出——范围就这些。
- 「创建库时引用已有目录」不属于 XMP，归 `memory/FUTURE.md` J 节的备份概念。
- **2026-10-09 例外说明**：崔总同意的增补只有三项——写出开关（默认开）、验收清单细化、巡检工具；
  §12 冻结的其余内容（不做跨家导入、不扩展 `crs:` 映射、不做批量动作）全部不变。
- **2026-10-12 例外说明**：崔总要求给导入侧的自动采纳加两道门 —— 「导入 xmp 配置」开关
  与「整组完整导入」门禁（§7.1）。这是**收紧**（少采纳，不会多采纳），不改变 §1.1 的范围，
  也不破「不做跨家导入」这条底线。
- 唯一的持续义务：`AGENTS.md` §2.19——将来改动会进 sidecar 的功能（如地理位置落地）时，
  必须在同一次改动里接好 XMP 侧影响；若 H4 标签管理面板开工，标签改名/合并须同步刷新受影响资产的 sidecar。
  这不是 XMP 的后续功能，而是其它功能对 XMP 的**联动义务**。
