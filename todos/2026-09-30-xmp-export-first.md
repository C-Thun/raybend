# XMP 主线：范围暂存与生态查实材料（导出优先、导入不急）

需求时间：2026-09-30（口径修正）
状态：**已转入规格**（同日定案并写出）——本文转为材料存档，不再维护；现行为以 `specs/xmp-w1.md` 为准

> 用途：本文件是需求与生态查实材料的存档（`AGENTS.md` §10 的 `todos/` 约定）。
> **规格已写：`specs/xmp-w1.md`（2026-09-30 草案，待崔总审；审后交 GLM 实施）。**
> 决策账本在 `memory/REVIEW.md` R6-01～R6-04；路线在 `memory/PLAN.md` §4 #2；原文在 `docs/user-requirements.md` 2026-09-30 两节。
>
> ⚠️ 本文写作较早（当时范围还是「导出优先、导入不急」）；同日后续已把**元数据**与**自家导入**纳入本波，
> 并定「XMP 不是第一公民」，**差异以 `specs/xmp-w1.md` 为准**。

---

## 一、本轮已定（崔总 2026-09-30）

- **导出做，导入不急**：先建立**自有格式表达与 sidecar**（写出）；导入兼容**不投入优先级**。
- **目的 = 表态**：开放态度、用户的数据资产归用户、开源精神。别的软件能否读我们的 sidecar 属**边界之外**，不作为投入依据。
- **目标用户口径（崔总）**：基本盘 = 「有拍照、但舍不得买正版、一直在凑和」的用户（崔总本人即此类），不指望从大软件抢用户。
- **判断依据**：无感导入条件过苛（旧软件要开着写 sidecar + 用标准命名）；别家导入兼容点到为止、欠维护；导入是一次性事件，而兼容是持续欠账。

## 二、需求原文（节选；完整原文见 `docs/user-requirements.md` 2026-09-30 节）

> 所以我对这个功能现在的想法就是，我们支持导出，但导入不着急支持，可以先建立我们自己的格式表达和 sidecar，
> 目的其实是表态，表态我们的开放态度，用户的数据资产归于用户，这也是开源的精神。
> 但至于你找不找得到别的软件兼容，那是另一回事，这是边界之外的事

## 三、开工前规格要点（写 `specs/xmp-*.md` 时用）

**导出侧（先做）**

- 写出 issue 定稿与 `latest` 的 XMP 表达——格式契约已定：`specs/issue-xmp-contract.md`（自有命名空间 `rb:`）。
- **待拍板**（`memory/REVIEW.md` R6-02）：是否同步写**标准层基本项**（`xmp:Rating`、`dc:subject` 关键词、标题/说明、GPS 等标量字段）。
  理由：只写 `rb:` 私有命名空间时，「数据归用户」的表态对外**不可验证**；标准层字段有 Adobe/IPTC 官方文档、成本低、无持续维护账。
  调整参数不做跨家映射（那是欠维护的部分）。
- 写出时机与范围（手动/自动、单张/批量/整库）、失败重试、原子替换（临时文件 + 替换，失败不回滚已落库编辑——契约已写）。
- 界面先行（`.pen` + 同名 `.md`，`AGENTS.md` §5.1）；命令注册表接入 + 默认热键决定（§2.15）；DB 变更走迁移框架（§2.16）。

**导入侧（不开工，留登记）**

- 若未来拉回，最低验收入口 = 读标准层评级/标签/GPS；决策与材料见下。
- **待拍板**：「导入不急」的边界是否也涵盖**读回自家 sidecar**（库重建 / 跨机迁移 / 灾难恢复）——
  若无读回，自表述只是单向宣言；若有，成本远低于跨家兼容（格式我们自己定）。

## 四、生态查实材料（2026-09-30 Agent 网络核实；写规格或做判断时直接引用）

### 4.1 XMP 规范性分层

| 层 | 规范 | 性质 |
| --- | --- | --- |
| 数据模型 + XML 序列化 + 核心属性 | **ISO 16684-1:2019**（第 2 版） | 国际标准 |
| schema 描述语言 | **ISO 16684-2:2014** | 国际标准 |
| 存储 / 嵌入 / 旁挂 | **Adobe XMP Specification Part 3**（2020 版，公开） | 事实标准（非 ISO） |
| 照片编辑字段 `crs:` 等 | Adobe 官方命名空间文档（Developer 站 + Part 2） | 官方文档化 |
| `lr:`（Lightroom 特有） | Adobe 官方命名空间清单里**没有** | 靠 ExifTool / exiv2 等第三方整理 |
| 跨软件写行为（何时写/写什么/嵌还是挂/冲突谁赢） | **无规范** | 全靠各家默认值与设置项 |

Adobe Part 3 对 sidecar 的全部表述只有一句 **"should" 级建议**（非 MUST）：

> "For applications that need to find external XMP files, look in the same directory for a file with the same name as
> the main document but with an .xmp extension. (This is called a sidecar XMP file.)"

### 4.2 软件行为矩阵（逐家核实）

| 类别 | 软件 | 核实到的行为 |
| --- | --- | --- |
| **严格只写 sidecar、从不嵌入** | darktable | 所有图像（含 JPEG）都写 `.xmp`，原文件只读；**读** Lightroom 命名（`<name>.xmp`）但**不写它**，另写自己的 `<name>.<ext>.xmp`；设置项 never / on import / after edit |
| | Capture One | 官方支持文档："only the .XMP sidecar files could be updated (modified)"；社区仍在提"支持嵌入"的 feature request |
| **RAW→sidecar / 非 RAW→嵌入** | Lightroom Classic | 官方帮助：RAW 旁挂以避免损坏；JPEG/TIFF/PSD/PNG/DNG 写进文件本体，且**不能**强制 JPEG 走 sidecar。⚠️ **默认全部留 catalog，什么都不写**（手动保存或开"自动写入 XMP"才写） |
| | DxO PhotoLab | 官方指南："For RAW files, this metadata is recorded in annex files, also called sidecars; for RGB files metadata is stored in fields provided for this purpose" |
| | Photo Mechanic | 可配置；官方论坛："JPEGs do not need sidecar files for XMP and Photo Mechanic doesn't support them for JPEGs" |
| | FastRawViewer | 默认 RAW 写 sidecar（自称 analogous to Adobe Bridge）；JPEG / TIFF / PNG / HEIC 的 sidecar 默认**关闭** |
| **可配置两可** | digiKam | 写粒度三档（只 sidecar / 双写 / 只读文件才 sidecar）＋"命名兼容商业软件"开关 |
| | Bridge / Camera Raw | 同 Lightroom 逻辑 |
| **不用 XMP sidecar** | RawTherapee | 处理参数用自有 `.pp3`；XMP 主要只读评级（新版有元数据同步实验） |
| | ON1 Photo RAW | 自有 `.on1`（社区长期提"switch to xmp sidecar"请求） |
| | DxO（校正参数） | 自有 `.dop`（与元数据分家） |

**命名方言**：`<name>.xmp`（Adobe / C1 / FastRawViewer 默认 / digiKam 兼容模式）vs
`<name>.<ext>.xmp`（darktable 写出 / digiKam 默认）vs `<name>_<n>.<ext>.xmp`（darktable 副本版本）。

**「点到为止 / 欠维护」实证**：darktable 是最认真的进口者（读 LR 的 tags / 色标 / 评级 / GPS + 部分开发步骤），
其文档自己注明部分映射模块**已 deprecated**、"永远不会得到相同结果"；RawTherapee 几乎只读评级。

**冲突处理乱象**：嵌入与 sidecar 并存时行业做法是比 `xmp:MetadataDate` 取新，
但 FastRawViewer 手册指出 "several programs, ignoring the standard, do not record that tag"。

### 4.3 主要来源

- ISO 16684-1 https://www.iso.org/standard/75163.html
- Adobe 规范总入口 https://developer.adobe.com/xmp/docs/xmp-specifications/ ；Part 3 原文 https://www.thedigitalwalters.org/Data/DigitalGalen/Documents/External/XMP/XMPSpecificationPart3.pdf
- Lightroom Classic 官方 https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/metadata-basics-actions.html
- darktable https://docs.darktable.org/usermanual/development/en/overview/sidecar-files/sidecar/ 与 .../sidecar-import/
- Capture One https://support.captureone.com/hc/en-us/articles/360002544898-Metadata-in-XMP-sidecar-files
- FastRawViewer https://www.fastrawviewer.com/usermanual17/xmp-metadata
- DxO https://userguides.dxo.com/photolab/en/managing-images/
- digiKam https://docs.digikam.org/en/setup_application/metadata_settings.html

## 五、验收提示（做出来时怎么算「表态」成立）

1. 开启「自动写 sidecar」后，评级/标签/定稿能跟着照片文件走（`.xmp` 与照片同目录、同行移动）；
2. 关闭开关后 raybend 不写任何 sidecar，功能不受损；
3. （若 R6-02 拍板）标准层基本项在中立工具（Bridge、Windows 资源管理器等）中可直接读到；
4. （若「读回自家 sidecar」拍板纳入）删掉 catalog.db 后能靠 sidecar 重建评级/标签/定稿。
