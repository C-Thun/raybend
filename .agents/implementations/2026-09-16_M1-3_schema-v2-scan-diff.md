# M1-3 前半：schema v2、扫描、变更追踪

完成时间：2026-09-16 02:51:12 CST

对应计划：`plans/M1-3.md`（v2）的步骤 **A / B / C**（步骤 D 起见后续记录）。
状态：**A / B / C 完成并提交**；D（EXIF）/ E（缩略图）/ F（实测）/ G（收尾）待做。

---

## 1. 本次改动的范围

把「磁盘上的照片」变成「库里的记录」这条路上最基础的三段：

| # | 做了什么 | 提交 |
| --- | --- | --- |
| A | `catalog.db` / `app.db` **schema v2**（标记、标签、地理、EXIF 时区）+ `store::tags` | `c548cc5` |
| B | `media::kind`（文件名解析与类型判定）+ `media::scan`（目录扫描） | `1cf2882` |
| C | `media::diff`（变更追踪纯算法）+ `store::assets`（落库） | `850fad1` |

计划外的两件（都是用户评审意见带来的）：

* **库的英文统一为 `repository`**（原 `library`）—— 用户明确约定，趁 v2 开工前一次性做完，
  否则新代码也要跟着改一遍。涉及表名/列名/模块名/文件名/文档名（`LIBRARY.md` → `REPOSITORY.md`）。
* **色标两套配色**（深色/浅色各 5 色）入 `tokens.css` 与 `DESIGN.md` §1.4，附实测对比度。

---

## 2. 涉及文件

### 新增

| 文件 | 内容 |
| --- | --- |
| `crates/raybend/src/store/migrations/catalog_0002_marking_tags_geo.sql` | 标记 3 列 + 文字/地理 7 列 + EXIF 时区 1 列 + `asset_tags` + 重建 FTS 纳入 `description` |
| `crates/raybend/src/store/migrations/app_0002_tags.sql` | 全局标签词典 `tags` |
| `crates/raybend/src/store/tags.rs` | 词典侧（取或建/改名/删除/用量）+ 关联侧（挂/摘/设/查/用量/清理）+ 跨库对齐 |
| `crates/raybend/src/store/assets.rs` | 文件记录读写、差分落库、位图↔RAW 配对、缺失标记 |
| `crates/raybend/src/media/kind.rs` | 扩展名解析、`MediaKind`、侧车判定、垃圾文件判定、配对用 stem |
| `crates/raybend/src/media/scan.rs` | 目录扫描 + 取消令牌 + 跳过规则 + 统计 |
| `crates/raybend/src/media/diff.rs` | 变更追踪（三轮配对 + 变化分类） |
| `crates/raybend/examples/scan-smoke.rs` | 真实目录扫描冒烟（只统计不写库） |
| `crates/raybend/examples/refresh-smoke.rs` | 临时库内跑完整链路（扫描→差分→落库→再扫） |

### 修改

* `store/migration.rs`：登记两条 v2 迁移；测试里的硬编码版本号改成 `supported_version()`，
  以后加迁移不会再破测试
* `store/mod.rs`、`media/mod.rs`：挂新模块与文档表
* `error.rs`：`NotALibrary` → `NotARepository`
* `tokens.css`、`DESIGN.md`、`FUTURE.md`、`plans/M1-3.md`：色标两套配色、RAW 预览推迟登记（B8）、
  评审意见落实表

---

## 3. 关键决策与理由

### 3.1 旗标不建列（用户明确）

`BROWSE.md` §3.2 已定「旗标只在内存」，本次 schema 也**没有**为它加列。
理由（用户原话）：打旗标本来就是为了临时挑一下，若持久化，挑完还得再反打一遍，很痛苦。
v1 已有的 `flag` 列语义收窄为「入库判定」这类需要持久化的值，**代码里与内存旗标分开命名**。
（后续有需要再改 —— 用户明确说「以后有需要再改嘛」。）

### 3.2 标签词典在 `app.db`、关联在 `catalog.db`，**故意不加外键**

标签是跨库公用的（在 A 库建的词，B 库也该能选到），但 `catalog.db` 必须能**单独搬走**
（`REPOSITORY.md` §2）。所以关联里的 `tag_id` 是弱引用：查不到名字**不是错误**，
UI 按「待命名」显示，库打开时用 `tags::register_tag_with_id` + `tag_usage` 做一次词典对齐。

### 3.3 标签不进 FTS

标签名住在 `app.db`，在 `catalog.db` 的 FTS 里冗余一份名字就会漂移。
搜索时在查询侧与 `tags` JOIN（登记在 `FUTURE.md` H8）。FTS v2 因此只加 `description`。

### 3.4 EXIF 时间没有时区 —— 口径写死在迁移注释里

`DateTimeOriginal` 不带时区（RapidRAW 也是这么处理的：读完转 UTC，用本机偏移）。
本项目的口径：**有 `OffsetTime*` 标签就按它换算并记 `taken_at_offset_min`；
没有就把墙上时间原样当 UTC 存、该列置 NULL**，显示端遇 NULL 按「无偏移」渲染——
这样用户看到的数字与相机/其它软件一致，而不是被我们本机的时区悄悄改掉。

### 3.5 扫描刻意单线程 + 广度优先

顺序读对 HDD / SMB / 云盘友好（随机寻道是它们的主要成本）；广度优先让「顶层有什么」
先出来，对导入界面更友好；同一目录内按折叠名排序 → **顺序确定**，这是导入时序号分配
可复现的前提。真正的并发留给 EXIF / 缩略图阶段（CPU 才是那里的瓶颈）。

### 3.6 差分三轮配对、身份优先，且**绝不产生删除**

1. `(volume_serial, file_id)` —— 改名/移动后不变；
2. 折叠路径 —— 同一位置（跨大小写也算同一处）；
3. 启发式（无身份信息时，如网络盘）：同目录 + 同大小 + 时间戳接近（±2s）+ 名字相似，
   **且文件名主体 ≥ 4 字符**。

第 3 条的字符数下限是被测试逼出来的：`a.jpg` 与 `z.jpg` 编辑距离只有 1，若不加限制，
两张毫不相干的照片会被认成同一张。宁可判成「一删一新」（用户看得见两个变化），
也不能把两张照片合并成一张（用户会丢照片）。

磁盘上找不到的文件只标 `missing_since`，**永不删记录**（拔盘、云盘离线、权限过期都会
让文件「消失」）。

### 3.7 三条被测试逼出来的行为（容易写错的）

* **同一路径但身份变了** = 文件被替换 → 算「改过」（大小与时间可能碰巧一模一样，
  但缩略图必须重做）；
* **纯 NFC/NFD 差异不算改名**（macOS 会写 NFD，否则每次扫描都报一堆噪声）；
  **大小写差异算改名**（用户看得见，展示路径要更新）；
* **全 0 身份**（部分文件系统会返回）视为「读不到」，不能当有效身份来配对。

---

## 4. 验证方式

### 已跑的命令与结果

| 命令 | 结果 |
| --- | --- |
| `cargo test -p raybend` | **263 passed / 0 failed**（另有 2 个文档测试），约 2 秒 |
| `cargo clippy --workspace --all-targets` | **0 警告** |
| `cargo fmt --all` | 无改动残留 |
| `lens_diagnostics(source=lsp)` 对改动文件 | 0 诊断 |

### 真实样本上的实测（`/mnt/c/src/tmp/pic`，301 文件 / 4.2 GB）

```text
scan-smoke:    目录 1 · 文件 300（158 JPG + 142 RW2）· 跳过 0
               配对：位图↔RAW 同名成对 142 组 · 只有 RAW 0 · 只有位图 16
refresh-smoke: 建库 16ms · 首轮 158 资产 / 300 文件 / 929ms
               次轮 变化 0 条 ✅ 幂等 · 离线资产 0
```

**「只有 RAW」为 0** 这一点很关键：它印证了本次不做 RAW 内嵌预览、
RAW 一律用占位缩略图的取舍在本用户的样本上**没有观感损失**（每张 RAW 都有同名 JPG）。

### 边界覆盖（单测，全部进常规 `cargo test`，不拖慢日常）

* 文件名：隐藏文件（`.gitignore` 不是「扩展名 gitignore」）、末尾点、多点、全角、200 字中文名、
  无扩展名、路径当名字传进来；
* 扫描：隐藏项、OS 垃圾（`Thumbs.db`/`desktop.ini`/`.DS_Store`/NTFS ADS）、编辑器临时文件、
  **自家 `catalog.db` 及其 `-wal`/`-shm`**、0 字节、深度上限、符号链接（含目录环风险）、
  无权限目录（折成 problem 继续）、取消中断、回调出错中止、Unicode/中文路径、确定性；
* 差分：空输入、改名/移动/跨目录、内容改过、替换、回归、早已缺失、启发式的四条前提逐一否证、
  1 万文件不劣化、位图与 RAW 作为两条记录共存、NFC/NFD、大小写改名、零身份；
* 落库：配对成一个资产、RAW 后补挂到既有资产、唯一索引防线、级联删除、缺失与回归、
  1000 文件一轮落库且第二轮幂等。

---

## 5. 遗留问题（交给后续步骤或后续波次）

1. **EXIF 抽取、缩略图、真实样本吞吐实测**——计划步骤 D/E/F，本次未做。
2. **RAW 内嵌预览**仍不做（用户批准），已登记 `FUTURE.md` B8，并写明届时必须一并完成
   独立 worker 进程、进程间协议、RAW-only 资产、与编辑模块共用解码后端。
3. **`asset_files` 的 `role` 目前只有 bitmap/raw**；侧车（`.xmp`）在扫描阶段被跳过，
   等写 XMP 时再决定是否入库。
4. **`store::assets::files_of_asset` 是全表读后过滤**（实现简单、量级够用）；
   真要优化时加一条 `WHERE asset_id = ?` 即可。
5. **身份读取（`FileId::try_read`）目前在调用侧每个文件读一次**。将来可优化成
   「大小与时间都没变就跳过身份读取」，等实测数据显示有必要再做。
6. 色标浅色套的黄/绿/蓝当色块只有 ~2:1，已在 `DESIGN.md` §1.4 定下「浅色下必须加
   1px 深描边」的纪律，**实现色标组件时要落实**。
