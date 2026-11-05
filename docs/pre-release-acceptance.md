# 对外推广前：功能核验清单（崔总执行）

> **这是给人做的验收清单，不是进度表**（进度只看 `todo`）。每做完一项就在本文件勾掉，
> 失败项按 §0.4 的格式回给 Agent。
>
> 建立：**2026-11-05**。此前散在各 `specs/` 的「真机验收」小节里，本文件把它们**汇总成一遍能走完的清单**；
> 每项后面标了**权威正文**——对不上时以那份规格为准，本文件跟着改。
>
> 与 `docs/release.md` 的分工：**那份管「怎么出包/怎么发」**（环境、密钥、命令、应急），
> **这份管「发之前功能对不对」**。发版的最后一步自检仍在 `docs/release.md` §3.2。

## 0. 准备

### 0.1 机器与程序

- Windows 10/11 一台，用 `pnpm debug:win` 出的 debug 包（`C:\rb-target\raybend\debug\raybend-desktop.exe`），
  同目录要有 `raybend-raw-worker.exe`。
- 有第二个显示器就接上（窗口还原与多 DPI 要看它）；缩放至少试 100% 与 125% 两档。

### 0.2 验收专用库（**不要**拿日常库做破坏性动作）

新建一个空库，照片源备 6–8 张，覆盖：

| 素材 | 为什么要 |
| --- | --- |
| RAW-only、JPG-only、RAW+同名 JPG | 三种变体、`_RAW/` 分流、导出 RAW 行 |
| 中文文件名、超长文件名 | 导入模版、侧车路径、`shortpath` |
| 一张手机拍的带 GPS 的 | GPS 只读展示 + 「按 GPS 填入地点」 |
| 一张竖拍、一张裁切过的 | 显示尺寸 / 方向 / 定稿输出尺寸 |
| 一张旁边预置**不含 `rb:`** 的 `.xmp` | 外来侧车接管（`.xmp.bak`） |

### 0.3 工具

- ExifTool（看侧车写得对不对）：https://exiftool.org/ ｜ `winget install --id=OliverBetz.ExifTool -e`
- XMP 文件层巡检：`pnpm check:xmp "<验收库根>"`（**关掉应用再跑**，只读）
- 导出产物检查：`pnpm check:export`（需另一终端 `pnpm dev`）

### 0.4 结果怎么记

失败项回给 Agent 时写全四条：**区域 + 步骤 + 期望 + 实际**（有日志就带时间点，日志在 §11.1）。
「说不上来哪不对」也算一条——视觉/手感类问题直接描述，别替 Agent 猜原因。

---

## 1. 库与存储

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 1.1 | 新建库、再「添加已有库」指向同一个库根 | 识别为**同一个库**（按 catalog 内 ID 比对，不按路径）；不出现两份登记 | `memory/FUNCTION-REPOSITORY.md` |
| 1.2 | 老库升级（拿一份旧版本建的库） | 升级前自动快照、逐条事务；升级后照片/评级/编辑栈都在；旧程序打开新库被拒绝 | `specs/display-size-source.md`、`specs/database-upgrade-gate.md` |
| 1.3 | 改盘符后「定位此库」 | 核对 ID 后接上；选错库被拒绝，原登记不被改写 | `docs/removable-storage-acceptance.md` |
| 1.4 | 拔掉库所在移动盘 → 库设置 / 浏览 | 库变离线但仍可打开设置；释放与重连按规格，不自动接回 | 同上 |
| 1.5 | 「释放此库」再重新连接 | 释放期间拒绝新任务；重连后照片与登记都在 | 同上 |

## 2. 导入

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 2.1 | 从 SD 卡 / 目录导入一批（含 RAW+JPG、中文名） | 目录树、勾选、最近目录都正常；导入进度可见、可取消 | `memory/FUNCTION-REPOSITORY.md` |
| 2.2 | 看导入后的目录结构 | RAW 进同名照片旁的 `_RAW/`；命名按导入模版；序号 `:SEQ` 连续、不重名 | 同上 |
| 2.3 | 同一批再导一次，开着「避免重复导入」 | 已入库的不重复导入；关掉该开关后按冲突规则处理，不静默覆盖 | `specs/import-dedup-fingerprint.md` |
| 2.4 | 导入时的体感（小图出现速度、滚动） | 小图**最快出现**、滚动不卡；不因「优化」越用越慢 | `AGENTS.md` §2.24、`specs/thumbnail-tiers-and-import-memory.md` |
| 2.5 | 导入**带自家 `.xmp`** 的照片 | 编辑/定稿/评级/标签/地点被采纳读回（采纳是只读，不会立刻补写新 xmp） | `specs/xmp-sidecar.md` §10 C1 |

## 3. 浏览与看图

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 3.1 | tiles 网格 / 列表切换、缩放档、按时间分组 | 分组按「同一天内间隔 > 1h 断片」；比例随显示变体 | `memory/FUNCTION-BROWSE.md`、`memory/DESIGN.md` §12.7 |
| 3.2 | 单击 / `Ctrl`+单击 / `Shift`+`Ctrl`+单击 | 选中唯一、勾选独立；反转跟随数据源；`Shift+Ctrl` 回退到当前场景单击（不是新操作） | `memory/FUNCTION-BROWSE.md` §5.2.4 |
| 3.3 | view 里缩放、平移、适应窗口、100%、改窗口大小 | **只有一次载入**；上述动作**不重新出图、不弹载入提示**；100% 是 1:1 | `AGENTS.md` §2.25/§2.26 |
| 3.4 | 切 SOOC / RAW / 各条定稿 | flowinfo 的尺寸与 MP、view 画幅、tiles 比例**一起**变，且毫秒级 | `specs/display-size-source.md` |
| 3.5 | 全屏（`F11`）、compare（最多 4 张） | 全屏是独立窗口、无 UI；compare 超过 4 张时提示只比最近 4 张 | `memory/FUNCTION-BROWSE.md` |
| 3.6 | 胶片带：尺寸拖拽、滚轮、`Ctrl`+滚轮 | 只有一套尺寸组件；偏好按工作流分开记；切换照片不重建 | `AGENTS.md` §11.4 |

## 4. 标记与元数据

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 4.1 | 星级 / 色标 / 旗标 / 赞踩，单选与多选 | 立即生效；`Ctrl+Z` 撤销；tiles 与右栏同步 | `memory/FUNCTION-BROWSE.md` |
| 4.2 | 照片标签与**目录标签** | 两者独立：目录标签**绝不**传播成照片标签，也不进自动桶 | `specs/photo-organization.md` |
| 4.3 | 作者 / 描述 / 国家 / 省州 / 城市 / 具体地点 | 点值 → 行内输入（focus 直接可打字）+ 建议列表（最近 20 ∪ 库内已有值）；Esc 先关列表；中文输入法回车不误提交 | `specs/browse-info-fields.md` |
| 4.4 | 多选时改一个字段 | 只改锚点那张；行尾「应用到所选 N 张」才批量写，落库后 Toast | 同上 |
| 4.5 | GPS 一项 | **有坐标才显示**（只读 + 复制 + 地图入口）；无坐标**整行不出现**；有坐标且地点有空缺时给「按 GPS 填入地点」，只填空、一步撤销 | 同上 + `specs/browse-info-fields.md` §3.5 |
| 4.6 | 字段超长 / 撞上限 | 文字字段 2000 字截断；标签 5000、层数 32、LUT 8、定稿 32 到顶时给明确反馈而不是静默失败 | `memory/LIMITS.md` |

## 5. 相片整理：标签 / 桶 / AI

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 5.1 | 左栏三个入口（库目录 / 桶 / 标签） | 三选一、不能全关；切换面板不丢当前来源 | `specs/photo-organization.md` |
| 5.2 | 标签：增删、去重、词典搜索、标签找照片 | 只筛出直接标注的照片；不影响目录导航 | 同上 |
| 5.3 | 手动桶：新建 / 改名 / 锁定置顶 / 拖入 / 移出 / 选中或旗标加入 | 成员幂等；跨库照片不串（同 `asset_id` 不同库不许混） | `specs/photo-organization-phase1.md` |
| 5.4 | 自动桶：多组 OR、组内 AND、从筛选复制条件 | 组数 ≤16、每组值 ≤64；**目录范围不进入规则**；条件不允许静默丢弃 | 同上 |
| 5.5 | 自动桶：暂停 / 恢复 / 重启应用 / 移出排除 | 初始补扫与增量收片对；排除在重放中持续生效；被 AI 装错桶的不会自动移出 | `specs/photo-organization-phase2.md` |
| 5.6 | AI 打标（装模型后） | 标签有来源区分（手动 / AI）；纠错与禁止生效；筛选、导出、自动桶读同一套有效标签 | 同上、`docs/ai/tinyclip-v1/README.md` |

## 6. 编辑

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 6.1 | 常规调整 + 自动调整 + 曲线 | 画面即时变化；自动调整只作用于 `默认` 层 | `specs/editor-area.md` |
| 6.2 | 编辑区：新建全图层 / 控制点、改名、上下移、删除、眼睛 | 列表自上而下 = 计算顺序、新层追加末尾；`默认` 层不可删；层数 32 到顶两个「新建」变灰 | `specs/editor-area.md` §10、`memory/LIMITS.md` |
| 6.3 | 控制点：拖动范围 handle、重新取样 | 种子在创建时冻结；引线按可用空间翻边；不显示影响面（有意为之） | `specs/editor-area.md` |
| 6.4 | LUT 链：加多层、改强度、开关、拖动排序 | 拖动排序手感与其它行一致；顺序改变画面就变；8 层到顶「加入」禁用 | `specs/editor-lut-chain.md` |
| 6.5 | 预设：新建目录/预设、应用、拖拽、LUT 丢失 | 应用后选中态与配置哈希一致；丢 LUT 时不动当前值并提示 | `specs/editor-presets.md` |
| 6.6 | 定稿：命名、再看「最近编辑」、上限 | 同配置不重复；32 个到顶按钮变灰并说明；每条定稿有自己的预览 | `memory/LIMITS.md`、`specs/issue-xmp-contract.md` |
| 6.7 | 撤销：拖动 / 松手 / 归位 / 重置 / 开关 / 应用 / 确认 | **只有 confirm 进撤销栈**；拖动途中不落库、不建快照；任何操作节点都能撤销一步 | `AGENTS.md` §2.21 |
| 6.8 | 几何：裁切、旋转、方向 | 首帧画幅正确、真帧到达不跳变；输出尺寸随裁剪 | `specs/editor-session-pipeline.md` §3.5 |
| 6.9 | 色彩：指定色彩状态、显示状态提示 | 显示状态与警告正确；关闭后恢复；照片身份（哈希/侧车）不变 | `specs/cm-w5.md`、`specs/color-management.md` |

## 7. 导出

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 7.1 | 进导出、选照片、选/换预设、筛选 | 四列布局与筛选可用；tile 小片与 RAW 按钮点得动 | `specs/toolsbar-compact-and-export-gallery.md` |
| 7.2 | 同一张照片多条定稿一起导出 | 文件名各带尾号、互不冲突（`IXX`/`ISO`/`IRA`/`ILA`） | `specs/export-issue-ordinal.md` |
| 7.3 | RAW 行导出 | 走 `IRA`；输出尺寸与色彩标记符合设置 | 同上 |
| 7.4 | 导出队列：暂停 / 取消 / 拔盘 / 重启 | 等待项让出执行槽、恢复后续跑；不谎报「重启后恢复未持久化的队列」 | `docs/removable-storage-acceptance.md` |
| 7.5 | 交外部编辑器（PS 等） | 交接文件与回读正常；RAW/JPG 都对 | `specs/M4-export-refinement.md` |

## 8. 删除与回收站

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 8.1 | `Delete` 键 / 工具条删除 | 弹确认；确认后进**库内回收站**（不是系统回收站） | `specs/library-recycle-bin.md` |
| 8.2 | 拖到左上角垃圾桶松手 | **不弹确认**、直接进回收站，Toast 反馈 | 同上 |
| 8.3 | 回收站视图 | 入口只在有内容时出现、带数量；低浓辅色底；固定按删除时间倒序；右栏显示「原位置」 | 同上 |
| 8.4 | 还原 | 回原位（目录缺了就重建）；同名自动 `_01` 并提示 | 同上 |
| 8.5 | 真删（**只有拖动**）/ 清空回收站 | 真删无确认；清空强确认；清完磁盘文件、sidecar、`cache/full/<id>`、DB、FTS 全干净 | 同上 |

## 9. XMP 侧车（**重点**）

> 权威正文 `specs/xmp-sidecar.md` §10。下面是「最小核验」+ 必过的硬项，A–E 全表留作出问题时的抽查依据。
> 全部都在**验收专用库**上做；动手前先关掉应用再跑 `pnpm check:xmp`。

| # | 做什么 | 应当看到 | 对应 |
| --- | --- | --- | --- |
| 9.1 | 关应用 → `pnpm check:xmp "<库根>"` | 路径规则 / 孤儿 / 哈希 / 元数据 / `crs:` 全部通过 | 巡检 |
| 9.2 | 编辑一张 RAW（曝光 + 曲线）→ 确认 | 库内出现同名 `.xmp`，含 `rb:latest` 与 `crs:Exposure2012`；**拖动途中文件不变** | A2 |
| 9.3 | 打 4★ + 一个标签 + 作者/说明/地点 | `xmp:Rating` / `dc:subject` / `dc:creator` / `dc:description` / `photoshop:City,State` / `Iptc4xmpCore:Location` 与界面一致 | A4 |
| 9.4 | 定稿命名 | `rb:profiles` 多一条；同配置再定稿不重复 | A3 |
| 9.5 | latest 切 SOOC | `crs:` 整段消失、`rb:` 与元数据仍在；切回 RAW 后回来 | A5 |
| 9.6 | 重置全部编辑 + 清空所有标记 | `.xmp` 被删掉，不留空壳 | A6 |
| 9.7 | `exiftool -G1 -a -s <照片或 .xmp>` | 逐项对得上界面值 | A7 |
| 9.8 | 删除该照片 | 回收站里照片与 `.xmp` 同进同出 | A8 |
| 9.9 | 手放一个无 `rb:` 的 `.xmp` → 编辑；再手改版本号破坏它 → 编辑 | 前者原文件变 `.xmp.bak`；后者**不被覆盖/删除**，编辑照常成功 | B1、B2 |
| 9.10 | 大写 `.XMP`、中文/emoji/超长文本 | 复用同一文件、不产生第二份；文本读回无误 | B3 |
| 9.11 | 200+ 张批量评级 / 色标 | UI 不卡、文件齐、日志无失败；中途杀进程不伤库 | B4 |
| 9.12 | 拷走 `photos/`（带 `.xmp`）→ 新库重建 | 编辑/定稿/评级/色标/标签/说明/地点全回来 | C1 |
| 9.13 | 设置里关掉「写入 XMP 文件」→ 编辑/标记一批 → 再打开 → 编辑一张 | 关闭期间不产生新 `.xmp`（也不删已有）；重开后新编辑的那张有，关期间的不补写 | E1–E3 |
| 9.14 | Bridge / Lightroom（有就做） | 评级/色标/关键词/作者/说明/地点可读；`rb:` 未知命名空间属预期 | C2 |

## 10. 外壳、命令与窗口

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 10.1 | 窗口还原：单屏 / 双屏不同 DPI / 改分辨率 / 拔副屏 / 最大化、最小化关闭 | 各自回原位与原尺寸（±2px）；原屏不可识别回主屏留白；任何部分不出屏 | `specs/window-restore.md` |
| 10.2 | 双击图标再开一个 | 第二个实例退出、已有窗口被聚焦 | 同上 |
| 10.3 | `Ctrl+K` 命令面板、快捷键设置 | 该有的命令都在；场景列可搜；真冲突拦保存，跨场景同键放行 | `specs/command-scenes.md` |
| 10.4 | 密度两档 / 主题 / 标题栏菜单 | 间距变、字号与图标不变；菜单是逃生通道，不是功能索引 | `memory/DESIGN.md`、`AGENTS.md` §11.5 |

## 11. 诊断、更新与首启

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 11.1 | 看日志位置与会话头 | `<LOCALAPPDATA>\com.cthun.raybend\logs\` 有本次会话文件，可读、无敏感原串 | `specs/logging-and-crash-report.md` |
| 11.2 | 人为崩溃一次（dev 构建）/ 结束 WebView2 子进程 | 崩溃记录含位置与 backtrace（或 dump + 重启后提示条）；渲染进程崩溃能自动重载 | 同上 |
| 11.3 | 导出支持报告包 | zip 结构、`manifest.json`、`summary.md` 可读；抽查 3 份看不到真实路径/姓名/库名 | 同上 |
| 11.4 | 检查更新 → 下载 → 安装 → 重启 | 提示版本、进度、成功/失败可分辨；断网与中断不弄坏已装版本 | `specs/update-flow-v2.md` |
| 11.5 | 首次启动（干净配置） | 跟随系统语言与主题；splash 正常；不弹无关提示 | `specs/first-start-system-preferences.md`、`specs/splash-language.md` |

## 12. 规模与性能底线

| # | 做什么 | 应当看到 | 权威正文 |
| --- | --- | --- | --- |
| 12.1 | 大库滚动（尽量接近 15 万张量级，或至少几万） | tiles 滚动不卡；384 小图缓存不因容量抖动 | `AGENTS.md` §6.5、`memory/LIMITS.md` §2 |
| 12.2 | 连续看图几十张 | 内存不持续上涨（库外真身只留 10 张）；切图不重复解码同一张 | `AGENTS.md` §2.24 |
| 12.3 | 编辑时的响应 | 参数拖动跟手、确认后落库；缓存目录不因频繁编辑爆量（真身/占位各留最新 2 份） | `AGENTS.md` §6.5 |

## 13. 发版前最后一道（环境层）

- [ ] `docs/release.md` §3.2 三条自检全过（dry-run 计划、私钥可读、`gh auth` + 工作树干净）。
- [ ] 本文件 §1–§12 的必过项已勾完；未过项已修复并复验。
- [ ] `RELEASE-NOTES.md` 里的「本版变更」与用户实际能感知的变化一致（源在 `docs/release-notes/<版本>.md`）。
- [ ] 官网/README 的功能描述与本版实际能力一致（不写还没做的）。

---

## 附：这份清单的出处

| 区域 | 权威正文 |
| --- | --- |
| 库与导入 | `memory/FUNCTION-REPOSITORY.md`、`specs/thumbnail-tiers-and-import-memory.md`、`docs/removable-storage-acceptance.md` |
| 浏览与看图 | `memory/FUNCTION-BROWSE.md`、`specs/display-size-source.md`、`AGENTS.md` §2.25/§2.26 |
| 元数据 | `specs/browse-info-fields.md` |
| 相片整理 | `specs/photo-organization.md`、`-phase1.md`、`-phase2.md`、`docs/ai/tinyclip-v1/README.md` |
| 编辑 | `specs/editor-area.md`、`specs/editor-lut-chain.md`、`specs/editor-presets.md`、`specs/editor-session-pipeline.md`、`specs/color-management.md` |
| 导出 | `specs/toolsbar-compact-and-export-gallery.md`、`specs/export-issue-ordinal.md`、`specs/M4-export-refinement.md` |
| 回收站 | `specs/library-recycle-bin.md` |
| XMP | `specs/xmp-sidecar.md` §10、`specs/xmp-audit-tool.md`、`specs/issue-xmp-contract.md` |
| 外壳/窗口 | `specs/window-restore.md`、`specs/command-scenes.md`、`memory/DESIGN.md` |
| 诊断/更新 | `specs/logging-and-crash-report.md`、`specs/update-flow-v2.md`、`specs/first-start-system-preferences.md` |
| 上限 | `memory/LIMITS.md` |
| 出包与发布 | `docs/release.md` |
