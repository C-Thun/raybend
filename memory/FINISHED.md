# raybend — 已完成部分（memory/FINISHED.md）

> **定位**：已完成工作单元的**摘要与证据指针**——每个 milestone 一节，只留结论与去向，
> 不重复规格正文（§2.12 单一事实源纪律）。
> **近期与未完成的排期在 `memory/PLAN.md`**；远期方向在 `memory/FUTURE.md`。
>
> 状态口径：✅ = 人类验收 / 正式收口；◐ = 开发与 Agent 冒烟收口，
> 真机 / E2E 验收仍归人类（`AGENTS.md` §2.8 测试分工）。
>
> 建立时间：2026-09-27（由 memory/PLAN.md 拆出，完成内容归档于此）。

---

## 0. 怎么读这个文件

- 波次的**规格**在 `specs/<单元>.md`（开工前的详细方案与验收标准）；
  **实施证据**在 `implementations/`（每次改动一份记录）。本文件只做里程碑级摘要。
- 附录 A / B 是历史路线归档（旧规划原文），仅供参考，**不是待办清单**。
- 「人类验收 / 真机验收」的结论以人类在对话中的确认为准；Agent 不得代填。

---

## 1. 路线演变（为什么现在是这个形状）

| 时点 | 变化 |
| --- | --- |
| 2026-09-15 | 初版 7 个 milestone；M0 从「一次性门禁」改为「按模块按需前置」（原规划归档为附录 A） |
| 2026-09-17 | 官网支线落地（不在 milestone 序列内，见 §9）；M2 波次 6→3（按功能切，每波结束人能看到东西） |
| 2026-09-20 | M2 后路线重排：先「一致性 + 导出」闭环（后被下一条取代） |
| 2026-09-21 | **编辑提前（人类定案）**：M3 = GPU 显影工作台；原 M3（一致性+导出）顺移 M4；组织检索 / 多仓 / 缓存治理移入 `memory/FUTURE.md` I/J 节；原 M6 → M5「四大模块完成即发版先用」 |
| 2026-09-24 | M3 波次再切：CUBE LUT + issue 与「编辑数据结构重整」合并为 W6；外部编辑器（纯单向）定案进 M4 |
| 2026-09-26 | M4 重排为 W0–W7（原四波路线归档为附录 B）；M5 与 M4 并行授权；发版后三条主线定优先级 |
| 2026-09-27 | M4 收口；发行方向拍板（`memory/REVIEW.md` R3-01：unsigned 直下 → 后续 MSIX） |

逐条决策原文见 `memory/PLAN.md` §6 决策记录。

---

## 2. M0 —— 可行性验证（按模块前置）✅

**目标**：用最小代价把「会推翻架构的未知」打掉。

| 单元 | 内容 | 状态 |
| --- | --- | --- |
| M0-1 | 仓库基线与 Windows 构建路径 | ✅ 2026-09-15 |
| M0-4 | 存储基准（SQLite / FTS5 中文 / 并发 / 备份） | ✅ 分批并入 M1-2 / M1-7 / M2-W1：单写者、FTS、备份恢复、10 万条查询均有实测 |
| M0-5 | 缩略图吞吐（编码格式 / 并发度 / 存储形态） | ✅ 并入 M1-3：真实样本吞吐、两段式缩放、SQLite 缓存与命中基线落地 |
| M0-6 | 前端骨架与虚拟化 | ✅ 网格产品化；真机裁决由 M2-W3 完成双屏采样（120fps，p50 8.3ms） |
| M0-3 / M0-3b | RAW 解码：内嵌预览路径 / 完整解码路径 | ✅ 2026-09-17 并入 M2-W1 完成 |
| M0-2 | 渲染可行性（wgpu 透明挖洞）★最高风险 | ✅ **2026-09-19 真机验收通过**：透明挖洞成立、1:1 锐利、命中中心 3000±1、数学往返 ≤4e-4 px；两档 DPI 成立（125% → dpr 1.375；200% → 2.200）；性能 CPU 0.41ms/帧、手操 p50 16.42ms。设备恢复闭环同日复跑通过；2026-09-25 验证 DX12 + DirectComposition 并固化为 Windows 编辑器默认。坐标契约见 `memory/ARCHITECTURE.md` §7.9；全程证据 `implementations/2026-09-19_windows-spike-verified-and-device-loss.md` |
| M0-7 | 架构定稿与 crate 边界 | ✅ `raybend` 核心库 + 薄 Tauri 壳成立；暂不拆空 crate，IPC 契约 DTO 双侧守卫 |

**历史取舍**：M0-2 当时可以后移，是因为导入模块不碰 GPU 直绘、M2 看图先走 DOM；
它最终服务于编辑视口（M3-W2 产品化）。

---

## 3. 设计稿现状（design/，截至 2026-09-27）

| 产出 | 内容 | 状态 |
| --- | --- | --- |
| `memory/DESIGN.md` | 令牌体系、四级面、双档密度、组件化策略、i18n、通用交互范式 | ✅ 已定（活文档持续演进） |
| `design/main.pen` + `.md` | 外壳、基础组件、导入工作区 | ✅ 已落地并用于 M1 |
| `design/browse.pen` + `.md` | 浏览 / 看图 / 对比 / 标签 / 命令面板 / 菜单 / 快捷键 / 外部编辑弹窗 | ✅ 用于 M2；2026-09-20/21 反向同步；外部编辑弹窗并入（M4-W6） |
| `design/editor.pen` + `.md` | 编辑工作区：视口、调整面板、裁剪、对比、LUT、issue | ✅ 主稿 + 8 帧已落（2026-09-23 定案）；后续微调持续进稿 |
| `design/export.pen` + `.md` | 导出工作区：画廊选定稿、预设、队列 | ✅ 2026-09-27 按连续验收定案同步保存 |

**硬规矩**（`AGENTS.md` §5.1 / `memory/PLAN.md` §0）：任何模块涉及未设计或未定案的界面，
必须先过「界面设计环节」（Pencil 落稿 → 人类定案）才写界面代码。

---

## 4. M1 —— 相片仓、目录与导入 ✅ 2026-09-16（Agent 侧证据齐；验收由人类清单）

**目标**：可信的数据底座，完成「建仓 → 导入 → 缩略图可见 → 重启仍在」最小闭环。

| 波次 | 交付 |
| --- | --- |
| W1 相片仓与全局库 | 仓生命周期（创建 / 登记 / 改名 / 移除 / 只读卷）+ `app.db` 表设计；库身份最终定案为 catalog.db 内唯一 ID + app.db 多路径登记（见 `memory/FUNCTION-REPOSITORY.md`） |
| W2 catalog schema 与迁移框架 | 表族定稿；文件身份主键（volume_serial + file_id128）+ NFC/原始名/大小写折叠；版本闸门 + `VACUUM INTO` 快照（保留 7 份）+ 单写者 actor |
| W3 扫描与变更追踪 | 并行可中断扫描、按目录批量读、增量差分（不丢编辑）、缺失/离线标记 |
| W4 元数据与缩略图管线 | EXIF 抽取、内嵌预览优先出图、缩略图 worker + SQLite BLOB 缓存、持久化队列（崩溃可续） |
| W5 导入 UI | 三列工作区、导入面板（来源/模版/重名/重复检测）、进度/暂停/取消/错误清单 |
| W6 收尾 ✅ | 崩溃恢复演练（600 + 5000 张）、性能基线、迁移/备份/故意损坏演练 |

**DoD 证据（2026-09-16 全部达成）**：5000 张 11.27s 导入完、重导全部判重跳过；
缩略图 18.9 张/s（P95 62.8ms）、缓存命中 0.09ms/张、5 万张窗口计算 72.6µs/次；
`pnpm crash:drill` / `pnpm migrate:drill`（v1→v3、7 份轮转、损坏拒绝打开、快照恢复）。
→ `implementations/2026-09-16_M1-7_*.md`（scale-5000 / perf-baseline / crash-recovery / migration-backup-drill）

**人类验收清单**（M1 收尾执行）：设计系统目视、外壳 8 项、真实照片缩略图与信息条、
排除链路、tile 倒角与 easy copy、语言切换、品牌 logo、启动闪屏（Windows webview 透明首次真机验证）、
端到端手感（建库 → 导入 → 出图 → 改模版 → 崩溃续传）。逐项走法的历史原文见
`git log` 中 2026-09-16 前后的 PLAN 版本；对应实现证据在 `implementations/2026-09-15~16_*.md`。

---

## 5. M2 —— 浏览与评估 ✅ 2026-09-21 人类验收通过

**目标**：把「看片、挑片、打分」做到比 ON1 更顺滑——第一战场。

| 波次 | 交付 |
| --- | --- |
| W1 地基 + 浏览网格 ✅（人类 2026-09-19 签字） | RAW 真解码（rawler + worker 进程隔离 + 内嵌预览优先）；catalog 查询引擎（10 万条 P95 < 60ms）；标记写入 + 撤销栈 + 旗标 + 回收站删除；IPC 11 条契约；浏览三列接线。渲染 spike 人类逐项验证同日完成 |
| W2 看图 / 对比 / 标记 / 筛选 ✅ 2026-09-20 | 看图（DOM 路线）、胶片带、2–4 图对比（比例同步）；`toolsbar` 全量（三态标记 + 标签弹窗 + 筛选）；撤销/重做 UI、toast、中央模态；右栏看图态（预览 + 直方图） |
| W3 命令面板 / 快捷键 / 收尾 ✅ 2026-09-21 | 命令面板（模糊搜索 + 快捷键 + 最近使用）、快捷键自定义/导入导出/冲突检测、菜单极简兜底；命令/菜单/快捷键同源一份注册表 |

**DoD（全部达成，真机双屏实测）**：10 万张筛选 < 100ms——Rust P95 28.29ms、真机端到端 20–22ms；
滚动稳定 60fps——实测 **120fps**（p50 8.3ms，2560×1441@1.375 与 3073×1826@2.2 双屏）；
全键盘评片流人类目视验收通过。
→ `specs/M2.md`、`specs/M2-W2.md`、`specs/M2-W3.md`；`implementations/2026-09-17~21_m2-*.md`

**M2 → M3 闸门**：2026-09-21 正式关门；路线重排为 M3 = 编辑（GPU 显影工作台）。

---

## 6. M3 —— 编辑：GPU 显影工作台 ◐ 2026-09-26 开发与冒烟收口（真机验收待人类）

**目标**：差异化核心——同一界面对 RAW/JPG 做无损编辑（只写编辑栈，原始文件零改写），
issue / SOOC 从概念变可用能力。

| 波次 | 交付 |
| --- | --- |
| W0 编辑工作区界面设计 ✅ 2026-09-23 | `design/editor.pen`（10 帧）+ 同名 `.md`；连带定案 `memory/DESIGN.md` §14.8–14.10、toolsbar 三段式术语、「取消在左 / 确认在右」 |
| W1 骨架与洞口契约 | ToolsBar 三段式、Tab 三档、左 LUT 面板、右三组页签、视口洞口契约（DOM 只上报原始事实）、空态四态、命令接入 |
| W2 GPU 编辑视口产品化 ✅ 2026-09-23 | 透明 WebView + 原生 wgpu；Windows 默认 DX12 + DirectComposition visual + Opaque；渲染线程捕获 panic + 重启；跨屏/DPR 用带 revision 的整包布局事务；真实照片进纹理 |
| W3 编辑栈与常规调整管线 | issue 落库（走迁移框架）、scene-referred 显影管线（曝光/反差/色温/曲线…）、拖动实时预览、编辑后缩略图失效语义；XMP 契约 `specs/issue-xmp-contract.md` |
| W4 清晰度 / 镜头 | 降噪 / 锐化；镜头校正（全库品牌/焦段搜索、畸变/暗角/色差）；编辑内对比数据侧 |
| W5 裁切 / 旋转 / 对比三工具 | 三工具互斥、命中测试全在 Rust；裁切把手/比例、旋转内接框 + 拉水平线、对分线对比 |
| W6a 可选基础曲线与机型档案 | 显示域可选曲线（EXIF 机型齐才提供）；`app.db` 按机型存档案、编辑栈存快照 |
| W6b 显式自动调整与档案学习 | 显式「自动调整」按钮；缩略级 RAW ↔ 直出位图分位数拟合；阈值内沿用档案 |
| W6c CUBE LUT + issue + 数据结构重整 | `.cube`（1D/3D/shaper）+ HaldCLUT 两种格式；按目录导入（≤3 层）、封面一次性烘焙、LUT 本体复制自包含；**issue 数据结构重整**：变更集 + 缓存快照 + `schema_version` + 旧数据迁移一致 |
| W7 收口 ✅（开发冒烟） | 性能基线与文档同步；数字见 `specs/M3-W7.md` |

**DoD（草案 → Agent 侧证据齐；真机裁决项）**：无损编辑 + 原文件零改写（校验和可验）；
亮度/色彩/曲线/降噪/锐化/镜头/裁剪可用；LUT 导入/应用/移除；SOOC 与 issue 定稿、浏览切换、编辑内对比；
编辑后缩略图正确反映。**真实照片、两档 DPI、交互帧率、颜色由人类真机验收**（列入 `memory/PLAN.md` §2）。
→ `specs/M3-W1.md`…`M3-W7.md`；`implementations/2026-09-23~26_m3-*.md`（m3-w2-gpu-viewport、m4 前 M3 各波）

---

## 7. M4 —— 可试用闭环：实时一致性 + 导出 ◐ 2026-09-27 开发与冒烟收口（真机验收待人类）

**目标**：磁盘在程序外变化时不说假话；SOOC / latest / 命名 issue 可靠导出成文件，
复用同一出图入口送外部编辑器。四大模块在此闭合。

| 波次 | 交付 |
| --- | --- |
| W0 设计与口径确认 | `design/export.pen` 六帧核对；行为口径表定案（见下）；外壳/右列尺寸校正（36/48/36、276/288） |
| W1 库与磁盘实时一致性 | 范围契约（进/展开/重读只扫相关目录）；完整扫描/权限失败/离线区分，不误标缺失；同卷移动/改名保 asset 与 issue；变更统一刷新 FTS/缓存/计数；notify 8.2.0 有界监听攒批 |
| W2 导出工作区、issue 画廊与预设 | 真实三列导出工作区（左列复用 browse、双 tiles 各带 statusbar、偏好独立持久化）；**唯一 PhotoGrid/行模型/Tile 上**做 issue 扩展；预设校验与版本化设备设置；**手势反转正式化**（`memory/FUNCTION-BROWSE.md` §5.2.4） |
| W3 统一全尺寸出图与 16-bit TIFF | 共享「解码 → profile → 成片几何 → 缩放」入口；浮点/16 位保持到编码；TIFF = RGB16 无损成片；profile/源按入队快照执行 |
| W4 五格式编码、命名与元数据 | 普通导出 WebP/AVIF/JPG/PNG（TIFF 专供外部编辑）；质量真实生效；模板子目录 + 不覆盖发布；EXIF/IPTC/版权元数据回读矩阵（中文字段实测）；预设文件交换暂不启用（2026-09-26 定） |
| W5 内存级多预设队列与后台运行 | Rust 权威内存队列（最多四预设四执行项）；启停/重置隔离；失败重试、执行前哈希失效跳过；退出丢弃队列（不持久化）；flowbar 仅实际 running 闪烁 |
| W6 外部编辑器：纯单向输出 | browse `toolsbar right` 入口；定稿下拉（不写 latest）；16-bit TIFF 复用 W3/W4；Windows ShellExecuteW 平台 adapter + 九候选发现；失效报错并自动清登记 |
| W7 整条闭环收口 | 1 张 + 1000 张小合成批量冒烟；故障/并发/原图哈希覆盖；迁移演练；Windows 产物核对；共享界面回归（节点身份 + fit context 两面） |

**已确认行为口径（2026-09-26 崔总指示开工，完整表见 `specs/M4-finalization.md`）**：
取消选中 = Esc（复用 `edit.clearSelection`）；选择 = 单击切换 / Ctrl 单选替换 / Shift 共用区间；
三态筛选（仅定稿 / 编辑过 / 全部）；方形 latest 主图 + 小图两列最多六张；
共享 TilesShell 配置边界（上 240–320、下 128–320）；入队快照（定稿哈希/源签名/profile/预设参数）；
最多四预设开启、队列全内存级、重置不打断在途出图；极简预设操作（名称唯一决定新建/变更）；
普通导出 WebP/AVIF/JPG/PNG 顺序、TIFF 专供外部编辑。

**范围控制**：不自行采纳 `memory/REVIEW.md` 未批准的 ICC、集合/智能集合、XMP 写回、旗标持久化扩围；
HEIC 解码仍是未完成导入能力，不因五格式导出标成完成。

**DoD（Agent 侧证据齐）**：一致性不误标、画廊按 issue 选择不写回 latest、四格式 + TIFF 按正确源出图、
队列语义可重复验证、1000 张资源有界、外部编辑多应用记忆、原文件零改写。
**真实照片、实际外部软件打开、DPI/色彩/性能由崔总真机验收**（列入 `memory/PLAN.md` §2）。
→ `specs/M4-W1…W4-W5、M4-export-*、M4-finalization.md`；`implementations/2026-09-26~27_m4-*.md`

---

## 8. M5 —— 分发与发布 ◐ 工程准备完成（发行操作与真机验收待人类）

**定位**（人类 2026-09-21 定）：四大模块完成即发版先用。品牌、闪屏、Windows 构建路径、
官网与 Pages 工作流已提前完成。

| 波次 | 工程已就绪（开发 + 冒烟） | 剩余（人类 / 外部输入） |
| --- | --- | --- |
| W1 安装包与首启 | 三处版本事务、Windows bundle 脚本、安装配置、首启与诊断隔离 | 真机首启/安装/卸载/权限；独立磁盘重载 |
| W2 自动更新 | 官方签名更新接线、稳定/预览/关闭/自定义源、任务保护 | 密钥生成、更新 JSON 部署、两版真机升级与失败恢复 |
| W3 文档、许可与支持 | 593 项离线许可、双语帮助/隐私、诊断摘要 | 真实 Release 后自动刷新下载；MIT 版权补足复核 |
| W4 签名与正式分发 | 有/无签名两路线、signtool 验证、SHA-256 与分安装器清单 | Win10/11 与 GitHub/Pages 实发（由崔总执行） |

**2026-09-27 直下发行链收口**：`pnpm release <升版> --windows ...` 自动生成完整发布目录；
`pnpm release:publish <目录> --execute` 由崔总执行提交/tag/推送/草稿上传/公开。
→ 规格 `specs/M5-distribution-closeout.md`，操作手册 `docs/release.md`。
**发行方向拍板（`memory/REVIEW.md` R3-01，2026-09-27）**：unsigned 直下首发
（GitHub Release + 官网 + 预安装说明 + SHA-256），不买证书；直下链走通后启动 MSIX 打通微软商店；
macOS 暂缓、Linux 空闲时先行（`memory/FUTURE.md` F1/F2）。

**2026-09-29 MSI 脚本推进（仅工程冒烟）**：参数拆为 `--win-msi` / `--win-nsis`，Windows 本地镜像、盘/挂载探测、路径转换、锁和旧包隔离已实现；官网优先 MSI。未实际生成安装器，M5 验收状态不变。证据 `implementations/2026-09-29_release-windows-msi.md`。

**2026-10-09 crates.io 支线收口**：核心库 `raybend` 已上 crates.io（`0.1.1`，2026-10-09T13:59:06Z，发布账号 `andares`；打包 236 文件 / 1.1 MiB；docs.rs 构建成功），
且**同步发布已接入发版流程**：正式版走 `pnpm release:publish <目录> --execute` 时会在 GitHub Release 公开之后自动同步核心库，`--no-crates` 跳过、beta/test 不发；重跑靠「索引 `cksum` vs 本地重打包 sha256」判定，一致跳过、不一致硬报错。
→ 规格 `specs/m5-crates-publish.md`，操作 `docs/release.md` §9，证据 `implementations/2026-10-09_crates-io-release-integration.md`。
（`memory/PLAN.md` §3 的 M5 DoD 不因此改变：仍待真机安装/升级与一次实发验证。）

**剩余验收与发行操作清单在 `memory/PLAN.md` §3**（M5 DoD 未勾选，不计作完成）。

---

## 9. 官网支线（提前完成，不在 milestone 序列内）

- 2026-09-17：`website/` 首页（双语 / 下载区 / 教程占位）+ `.github/workflows/website.yml`
  （push master / 正式版 release / 手动 → GitHub Pages）。
  方案 `specs/website-homepage.md`；约定 `website/AGENTS.md`；素材待办 `website/ASSETS.md`。
- 官网是**独立技术栈**（SolidStart 2.0 / Solid 2.0 / Tailwind v4 / Lucide，纯静态站），
  与应用本体互不隶属；细则见 `website/AGENTS.md`。

---

## 10. 可卸载存储 W1–W4 —— 开发与 Agent 冒烟收口 ◐ 2026-09-29

四波连续完成：统一 catalog 会话/状态、有界探测与永久齿轮；离线设置可定位换盘符后的同库位置，
正常未找到用中性反馈；卷变化更新来源树/最近/勾选及同路径图片区，保留选择意图。
导入/导出失联等待，恢复核对原实体，部分成功不重计；同 ID 多实体明确选择，活动任务不静默换根。
主动释放排干本库任务/watcher/数据库，保留登记与文件，自动观察不能解除释放屏障。

W1 修订稿和 W3/W4 补充 Pencil 稿均由崔总定案。全量 Rust 单元、前端 112 个文件级测试、质量门、
导入/浏览/导出合成冒烟通过；Windows debug 主程序/worker 成套构建、资源与协议校验通过。
规格 `specs/storage-recovery-w1.md` 至 `storage-recovery-w4.md`；证据分别为
`implementations/2026-09-29_storage-recovery-w1-completion.md`、`2026-09-29_storage-recovery-w2.md`、
`2026-09-29_storage-recovery-w3.md`、`2026-09-29_storage-recovery-w4.md`。

真机拔插、换盘符、句柄退场、DPI/观感和真实照片仍待崔总统一验收，步骤见
`docs/removable-storage-acceptance.md`。NAS 已有定位/状态/探测及文件操作/恢复接缝，实际协议未接。

---

## 11. 相片整理上期 —— 开发与 Agent 冒烟收口 ◐ 2026-09-30

库目录／相片桶／标签三入口已落地；照片与目录标签独立，手动桶及多组自动规则共用一套照片来源与规则语义。跨挂载库选择保留复合身份，进入导出后可将全部选中照片的主定稿送入同一预设队列；离线库留待重试。上期未引入 AI；下期当前交付见 §11.1。

规格 `specs/photo-organization.md`、`specs/photo-organization-phase1.md`；实现与验证证据见 `implementations/2026-09-30_photo-organization-phase1-implementation.md`、`implementations/2026-09-30_photo-organization-export-handoff.md`。Rust 单元、前端测试、静态检查、UI 与浏览／导出启动冒烟、Windows debug 产物核对通过。真实照片库、Windows 操作、DPI 和大库性能待崔总验收。

### 11.1 相片整理下期：TinyCLIP AI 标签 —— 代码与 Agent 冒烟收口 ◐ 2026-10-04

崔总指定 TinyCLIP 并接受偏向减少误标的折中。固定七类、逐类校准 precision 目标85% / F0.5 / 至少5个真阳性，人/海加入固定易混淆负描述；一次图像编码后共用评分器，允许零标签，不显示概率。原90%/60%门槛失败记录保留；人物漏标、海类误标/漏标及困难负例不足仍如实披露。

有效 manual/ai 来源、文字禁止/恢复/保留与撤销、统一消费方/累积自动桶/XMP、范围冻结/持久任务/暂停取消/默认暂停恢复、原始输入版本守卫与独立串行 CPU worker 已接线。已配置有效本地模型源时桌面启用 CPU，固定约152MiB图像编码器与约15MiB ORT DLL 随应用资源提供；未配置或资源不可用时构建基础版；设置中主动安装/可信离线导入，可修复损坏与卸载，保留标签与纠错。不要求 CUDA/Python/PyTorch，不自动扫描图库；在线下载留后续分发，SigLIP 2 留 FUTURE E1。

Rust 核心1437项、桌面108项、前端1203项测试及质量门通过；Windows debug 资源/可信摘要/RAW协议核对与完整桌面 CPU worker 两图编码、非法协议/缺原片拒绝、正常退出冒烟通过。真实图库、GUI/DPI、盘上 XMP 往返、并发性能体感及干净 Win10/11 安装仍待崔总验收；未生成安装器或发布。规格 `specs/photo-organization-phase2.md`，实测 `docs/ai/tinyclip-v1/README.md`，实施记录 `implementations/2026-10-04_photo-organization-ai-tinyclip-delivery.md`。

2026-10-04 续修：独立 model-registry/LFS 和可选 AI 构建已收口；本地源登记 Git ignored，普通构建不导出模型。Windows有→无→有切换、实际CPU导出与worker拒绝、默认无AI数据/XMP往返通过冒烟；当前debug已恢复有AI。记录 `implementations/2026-10-04_ai-model-registry-optional-build.md`，协议 `specs/ai-model-library-build.md`。GUI/真实库/安装器仍待崔总验收。

---

> 以下两个附录为**历史路线归档**（原文照搬，仅随全仓更新了文件路径引用），
> 2026-09-27 随本文件自 memory/PLAN.md 迁入。仅供参考，不是待办清单。

## 附录 A：M0 未开工波次的此前规划（归档，仅供参考）

> **为什么在这里**：2026-09-15 的第一次规划把整个 M0 一次性铺开（7 个波次 + 后续设计阶段），
> 实施中暴露两个问题：① 计划文件范围过大，勾选状态随变更失真；② 进度 tracker 挂着大量未开工条目。
> 按 §0.1 的规划纪律，那次规划的 **M0-1 部分已抽出为 `specs/M0-1.md`**，其余部分归档在此。
>
> **⚠️ 不要把下面当成待办清单直接开工。** 它们只记录当时的思路与检查项；
> 真正开工某个波次前，要按**当时的实际情况重新规划**（技术前提、依赖、优先级都可能已变）。
> 完整原稿：`git show cfb84a2:plans/M0.md`

### A.1　平台验证分工（仍然有效）

| 波次 | 在 WSL/Linux 做 | 必须在 Windows 复测 |
| --- | --- | --- |
| M0-2 渲染 | 代码与架构原型 | ✅ 2026-09-19 spike 真机闭环；当时只跑通 Vulkan。2026-09-25 进一步验证 DX12 + DirectComposition 并作为 Windows 产品默认；强制不可用后端时不回退仍属预期 |
| M0-3 RAW | 全部（算法与解码平台无关） | MSVC 编译 + 解码结果抽查 |
| M0-4 存储 | 全部（SQLite 平台无关） | 文件身份（`FileIdInfo`）与路径规范化 |
| M0-5 缩略图 | 全部 | 文件系统行为抽查 |
| M0-6 前端 | 组件开发与迭代 | ⏳ **帧率与内存数字**仍缺（需一个真实大库；2026-09-19 真机只做了主观确认） |

**前置事实**：WSLg 无硬件 GPU（软件渲染回退）→ GPU 相关结论只能在 Windows 取得。

### A.2　M0-2　渲染可行性 spike ★（最高风险）

代码落位：`crates/raybend/src/render/`（wgpu 上下文、视口变换、WGSL）+ `src-tauri/src/spike_viewport.rs`

**窗口策略**：不动主窗口。用 `WebviewWindowBuilder` 另开 `label = "spike-viewport"` 的调试窗口（`transparent: true`），即使透明挖洞彻底失败也不影响主窗口与设计工作。

当时的检查项：

- wgpu surface 能否挂到 Tauri 窗口（`raw-window-handle` 版本一致性 —— 最常见的集成失败点）
- 透明挖洞成立（无 alpha 预乘色偏、无闪烁）
- DPI 与缩放（100%/125%/150% 下 1 图像像素 ↔ 1 物理像素）
- 多显示器（DPR 不同屏间拖动）
- 窗口状态（resize / 最大化 / 全屏 / 最小化恢复）
- 坐标同步（快速缩放是否漂移 —— RapidRAW 踩过「差 1 像素就图跟不上鼠标」的坑）
- 命中测试（透明区是否吞鼠标事件；`pointer-events: none` / `set_ignore_cursor_events`）
- 后端回退与 device lost（`WGPU_BACKEND=dx12|vulkan|gl`；`device.destroy()` 模拟）
- 性能基线（6000×4000 纹理 + 平移缩放的帧时间）
- 结论：能做 / 不能做 / 规避手段；是否需启用退路（独立子窗口 / Tauri CEF）

### A.3　M0-3　RAW 解码 spike

代码落位：`crates/raybend/src/raw/`（`trait RawDecoder` + `RawlerBackend` + worker 进程入口）

- rawler 0.8.0 在 WSL 与 Windows MSVC 下均编译通过（其 `multiversion`（SIMD target-feature）/ `memmap2` / `jxl-oxide` 依赖是最易出平台问题处）
- 格式矩阵：CR3 / NEF / ARW / RAF / RW2 / ORF / DNG 各跑通
- 路径 A（内嵌预览提取）：耗时目标 <10ms/张、内存、拿到的是 JPEG 还是 RGB
- 路径 B（完整解码）：黑电平/白平衡/最简去马赛克 → 线性 f32 RGB；记录耗时与内存峰值
- worker 进程隔离：注入 `panic!` 与损坏文件，验证主进程不受影响（**release profile 是 `panic = "abort"`，必须用 release 各跑一次**）
- 决定 rawler 用上游还是 RapidRAW fork（先用上游）

### A.4　M0-4　存储基准 spike

代码落位：`crates/raybend/src/index/`（rusqlite 连接管理、单写者 actor、迁移骨架）

- 确认 `rusqlite` bundled SQLite 版本 **≥ 3.34**（`trigram` 分词器所需）
- 10 万行合成 catalog：批量插入耗时（每批 500 条 vs 单条）
- 查询基准：日期区间 / 机型 / 评级筛选；`ORDER BY` + 分页（**keyset vs OFFSET**）
- FTS5 trigram 中文实测：中文关键词 / 子串 / 中英混合 / 单字查询
- 跨仓查询：`ATTACH` 3 个库 + `UNION ALL` 延迟
- 并发压测：单写者 actor 下模拟「导入写 + 缩略图写 + 读查询」
- 备份演练：`VACUUM INTO` 耗时与体积、`integrity_check`、故意损坏后的恢复
- Windows 侧抽查：文件身份（`GetFileInformationByHandleEx(FileIdInfo)`）与路径规范化

### A.5　M0-5　缩略图吞吐 spike

代码落位：`crates/raybend/src/thumbnail/`

- 从 M0-3 样本生成 512px 缩略图，端到端计时（5000 张规模）
- 编码格式对比：**QOI / WebP(lossy) / JPEG** 的「编码耗时 / 体积 / 前端解码耗时（`createImageBitmap`）」
- 并发度扫描：物理核数 N、N-1、N/2 三档吞吐曲线
- 存储形态对比：**SQLite BLOB vs 两级分片文件** 的随机读延迟
- 决策输出：编码格式、默认并发度、5000 张导入端到端估算

### A.6　M0-6　前端骨架与虚拟化 spike

代码落位：`src/`（Solid + Tailwind）

- 手写最小虚拟化网格（固定单元格 + 可见窗口 + overscan），**不引入任何组件库**
- 10 万条合成数据：滚动帧率、内存占用、`scrollTop` 抖动处理
- 验证「占位 tile → 位图到位后替换」与不可见时取消解码请求
- 对照实验：`content-visibility` / `will-change` 的有无对帧率的影响
- 决策：**是否需要 canvas tile**（按 `memory/FUTURE.md` §C3 的触发条件；结论写回该文件）
- 产出可复用组件 `VirtualGrid`（M2 直接使用）
- **在 Windows（WebView2）上复测**帧率，以 Windows 数字为准

### A.7　M0-7　架构定稿与移交

- 依据 spike 结论敲定 crate 边界：是否现在拆出 `raybend-raw` / `raybend-cache` / `raybend-jobs`
- 敲定 `raybend-ipc`（specta）契约方式、错误模型、Rust↔TS 类型生成管线
- 回填 `AGENTS.md` 架构章节与版本基线（wgpu 版本）
- 更新 `memory/FUTURE.md`：spike 结论（CEF 退路、canvas tile 判定）置为已评估
- 产出**设计阶段输入清单**：窗口尺寸档位与密度、令牌初稿、视口交互范围、主界面信息量

### A.8　M0 剩余完成定义

- 六个 spike 全部有可复现命令 + 数据 + `implementations/` 记录
- 四个架构未知均有明确结论（渲染成立与否 / RAW 可用性 / 存储目标达成与否 / 缩略图编码与并发）

（已达成部分：WSL 与 Windows 两侧均能起窗口、`check`/`clippy` 全绿；许可/中文名/工具链纪律/去个人化已落实；无模板残留与未使用依赖。）

### A.9　UI 设计阶段（M0 之后、M1 之前）

界面截图 + 逐块口述 → 用 Pencil 在 `design/` 绘制 `.pen` + 同名 `.md` → 同时定案组件库与图标库（已定，见 A.10）。

### A.10　UI 层选型信息（2026-09-15 实测）

**组件层 —— 已定 Ark UI：**

| 维度 | Kobalte | Ark UI |
| --- | --- | --- |
| npm | `@kobalte/core` 0.13.14（另有 2.0.0-alpha.2） | `@ark-ui/solid` 5.39.2 |
| 最近发布 | 2026-09-07 | 2026-09-13 |
| 组件目录数 | 59 | 61 |
| GitHub | 1.85k ★ / 126 open issues | 5.39k ★ / 9 open issues |
| 独有 | 颜色选择器家族、`rating` 星级、`number-field`、`time-field`、`menubar` | **`splitter`**、**`tree-view`**、**`date-picker`**、`tags-input`、`scroll-area`、`drawer` |

关键差异：最需要且最难自研的 `splitter`（可拖拽分栏）与 `tree-view`（标签层级树）**只有 Ark UI 提供**。

**图标层 —— 已定 Tabler：**

| 维度 | Lucide | Tabler | Phosphor | Material Symbols |
| --- | --- | --- | --- | --- |
| 数量 | ~1,600 | **6,184（5,130 描边 + 1,054 填充）** | 1,248 × 6 字重 | 2,500+ |
| 填充态 | ❌ 官方不支持 | ✅ 独立 filled 文件 | ✅ fill / duotone | ✅ FILL 可变轴 |
| Solid 包 | `lucide-solid` | `@tabler/icons-solidjs` | 无 | 无 |

Tabler 是唯一同时满足「数量大 + 描边/填充成对 + MIT + 有 Solid 包 + 活跃」的选项。

**设计令牌的视觉规格见 `memory/DESIGN.md`**（配色、双主题、无边线风格）。


---

## 附录 B：M4 重排前的四波路线（历史参考，不是待办清单）

> 保存 2026-09-26 本次重排前的正文，便于核对原已确认范围。上文七波路线已于 2026-09-26 确认；
> 本附录不用于排期、进度或新实现。旧编号在历史决策与 FUTURE 中暂保留，确认新路线后同步引用。

## M4 —— 可试用闭环：实时一致性 + 导出

**目标**：磁盘在程序外变化时不说假话；以 M3 产出的 issue/SOOC 为口径把选好的照片可靠导出——
四大模块（导入/浏览/编辑/导出）在此闭合，之后即可进入发版准备（M5）。

### M4-W1　库与磁盘实时一致性

- 复用现有 `media::{scan,diff}` 与 `store::assets`，补正式编排：进入/展开/重读目录时后台重新读盘，
  少的补入、多的标记缺失（不物理删记录），移动/改名保持资产身份；库离线时只降级，不误判缺失
- 增量监听与攒批：监听只负责触发重读，真相仍来自重新扫描；限制递归范围与触发频率，避免扫描风暴
- 缩略图、FTS、相片/图片计数与 catalog 变化保持一致；为外部增删、拔盘/插回、权限失败补可重复测试
- 这是原 M1-W3 未落到产品运行时的尾项，也是 `memory/FUTURE.md`「清点人头」的正式排期

### M4-W2　导出引擎与预设（按 issue 口径）

- Rust 侧统一导出入口：尺寸/缩放、质量、命名与目录模板、SDR 基线；前端不写像素算法
  * **格式范围以 `memory/FUNCTION-IMAGING.md` §1 为准**（唯一事实源）：位图**导出 5 种**
    （JPEG / TIFF / PNG / WebP / AVIF）、**导入 6 种**（+HEIC）、**RAW 只进不出**、
    JXL 排在后面（导入导出全流程，不替换缓存）、**issue 快照恒 AVIF**；
  * **AVIF 编码器已经在 M3-W3 引入**（缓存图就用它，`image` 的 `avif` feature / ravif）——
    导出只是复用它，不再新增依赖；
  * ⚠️ 「导入 avif / heic」需要另接解码器（`image` 只有 avif 编码器、完全不支持 HEIC）——
    `memory/FUNCTION-IMAGING.md` §1.4；
  * **TIFF 是 16-bit 无损**，兼作外部编辑的前置格式（见 M4-W4）
- 导出来源以 **issue / SOOC** 为第一公民：导出选中的 issue（含编辑栈渲染）或 SOOC 原片；
  RAW / 位图 / 位图+RAW 三形态的来源选择沿用既有契约，身份从「照片」细化到「issue」
- 元数据保留策略：EXIF/IPTC/关键词/版权；RAW 原文件永不改写；水印不阻塞首批闭环
- 此处为旧波次归档：当前预设仅列表载入与按名保存，文件交换暂不启用、目标预览移除；队列内存级，失败信息直接在条目显示。

### M4-W3　导出工作区、批量队列与试用收口

- **开工前置**：`design/export.pen` + `.md`（画廊选 issue、预设、队列与错误清单——未设计，先过界面设计环节）
- 导出工作区（画廊式选择导出哪些 issue，`memory/FUTURE.md`「issue 与 SOOC」的原始设想）；
  批量队列可停止、继续、移除、失败重试，失败信息在条目显示，失效自动移除；无独立摘要或错误清单
- 导出历史可从资产/issue 追溯；文件同名、目标不可写、磁盘写满、源文件途中离线等失败不打断整批
- 1000 张合成/小样本的 Agent 冒烟与内存基线；真实照片的画质、色彩、元数据与使用手感由人类 E2E
- 汇总尚未验收项，形成首个可长期试用的基线；**不在本阶段做安装包发布（那是 M5）**

### M4-W4　外部编辑器：纯单向输出（人类 2026-09-24 定案）

**结束时可看到**：浏览/编辑里能把当前 issue「在外部编辑器中打开」—— 渲染成 16-bit TIFF
存到用户指定位置并唤起外部编辑器；**单向，不取回**（取回链路跑不顺，见 `memory/FUTURE.md`「Gallery / 外部编辑器」）。

- **开工前置**：弹窗界面先过设计环节（`design/external-editor.pen` 或并入 `export.pen`，开工时定）；
- **弹窗**：应用选择（默认选中上次用的；**支持登记多个应用**）+ 导出路径（记忆上次目录）+
  取消/确定（取消在左，§11.5）；**无已登记应用时确定置灰**；
- **弹窗内「+ 应用」**：常驻应用选择最右侧，点击触发**应用发现**，发现结果给用户勾选加入；可反复发现；
- **应用发现**：候选表 × Windows 系统级接口（**可插拔 adapter**，`memory/FUTURE.md`「编辑器发现」）——
  `App Paths` 注册表（HKCU/HKLM/WOW6432Node）+ 常见路径 glob；探测到路径顺手 `fs::metadata` 验证，
  卸载残留直接过滤；**不做全系统枚举**；
- **边界**：登记判重按 **exe 路径**（发现列表中已登记的显示「已添加」）；
  确定时应用 exe 已不存在 → **报错并自动删除该登记**（删的是默认项则回退下一个，删空则确定置灰）；
  弹窗打开时可静默预检失效登记（toast 告知）；
- **候选表首批 9 个**（人类 2026-09-24 定）：Photoshop / Affinity Photo（1.x+2.x 合并识别）/
  Affinity（Canva 整合版；独立 Designer·Publisher **不进**）/ GIMP（2.10+3.x）/ Krita /
  Corel PaintShop Pro / Topaz Photo AI / **RawTherapee** / **RapidRAW** ——
  后两个进入的理由：**我们的 RAW 开发不做极致（至少现在完全不想），专业 RAW 开发送外部**；
  逐条 exe 名与参数模板见 `memory/FUTURE.md` 调研矩阵，落地时逐个实测；
- **送出的是渲染结果 TIFF**（不是原始 RAW）—— RawTherapee/RapidRAW 拿到 TIFF 后其 RAW 专属模块不可用，属预期；
- **命令体系**（§2.15）：登记命令（浏览/编辑上下文可用），默认热键**留空**并写明理由
  （会弹窗、秒级耗时、非高频，不与导出抢键位）；
- 实现手段：`tauri-plugin-opener` 的 `open_path(path, with)`（内部 ShellExecuteW，避引号地狱）；
  编辑器预设存 `app.db.settings`（**设备级**）；**不需要新表、不动 issue 模型、不动 catalog schema**。

### M4 完成定义（DoD）

- 应用外新增/删除/移动照片后，重新进入对应目录能得到真实状态，离线卷不误删 catalog
- 一张与 1000 张导出都能完成；SOOC 与编辑后 issue 都能正确导出；失败可重试、队列可恢复，内存无持续增长
- 「导入 → 浏览 → 编辑 → 导出」不用离开 RayBend 即可走完
- 对 issue/SOOC/RAW/位图的导出来源、尺寸与元数据策略有明确且可测试的契约
- 当前 issue 可一键送外部编辑器：应用发现、多应用记忆、失效登记自动清理符合 M4-W4 规格

---

2026-09-26 崔总补充：手输名称停下 1.5 秒后再判断新增/变更；点选列表立即载入并匹配。等待期间禁用保存，防止按钮含义与实际保存目标不一致。


2026-09-27 M4-W6/W7 开发收口：证据与限制见 `implementations/2026-09-27_m4-finalization.md`，真机验收仍独立。


## 2026-10-04 色彩管理首版：开发与 Agent 冒烟收口

在既有 W1–W5 首版上完成 Astra 的复杂显示 ICC、精确输入加速、60MP 有界后台上传与 RAW 新旧标度重放，再完成 Sol 的实际呈现状态/系统事实分离、共享组件、本地化和设置刷新接线。最终 Rust 1562 项、前端 1207 项通过；中英组件22项与UI冒烟、Windows主程序/worker配套构建核对通过。统一实机验收尚未完成，不自动升级旧照片，复杂CLUT打样/完整HDR/打印仍按既有范围留后续。证据与验收路径：`implementations/2026-10-04_color-management-ready-for-acceptance.md`；核心数值与性能：`implementations/2026-10-04_color-management-core-hardening.md`。Pencil内存稿仍须崔总手动保存，代码未提交。
