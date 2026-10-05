# XMP sidecar：自有 rb: 表达 + 标准层输出 + 自家导入

**状态**：**已实施完成**（崔总 2026-09-30 批准「按照 spec 实施」，Agent 同夜完成——单测/编译/lint 门全绿，真机验收待崔总，见 §10 与 `implementations/2026-09-30_xmp-sidecar-w1.md`）。
**范围已冻结（崔总 2026-10-01）**：XMP 功能到此为止，**不适配其他家的导入**（不是延后，是不做）；
本规格即**全部**实现方案，**没有分波次**——本文件就是一次性做完的那一份（原文件名中的「W1」已随更名移除）。
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
- 不做 XMP 相关界面（**无设置开关、无命令**；见 §9）。
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
| `issue_create` / `issue_delete` | 该资产 |
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
- **无设置开关**：写出是数据资产表态的一部分，默认且唯一行为 = 编辑/标记时自动写——避免误关造成数据残缺。将来若确有需求再加（另立波次）。
- **界面零新增**：最多在导入摘要/日志里出现采纳计数。

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

#### 人类真机验收（GUI/视觉归人类）

1. 编辑一张 RAW 基照片 → 照片旁出现 `<名>.xmp`；文本编辑器可见 `rb:`（latest + 定稿）与 `crs:` 调整。
2. 定稿、改评级/色标/标签/说明 → sidecar 随之更新（`rb:profiles`、`xmp:Rating`、`xmp:Label`、`dc:subject`）。
3. latest 切到 SOOC 再编辑 → `crs:` 调整消失，`rb:` 仍记录。
4. 重置全部编辑并清空所有标记 → sidecar 被删除。
5. **拷库演练**：整个 repos 目录拷到别处（或当新库导入）→ 编辑、定稿、评级、色标、关键词、说明都回来。
6. 用 ExifTool / Bridge / Lightroom 打开带 sidecar 的照片 → 能读到评级、色标、关键词与基础调整（近似）。
7. 删除一张照片 → 回收站里能看到照片与它的 `.xmp`。

## 11. 实施中采用的设计决定（原「自主决定与待审项」，已随批准实施生效）

以下是写规格时**替崔总定**的七项（崔总批准实施时一并生效；如需反悔，改动点均在对应小节）：

1. **内容全空 ⇒ 删除 sidecar**（「没有就没有」的一致化；仅删含 `rb:` 的文件）。
2. **外来同名文件（无 `rb:`）⇒ 备份为 `.xmp.bak` 后接管**（不静默毁掉别家数据）。
3. **源目录 sidecar 不复制进库**（只读采纳）。
4. **曲线纳入 `crs:` 映射**；温度/几何/镜头/LUT/基准曲线不映射（理由 §5.1）。
5. 地点字段用 `photoshop:` + `Iptc4xmpCore:Location`；`Cyan` 无 Adobe 对应值（照写）。
6. **无设置开关、无命令、界面零新增**（§9）。
7. 导入采纳同时覆盖**重扫/重建**路径（灾难恢复语义）。

## 12. 范围冻结（崔总 2026-10-01）

- **本规格的全部范围 = §1.1 的三项，已一次性实施完成；没有分波次，也没有 W2。**
- **适配其他家的导入（Lightroom / darktable 等方言读取）不会做**——不是延后，是不做（崔总原话）。
- 同样不在范围内：`crs:` 扩展映射（几何/裁切/温度）、GPS/版权字段写出——范围就这些。
- 「创建库时引用已有目录」不属于 XMP，归 `memory/FUTURE.md` J 节的备份概念。
- 唯一的持续义务：`AGENTS.md` §2.19——将来改动会进 sidecar 的功能（如地理位置落地）时，
  必须在同一次改动里接好 XMP 侧影响；若 H4 标签管理面板开工，标签改名/合并须同步刷新受影响资产的 sidecar。
  这不是 XMP 的后续功能，而是其它功能对 XMP 的**联动义务**。
