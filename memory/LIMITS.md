# 硬性上限登记册（LIMITS）

> **为什么单独一份**：这些数字是**为性能 / 面板可读性 / 侧车体积**设的硬闸门，散在各个模块里很容易
> 「加功能时忘了还有上限」，或者「同一个上限在三处各写一个数」。集中登记之后，改一个上限**只改一处代码 +
> 改这一行**，review 时一眼能看出连锁影响。
>
> **纪律**（`AGENTS.md` §2.28）：**任何新增或改动的硬上限，必须在同一次改动里登记到这里**；
> 表里的「代码唯一真相」一栏写的是**判据本身住在哪**，本表只做索引，**不复述实现**。
>
> 建立：2026-10-31（崔总提出：LUT 8 / issue 32 / area 32 / tags 5000 这些应当有一处统一记着）。

## 1. 业务上限（用户会撞到、界面必须给反馈）

| # | 上限 | 值 | 作用域 | 代码唯一真相 | 到顶时怎么办 | 为什么是这个数 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | LUT 链层数 | **8** | 每张照片的 `lut_chain` | `crates/raybend/src/store/develop.rs::MAX_LUT_LAYERS`（GPU 侧 `render/creative_lut.rs::MAX_LAYERS`、着色器 uniform 同步） | 面板「加入」禁用；预设合并时多余层不加入并提示一次 | GPU 一趟 pass 内每层一次 3D 采样，8 层仍可实时（`specs/editor-lut-chain.md` §1） |
| 2 | 单张照片的定稿数 | **32** | 每个资产 | `crates/raybend/src/store/issues.rs::MAX_ISSUES_PER_ASSET` | 定稿按钮变灰 + 一句可懂的错误 | 面板可读性 + 侧车 `rb:profiles` 体积（序号虽到 99，业务上限另钉） |
| 3 | 编辑区层数 | **32** | 每张照片（**不含 `默认` 层**） | `crates/raybend/src/store/develop.rs::MAX_AREAS`（渲染侧 `develop/areas.rs::MAX_AREAS`、前端 `features/editor/store.ts::MAX_AREAS` 同步） | 两个「新建」按钮变灰 + 带上限数字的浮条 | 崔总 2026-10-31 定；层数直接乘进每像素内循环（W2 已接：`render_layered16`） |
| 3b | 编辑区层名长度 | **30** 字符 | 每一层 | `crates/raybend/src/store/develop.rs::MAX_AREA_NAME_CHARS`（前端输入框 `maxLength` 同值） | 输入框截断（`Array.from(name).slice(0, 30)`，按字符不按字节） | 面板行高 30、名字要留出开关与删除的位置；30 字符已够写「天空」「左脸阴影」这类描述 |
| 4 | 标签总数 | **5000** | 每个库 | `crates/raybend/src/store/tags.rs::MAX_TAGS` | 只能挂已有标签、不能新建；界面显示 `n / 5000` 并给「清理未使用」入口 | 「5000 已经远超正常使用」（代码注释） |
| 5 | 色彩批处理张数 | **256** | 一次批量操作 | `crates/raybend/src/store/color_batch.rs::MAX_PHOTOS` | 超出部分拒绝 | 单次事务与进度 UI 的上限 |
| 6 | 自动桶规则组 / 组内值 | **16 / 64** | 每条自动桶规则 | `crates/raybend/src/store/organization.rs::MAX_RULE_GROUPS` / `MAX_RULE_VALUES` | 加不进去，界面禁用加号 | 见代码注释 |
| 7 | AI 概念数 | **32** | 每张照片 | `crates/raybend/src/store/photo_tags.rs::MAX_AI_CONCEPTS` | 多余的不落库 | 见代码注释 |
| 8 | 最近目录 | **50** | 每个库 | `crates/raybend/src/store/recent.rs::DEFAULT_LIMIT` | 最早的自动淘汰 | AGENTS.md §11.5「最近」 |
| 9 | 库可用性探测 | **4** 路 | 每次探测 | `crates/raybend/src/store/availability.rs::MAX_PROBES` | 见代码 | 见代码注释 |
| 10 | 右栏文字字段长度 | **2000** 字符 | 每个字段（作者/描述/国家/省州/城市/具体地点） | `crates/raybend/src/store/marking.rs::TEXT_FIELD_MAX_CHARS`；前端 `lib/text-history.ts::TEXT_FIELD_MAX_CHARS` 同值（输入框 `maxLength`） | 输入框打不进去；后端再校一次并报错 | 够写一段描述，又不至于把界面/DB/侧车撑爆 |
| 11 | 建议输入候选数 | **20** 条/字段 | 右栏每一个可编辑字段 | `crates/raybend/src/store/marking.rs::TEXT_SUGGEST_LIMIT`（库内已有值）+ `src/lib/text-history.ts::TEXT_SUGGEST_LIMIT`（最近输入） | 超出部分不进下拉；最近输入按 MRU 挤出最旧的 | 下拉 8 行可见、最多滚两屏；20 条覆盖一次旅行的常用地点 |

## 2. 容量预算（不是"条数"，是"字节与比例"）

| 上限 | 值 | 作用域 | 代码 / 规格 | 到顶时怎么办 |
| --- | --- | --- | --- | --- |
| 384 小图缓存 | **2 GiB** | 每个库 | `crates/raybend/src/thumbnail/cache.rs::DEFAULT_MAX_BYTES`；`AGENTS.md` §6.5 | 按 frecency 淘汰、按需重建 |
| 真身 / 1920 缓存 | **5 GB 或可用空间 5%（取小）** | 每个库 | `AGENTS.md` §6.5（`FullCache::maybe_enforce_capacity`） | 真身档先删、同档最旧先删 |
| 单库建议规模 | **15 万张** | 每个库 | `AGENTS.md` §6.5 | **不阻断**，库设置里提示建议开新库 |

## 3. 技术性上限（用户一般不会撞，但改代码要认）

这些是格式 / 协议 / 平台约束，不是产品决策；**各自跟着自己的代码走，本表只列指针**：

| 项 | 值 | 出处 |
| --- | --- | --- |
| 文件 / 目录名 | 255 字节 | `crates/raybend/src/repo/dirs.rs::MAX_NAME_BYTES` |
| 导入序号宽度 | 9 位 | `crates/raybend/src/import/template.rs::MAX_SEQ_WIDTH` |
| 标签名字 | 64 字符 | `crates/raybend/src/store/tags.rs::MAX_TAG_NAME_CHARS` |
| 桶名字 | 80 字符 | `crates/raybend/src/store/organization.rs::MAX_BUCKET_NAME_CHARS` |
| 视口最大缩放 | 64× | `crates/raybend/src/render/viewport.rs::MAX_ZOOM` |
| 文件重名后缀尝试 | 999 次（`_01`…`_999`） | `crates/raybend/src/store/relocate.rs::MAX_RENAME_ATTEMPTS`；回收站还原同一口径见 `store/recycle.rs`（`attempt <= 999`） |

## 4. 待补

* 预设数量 / 预设目录数目前**没有上限**（只有名字长度校验）。要不要设、设多少，等真实使用反馈。
* 上面的「为什么是这个数」有几条只写了"见代码注释"——将来动到那一条时顺手补成一句话理由。

## 5. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-10-31 | 建立；登记 LUT 8 / 定稿 32 / area 32（规划）/ 标签 5000 等 9 条业务上限、3 条容量预算、5 条技术性上限 |
| 2026-10-31 | area 32 落地（W2/W4）：代码唯一真相 + 面板变灰与浮条已接；新增「编辑区层名 30 字符」一条 |
| 2026-11-03 | 新增右栏「文字字段长度 2000 字符」与「建议输入候选 20 条/字段」两条（`TEXT_FIELD_MAX_CHARS` / `TEXT_SUGGEST_LIMIT`，前后端同值） |
| 2026-11-05 | 新增技术性上限「文件重名后缀尝试 999 次」（移动资产 `relocate` 与回收站还原共用同一口径；此前只活在 `recycle.rs` 里没登记） |
