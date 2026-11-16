# raybend — 开发计划（specs/PLAN.md）

> **定位**：**只放还没完成的事** —— 路线、待人类事项、未开工单元、未关闭的问题。
> 已完成部分（M0–M4、M5、各主线首版）在 `.agents/memory/FINISHED.md`；远期方向 `.agents/memory/FUTURE.md`；
> 决策记录在 `.agents/memory/ARCHITECTURE.md` §11；定期评审 `.agents/memory/REVIEW.md`；单工作单元规格 `specs/<unit>.md`。
> **进度不写在这里**（唯一的进度事实来源是 todo 工具）。
>
> 建立时间：2026-09-15 ｜ 重构：2026-09-27（完成内容拆至 `.agents/memory/FINISHED.md`）、
> 2026-11-15（决策迁 ARCHITECTURE §11；已销账事项迁 FINISHED §12；`todos/` 退役，需求并入各规格）

---
## 0. 规划原则

1. **先打通四大模块核心流程**。按 2026-09-21 已定路线，M3 先做编辑与 issue 地基，M4 形成「导入 → 浏览/评级/筛选 → 编辑 → 导出」闭环；高级组织检索与全面产品化后置。
2. **风险前置**：把可能推翻架构的事（渲染合成、RAW 解码、SQLite 并发、缩略图吞吐）全部放进 M0 验证，不允许带着未知直接开工产品代码。
3. **每个 milestone 都要能独立交付价值**，而不是「一次性铺完所有底层」。
4. **基础设施只做当前工作单元需要的部分**（例如缓存淘汰策略留到 M5，M1 只要有能跑的缓存即可），避免过度设计。
5. **一个 writer 原则**：人机协作期间，避免同一时段多路修改同一文件。

### 0.1 规划纪律（2026-09-15 修订）

- **一次规划只覆盖一个工作单元**（一个波次，或一个 milestone 里连续可交付的几波）。
- **本文件（specs/PLAN.md）只做路线级描述**：每个 milestone 含哪些波次、完成定义是什么。
- **具体实现方案在每个工作单元开工前单独规划**，写入 `specs/<milestone>-<单元>.md`（如 `specs/M0-1.md`）。
- **不提前细化未开工的工作单元**：项目变大时前置规划必然失真，开工时按当时情况重新规划。
- **已归档的旧规划**放在本文件附录（如附录 A），**明确标注「仅供参考、不是待办清单」**。

---

## 1. 路线总览

只列**还没完成**的部分；已完成里程碑的摘要与证据见 `.agents/memory/FINISHED.md`。

| 阶段 | 内容 | 还差什么 |
| --- | --- | --- |
| M3 编辑：GPU 显影工作台 | RAW/JPG 无损编辑 + issue/SOOC | **真机验收**（真实照片、Windows 两档 DPI、帧率、颜色、原文件哈希）；证据 `.agents/memory/FINISHED.md` §6、`specs/M3-W7.md` |
| M4 可试用闭环 | 实时一致性 + 导出 + 外部编辑 | **真机验收**（三形态、多定稿四格式、16-bit TIFF 外部编辑、颜色/元数据、最小窗/两密度/DPI、多队列长跑）；证据 `.agents/memory/FINISHED.md` §7、`specs/M4-finalization.md` |
| 可卸载存储收口 | 库恢复与手动定位 → 来源恢复 → 任务续跑 → 副本冲突/释放 | **统一 Windows 真机验收**（见 §1.1）；四波开发与 Agent 冒烟已收口 |
| M5 之后三条主线 | 色彩管理 / XMP / 相片整理（原 #4 编辑 preset 已提前开工并完成） | **真机验收**：XMP 核验、bucket/tag + AI 实跑、色彩管理整窗/DPI/多屏/RAW；细节见 §4 |
| 编辑区（`area`）系统 | 多层编辑栈 + 控制点（类 U Point）；层内只装逐像素调整 | **W2–W6 未开工**（W0 设计 + W1 类型与迁移已落地）；规格 `specs/editor-area.md`，接手指南 `specs/editor-area-handoff.md` |
| 窗口还原 + 单实例 | 启动恢复上次四角点位/最大化；最小化关闭清记忆；产品禁止双开 | **真机验收**；实施与单测已收口，证据 `.agents/implementations/2026-10-31_window-restore-and-single-instance.md` |

**基线（2026-09-27）**：四大模块（导入 / 浏览 / 编辑 / 导出）的开发与 Agent 冒烟当时已收口；
真机验收（§2）与发行操作仍单列，2026-10-24 起发行链已收口（`.agents/memory/FINISHED.md` §8/§12）。
2026-09-28 自用摸排后新增可卸载存储收口专项（§1.1）；它完善现有可靠性，不替代 §4 已定的三条主线。

### 1.1 可卸载存储：统一真机验收

2026-09-29 四波开发、单元与 Agent 合成冒烟收口，完成内容与证据已归 `.agents/memory/FINISHED.md` §10。
W1 修订稿与 W3/W4 补充稿均已由崔总定案；暂停阶段记录保留，不回改历史。

按崔总确认的安排，只交同一份 Windows debug 主程序及 worker，统一验证离线设置/盘符定位、
来源恢复、长任务等待续跑、副本选择与主动释放。步骤及边界见 `docs/removable-storage-acceptance.md`；
真实拔插、Windows 句柄、照片、DPI/观感与性能体感尚未经崔总验收，不能以构建或合成冒烟代替。

NAS 实际协议、认证、远程 catalog 和多机写入继续在 `.agents/memory/FUTURE.md` G23，**不作为第五波默认开工**。

---

## 2. 待人类事项汇总（Agent 侧已备好证据，等真机裁决）

已销账的四条（M5 安装/升级/卸载、自动更新链路、正式发行、三条主线开工前讨论）见 `.agents/memory/FINISHED.md` §12.2。

| # | 事项 | 依据 |
| --- | --- | --- |
| 1 | **M3 真机验收**：真实照片、Windows 两档 DPI、交互帧率、颜色、原文件哈希 | `specs/M3-W7.md` 验收清单 |
| 2 | **M4 真机验收**：真实照片三形态、多定稿四格式、外部编辑 16-bit TIFF（九候选实际打开）、颜色/元数据、最小窗/两密度/DPI、多队列长时间运行；M3 未验项一并核对 | `specs/M4-finalization.md`；`.agents/implementations/2026-09-27_m4-finalization.md` |


---

## 3. M5 —— 分发与发布 ✅ 2026-10-24 收口

已收口：四波的交付边界、外部输入项与 DoD 全文在 `.agents/memory/FINISHED.md` §12.1，里程碑摘要与证据在 §8。
留在本文件的相关未关闭项只有一条：**签名/分发链的后续方向**（unsigned 直下 → MSIX）见 §5 #7。


---

## 4. M5 之后：三条主线（人类 2026-09-26 定；**这是发版后的工作重点**）

> **人类原话（2026-09-26）**：「后面最重要的就是 3 点，一个色彩管理，一个 XMP 方便导入迁移，一个强化相片集功能，
> 相片集功能我已经有了些创意想法，到时我们再讨论。」
> **2026-09-30 收缩**：原 #4「编辑器 preset 的存储与载入」（2026-09-28 崔总加入）被崔总**提前拉出**：
> 「现在就做，future 里有就删掉」——已转入当前工作（设计稿先行于 `design/editor.ptr`，
> 需求原文在 `specs/editor-area-bugfix-handoff.md`），不再属于本节。
> **2026-09-30 XMP 口径修正**：#2 的范围改为**写出优先 + 自家导入**；跨家兼容不做（边界之外）。
> 目的是**表态**（开放态度、数据资产归用户）+ best-effort 恢复；**XMP 不是第一公民**，整库备份 = 直接拷 repos。
> 落地规格已写：`specs/xmp-sidecar.md`（同日草案，**待崔总审**；审后交 GLM 实施）。
> （原话见 `docs/user-requirements.md` 2026-09-30 两节；结论账本 `.agents/memory/REVIEW.md` R6-01～R6-04。）
>
> **定位**：M5 发版之后，**最重要的事就是这几件**。其余（多仓、缓存治理、数据安全、通用 AI、跨平台、
> 打印/HDR……）仍留在 `.agents/memory/FUTURE.md`；2026-09-30 崔总明确将有限本地 AI 标签列入相片整理下期，范围仅见下表 #3。
> **状态**：相片整理上期功能（含跨库多选导出）已实现并通过 Agent 冒烟，Windows 真机交互与大库性能待验收；下期 TinyCLIP 折中阈值、任务/来源纠错/获批 UI/XMP 与默认 CPU 模型资源已接线，通过开发与 Windows worker 冒烟；可进入上线前真机测试。人物漏标、海类误标/漏标及真实图库/GUI/安装器尚待验收；证据见 `docs/ai/tinyclip-v1/README.md`。其余主线按开工前规格继续细化。
> （背景：当时 M4-W3 在开发中、M5 未开工；同日已授权 M5 独立工程准备并行，状态见上表。）

| # | 主线 | 是什么 | 现状与落点 |
| --- | --- | --- | --- |
| 1 | **色彩管理** | 输入认得出（位图内嵌 ICC / 相机 profile）、工作空间装得下（线性广色域）、**显示器显示得准**、导出说得清 | **2026-09-30 崔总授权开工**；主题契约 `specs/color-management.md`。先补设计稿（paintress）并自行审视，接着按下表波次持续实施，直到只剩真机验收或无法绕过的外部条件。系统互操作提前按 adapter 设计。 |
| 2 | **XMP**（**写出 + 自家导入**；**范围已冻结**，2026-10-01） | 让本库成果能**跟着照片走**：写出 `rb:`（全部 issue + latest）+ 标准层（raw-based latest 的 `crs:` 调整 + 基础元数据：评级/色标/关键词/说明/作者/地点）；自家导入在新登记资产时自动发现读回。目的是**表态**（开放、数据资产归用户）+ best-effort 恢复；**XMP 不是第一公民**，备份靠直接拷 repos 目录 | **已完成，待验收（崔总 2026-10-04 最新口径）**；本轮缺陷修复与回归见 `.agents/implementations/2026-10-04_xmp-audit-fixes.md`，Windows 真机验收尚未进行；已冻结的功能范围仍见 `specs/xmp-sidecar.md`（旧名 `xmp-w1`），原实施报告保留作阶段证据；主体交付与真机验收状态分开记录。契约：`specs/issue-xmp-contract.md`（随规格小幅修订：`rb:ordinal`、`profileJson` 剔除 `auto_adjust`）。**跨家导入不做**（不是延后；R6-01/R6-05）；R6-02/03/04/05 均已结案。材料存档：`specs/M4-export-cleanup.md`。**2026-10-09 增补**：设置里新增唯一一项**写出开关（设备级、默认开、可关不删）**；验收清单细化进 `specs/xmp-sidecar.md` §10（A–E 组）；巡检工具 `pnpm check:xmp`（`specs/xmp-write-toggle.md`、`specs/xmp-audit-tool.md`）——真机验收仍待崔总 |
| 3 | **相片整理：bucket + tag** | 照片与目录分别打标签，桶承担持久工作集合；H6 图片筒并入 | **上期代码与 Agent 冒烟已完成（含跨库多选导出），真机验收待崔总。** 上期为三入口整栏、独立照片/目录标签、手动桶、共享规则弹窗、多组 OR / 组内 AND 自动桶和挂载库浏览；目录标签不传播到照片，筛选转桶不保存导航范围。正文 `specs/photo-organization.md`、`specs/photo-organization-phase1.md`。**下期 TinyCLIP 已定案，折中阈值、数据/任务/默认 CPU worker/模型资源/来源纠错/获批 UI/XMP 已接线，并通过开发及 Windows worker 冒烟；进入真机测试阶段，效果/GUI/安装器尚待验收。** 实施记录 `.agents/implementations/2026-10-04_photo-organization-ai-tinyclip-delivery.md`。 正文 `specs/photo-organization-phase2.md`；首个有界模型验证单元 `specs/photo-organization-ai-w1.md`。点击蒙版留 FUTURE D4/E2，不纳入照片整理。 |

**2026-10-04 收尾顺序（崔总确认）**：XMP 与 editor presets 已完成、待验收；当前完善色彩管理与文件管理，等两项完善并验收后推进发版，由崔总在其它会话处理；**发布 CI 化不在计划内**（`.agents/memory/FUTURE.md` G24 仅存档，发版前/后都不做）。

### 编辑区（`area`）系统（2026-10-31 崔总口述；**W0 + W1 已完成，W2–W6 待做**）

> **接手入口**：`specs/editor-area-handoff.md`（换手交接书：现状、剩余波次、陷阱、验收、交付定义）。

编辑栈从「一份参数」变成「一叠层」：每层有名字、上下顺序、启停，层内装逐像素的影调 / 色彩 / 曲线；
**`control point`（控制点）是层的一种类型**（一处冻结种子向量 + 一个**容差** + 平滑权重；**全图匹配、不是蒙版**）。

* **规格唯一事实源**：`specs/editor-area.md`（数据契约、求值与阶段边界、交互、波次、决策账本、风险）。
* **红线**：层内只允许逐像素操作（`AGENTS.md` §2.27）；这条决定了「合成不需要中间图」。
* **开工门禁**：先出 `design/editor-area.pen` + 同名 `.md` 六屏（面板 / 层名条+直方图瘦身 / 画布标记 /
  非全图禁用态 / toolsbar 两处 / 两种空态），定案后才动代码（`AGENTS.md` §5.1）。
* **波次**：W0 设计 → W1 类型与迁移（不碰 UI）→ W2 渲染 → W3 撤销与持久化 → W4 面板与画布 → W5 周边适配 → W6 收尾。
* **已定的三个口径**：层间**顺序累积**（非归一化竞争）；**首版即可拖动重排**；种子**创建时冻结**并给「重新取样」。
* **三种层**（2026-10-31 定稿）：`默认`（恰好一个、不可删、全图效果 + 全局功能宿主：LUT / 局部色调 /
  清晰度 / 镜头 / 色彩管理）、`全图`（用户建的调整层）、`控制点`；蒙版未来再加。
* **精度契约**：层间 f32 线性、不落地、不夹取，量化只在入 / 出各一次（"多 area 不伤色阶"的根据）。
* **画面变化**：LUT 从"锐化之后"提到"锐化之前"（LUT 挂 `默认` 层，锐化必须在链后），需 `PIPELINE_VERSION` 加一。
* **不属本工作流**：蒙版 / 智能蒙版（未来作为 `AreaKind` 新变体）。

### 色彩管理执行波次（本轮获准开工）

| 波次 | 交付边界 | 验收性质 |
| --- | --- | --- |
| CM-W1 | 设计稿、自有色彩身份/资源与旧版兼容契约、系统 adapter 接缝、全局设置入口与界面骨架 | 设计与架构可审查；首波不宣称显示色彩已准确 |
| CM-W2 | RGB ICC 输入、RAW/位图高精度工作空间与共用输出转换，缓存代理和导出正确标记 | CPU/离屏参考向量、编码读回与旧 issue 回归 |
| CM-W3 | Windows 双路径显示管理、GPU 转换、跨屏与 HDR 桌面 SDR 兼容 | adapter fake 与 GPU 冒烟；真实屏幕由崔总验收 |
| CM-W4 | editor 面板、全局资源/默认与批量指定、预设可选组，XMP 色彩扩展与依赖保全 | 命令、存储、撤销、旧定稿与资源迁移回归 |
| CM-W5 | 软打样、目标色域警告与本轮范围收口 | 单测/冒烟后交真机色彩和观感验收 |

2026-10-04 色彩管理实施证据：`.agents/implementations/2026-10-04_color-management-float-gpu-and-persistence.md` 与 `2026-10-04_color-management-core-hardening.md`。W2 新处理路径已有显式入口，复杂输入 LCMS 精确并行、RAW 新标度与旧身份重放接通；W4 资产/默认/批量/预设/XMP 接通；W5 已验证矩阵/TRC RGB 打样（复杂 CLUT 打样仍不支持）。W3 已接 Windows adapter、GPU/scRGB、受验证原生 mBA/PCS CLUT 显示及大图后台完整帧提交，真实 DX12 离屏和 60MP 独立进程资源已有证据；整窗/DPI/多屏/RAW 与显示色彩仍待崔总统一验收。旧照片/issue 不自动升级。常规显示状态本地化/共享组件与实际呈现接线也已完成，最终单元/UI冒烟和Windows配套构建通过，首版开发可统一验收；见 `.agents/implementations/2026-10-04_color-management-ready-for-acceptance.md`，不能仅以工程验证宣布五波真机验收通过。

各波次开工时只细化**当前一个**到 `specs/cm-w<n>.md`，按 `AGENTS.md` 的设计先行与实施记录纪律推进。打印、完整 HDR 照片功能、硬件校准及跨平台实装留 `.agents/memory/FUTURE.md` C1/C2；不把它们混入本轮完成定义。

### 相片整理上期交付与下期去向

| 期 | 工作单元顺序 | 完成定义 |
| --- | --- | --- |
| 上期：主要功能 | U1 交互稿/契约 → U2 标签导航与多库数据源 → U3 桶本体与共享规则编辑 → U4 自动收片及收口 | 不依赖 AI 即可用标签找照片/目录、跨库装桶、按多组规则持续收片；可靠性与已有编辑/导出闭环成立 |

上期已按获批 Pencil 稿实施，并保留来源与统一标签入口。**2026-10-04 下期代码与 Agent 冒烟收口，进入上线前真机测试**：崔总指定 TinyCLIP，逐类按校准 precision 目标85%、F0.5、至少5个真阳性固定阈值，人物/海类加入固定负描述；七类独立评分，可返回空标签。数据/来源禁止/撤销/消费方/XMP、范围冻结/持久队列/串行 CPU worker 与获批三稿均已接线。可信清单含固定 v1，默认桌面启用 CPU；固定模型与 ORT 资源随应用提供，设置中主动安装，可损坏修复/卸载与可信离线导入。Windows debug 资源核对及完整桌面 worker 真图编码冒烟通过。公开回归中人物漏标、海类误标/漏标仍明显，不宣称所有类别达到旧90%/60%门槛；GUI、真实图库、盘上 XMP 与干净 Win10/11 安装仍待验收。在线下载留后续分发优化，SigLIP 2 留 FUTURE E1，不阻塞本期。本轮独立 Windows 测试副本为 `C:\rb-target\raybend-ai-test-2026-10-04\raybend-desktop.exe`（13:21:13源码快照，AI源与交付时原仓一致）；后续整体发行另按届时源码统一构建。主题契约 `specs/photo-organization-phase2.md`；当前依据 `docs/ai/tinyclip-v1/README.md`；实施记录 `.agents/implementations/2026-10-04_photo-organization-ai-tinyclip-delivery.md`。
XMP 已完成、待真机验收（2026-10-04）；AI新增标签来源已接入现有sidecar，并通过标准关键词/rb:tagState往返回归；真实库仍未验收。点击生成蒙版已整合 FUTURE D4/E2，分割/人脸/语义搜索不随本线扩围。

### 三条主线的共同约束（沿用既有纪律，不另立规矩）

- **界面设计先行**：三条要么新增界面、要么改现有界面 ⇒ 按 `AGENTS.md` §5.1 先出 `.pen` + 同名 `.md`。
  色彩管理本轮崔总已明确授权设计后自行审视并直接实施，无需中途等设计确认；其余工作单元按各自授权执行。
- **命令体系接入**：新增的用户可触发动作要登记进命令注册表，且在**同一次改动里**给出默认热键
  或写明留空理由（`AGENTS.md` §2.15）。
- **数据库变更走迁移框架**：需要新增表/列时一律走 `store/migration.rs`（`AGENTS.md` §2.16），
  不许启动时临时 `ALTER`、不许另造迁移通道。
- **开工顺序**：每条开工前另写 `specs/<主线>-W<n>.md`（`AGENTS.md` §5.4），不提前细化未开工的波次。

> **第 3 条特别说明**：当前语义见 `specs/photo-organization.md`；上期实现沿用整栏三入口和目录标签不继承的方案，不回退到旧集合草案。

---

## 5. 必须处理的问题清单（按优先级；2026-11-15 核对，只剩未关闭项）

已关闭的 7 条（渲染合成风险 / rawler / 文件身份 / catalog 损坏 / GPU 恢复 / 中文分词 / 老库 RAW 尺寸）连同结论迁入 `.agents/memory/FINISHED.md` §12.3。

| # | 问题 | 影响 | 状态 / 处理时机 |
| --- | --- | --- | --- |
| 1 | **去马赛克与画质算法自研成本高**（GPL 实现需移植而非直接可用） | 决定编辑质量与性能 | M3 已落第一阶段；升级路径与候选见 `.agents/memory/FUTURE.md` D2/D3，随编辑深化再取 |
| 6 | 范围失控：一人做 ON1 级产品 | 项目死亡 | 持续——严格按里程碑走，扩围一律过 `.agents/memory/REVIEW.md` |
| 7 | 代码签名 / 分发 / SmartScreen / 自动更新 | 发布期阻塞 | ◐ R3-01 拍板 unsigned 直下首发 → MSIX 后续（本文件 §3、`.agents/memory/FUTURE.md`）；EV 证书明确不买 |
| 11 | **其余画稿的弹窗描边未同步** | 设计稿与实现不一致 | ◐ 2026-10-24：`design/main.ptr` 的 32 个弹窗帧已加 `$brand-2` inner 描边；2026-10-31 随实现同步减为 0.5px。`browse.ptr` / `editor.ptr` / `export.ptr` 的弹窗帧仍**没有描边**。**2026-11-12 崔总定：等 paintress 调顺后再补**，已登记 `.agents/memory/FUTURE.md` 发版前板 #11（工具见 `.agents/memory/PAINTRESS.md`） |


---

## 6. 决策记录

已迁 **`.agents/memory/ARCHITECTURE.md` §11**（逐字保留：日期 / 决策 / 结论 / 依据）。本节不再维护第二份。


---

## 7. 本文件沿革（specs/PLAN.md）

> 只记**本文件**改过什么；跨文件的事实变化在 `.agents/implementations/`。

| 日期 | 变更 |
| --- | --- |
| 2026-10-31 | **编辑区（area）系统规划落地**：崔总口述多层编辑栈 + 控制点交互，同日四个关键问题拍板；规格写入 `specs/editor-area.md`，术语与红线进 `AGENTS.md` §2.27/§11/§13，远期项进 `.agents/memory/FUTURE.md` D11–D13。**未实施，未开工**。 |
| 2026-10-31 | **窗口还原 + 单实例落地**：四角点位比例 + 屏幕身份 → `windows-state.json`；启动单次判断 + 三种操作；最小尺寸单边保底与 titlebar 保底；最小化清记忆、最大化该屏最大化；`tauri-plugin-single-instance`（dev 例外）。规格 `specs/window-restore.md`，实施记录 `.agents/implementations/2026-10-31_window-restore-and-single-instance.md`。 |
| 2026-09-30 | **相片整理两期规划**：崔总明确整栏三入口、目录标签保持目录颗粒度、多组自动规则、新建/编辑共用条件链弹窗，筛选转桶不带目录等导航范围；要求上期主要功能、下期 AI 增强分别规划。范围与验收已落 specs，未实施，见 §4 #3。 |
| 2026-10-24 | **弹窗叠放与边界线**：统一 `DialogFrame`、自动遮罩栈（删除 `scrim` 开关）、栈深 z、1px `--brand-2` border、`--z-popover` 抬升、titlebar `persistentElements`。`main.ptr` 已同步，其余 `.pen` 见 §5 #11。 |
| 2026-09-29 | **相片集主线讨论启动**：H6 图片筒（跨目录工作集）并入本线一起做（崔总定）；Agent 设计草案已出但未采纳，等崔总整理方案后再写规格 —— 见 §4 #3 与 `specs/photo-meta-and-tile-display.md`。同日备份功能确认要做（落点 `.agents/memory/FUTURE.md` J 节） |
| 2026-09-29 | **可卸载库 W1 开发冒烟收口**：按批准修订稿完成永久设置入口、同库位置管理与手动定位，统一会话/状态/反馈，故障注入与 Windows debug 校验通过；真机验收与 W2–W4 仍单列。最终证据见 §1.1 |
| 2026-09-29 | **可卸载库首波扩围（崔总采纳）**：同库备用位置管理、离线设置入口和手动定位由 W4 提前到 W1；W4 保留副本冲突与释放。修改后的视觉稿仍须定案，当前代码实施保持暂停；规格与阶段证据见 §1.1 |
| 2026-09-15 | 初版：确立 7 个 milestone 与波次划分、决策记录表 |
| 2026-09-15 | 新增 §0.1 规划纪律（一次规划只覆盖一个工作单元）；M0 章节收敛为工作单元索引表；原一次性铺开的 M0 波次规划归档到**附录 A** |
| 2026-09-15 | **M0 重构**：spike 由「一次性门禁」改为「按模块按需前置」——M0-4/5/6/3 服务于 M1，M0-2 后移到 M2 前；新增「设计阶段」章节 |
| 2026-09-17 | **官网支线落地（不在 milestone 序列内）**：`website/` 首页（双语 / 下载区 / 教程占位）+ `.github/workflows/website.yml`（push master / 正式版 release / 手动 → GitHub Pages）。**不属于应用本体的进度**，不给 milestone 排期；方案见 `specs/website-homepage.md`，约定见 `website/AGENTS.md`，待人做的事见 `website/ASSETS.md` |
| 2026-09-20 | **M2 后路线重新基线化**：M3 改为「实时一致性 + 导出」以尽快闭合首个可试用流程；原 M3 与 XMP 合并为 M4；M5 按已提前完成的底座从 5 波压成 3 波；新增 §1.1 明确“已完成地基 ≠ 产品能力完成”的边界 |
| 2026-09-21 | **M2 结案 + 路线再重排（人类定案）**：新增 §1.2；M3 改为**编辑（GPU 显影工作台）**并设 W0 界面设计前置；原 M3（一致性+导出）顺移 M4，导出按 issue/SOOC 口径重写；原 M4（组织检索）与原 M5（多仓/缓存/数据安全，含数据备份支持）移入 `.agents/memory/FUTURE.md` I/J 节；原 M6 → M5，定位「四大模块完成即发版先用」；「设计阶段」新增硬规矩：未设计界面的模块必须先过界面设计环节（现有设计仅导入与浏览） |
| 2026-09-24 | **M3 波次再切（人类定案）**：CUBE LUT 与 issue 从 W4 拿出来，与「编辑数据结构重整」合并为新 W6；原 W6（收口）顺移 W7。W6 同时重整：无损变更的变更集数据结构与存储、调整后的缓存快照；issue 数据结构打版本号（现阶段既有 latest 与旧 issue 数据须迁移并保持结果一致） |
| 2026-09-24 | **外部编辑器进 M4（人类定案）**：M4-W2 导出格式清单已含 **TIFF**（16-bit 无损，外部编辑前置）；新增 **M4-W4「外部编辑器：纯单向输出」**（候选表首批 9 个 + 应用发现可插拔 adapter + 弹窗交互规格：默认上次应用 / + 应用发现 / 按路径判重 / 失效报错并自动删登记）；M4 波次 3 → 4 |

| 2026-09-26 | **M4 重排定案（已确认开工）**：依据 export 画稿/说明、`.agents/prompts/exports.pd` 与 M3-W6c/W7 实际代码，将原四波拆为 W0 口径确认 + W1 一致性 / W2 工作区与预设 / W3 高精度全尺寸出图 / W4 五格式与元数据 / W5 内存后台队列 / W6 外部编辑 / W7 收口；原路线归附录 B；不自行采纳 REVIEW 待决扩围 |
| 2026-09-26 | **W2 补充定案与实施**：issue 是入队单位；各预设独立队列与处理开关全部内存级，参数与展示偏好持久化；卡片显示待完成/含完成总数；实际 running 才触发 flowbar 文字闪烁。W2 交付选择/快照入队与预设，编码/后台线程仍属 W3–W5；取消选择复用 Esc |

| 2026-09-26 | **W3–W5 实施收口与行为校对**：共享高精度出图、方形定稿 Tiles、五格式原生元数据、预设交换/预览、最多四预设后台内存队列已开发并通过单元/冒烟与 Windows 产物核对；执行前哈希失效跳过，重置不打断在途出图。已确认行为表按当前定案重写，取消选中只保留 Esc，范围控制原文保留；W6/W7 与真机验收仍未完成 |

---

| 2026-09-27 | **记忆体重构**：完成内容（M0–M4、M5 工程准备、设计稿现状、路线演变、历史附录）拆出至 `.agents/memory/FINISHED.md`；本文件只保留未完成排期（M5 剩余验收、三条主线）、问题清单、决策与变更记录 |


---

## 8. 规格已就位、尚未开工

| 规格 | 内容 | 原文出处 |
| --- | --- | --- |
| `specs/lut-multilayer-ui.md` | 多层 LUT：应用层 + 列表多选（已实现，真机观感待验收；规格作为需求与审阅存档） | 原 `todos/2026-10-31-lut-multilayer-ui.md`（逐字） |
| `specs/photo-meta-and-tile-display.md` | 相片集（bucket + tag）主线需求与设计讨论存档；本线正文在 `specs/photo-organization.md` | 原 `todos/2026-09-29-photo-collections.md`（逐字） |
| `specs/editor-area-bugfix-handoff.md` | editor 预设面板的需求原文（已实现，见 `specs/editor-presets.md`） | 原 `todos/2026-09-30-editor-presets.md`（逐字） |
| `specs/M4-export-cleanup.md` | XMP 范围与生态查实材料（导出优先） | 原 `todos/2026-09-30-xmp-export-first.md`（逐字） |
| `specs/M2-W2-tiles-film-view-unify.md` | toolsbar 紧缩 / 导出筛选 / export tile 小改的需求原文（已实现） | 原 `todos/2026-10-11-toolsbar-compact-export-filter-raw-tile.md`（逐字） |
| `specs/browse-info-fields.md` | browse 左列目录入口改造 + 照片移动 + 编辑区空格修复的需求原文（已实现） | 原 `todos/2026-11-05-browse-left-dir-entry-and-move.md`（逐字） |
| `specs/database-upgrade-gate.md` | catalog 升级中的必需预置图生成（未做，留待真实生成需求） | 原 `todos/2026-09-29-catalog-preset-image-upgrade.md`（逐字） |

---

## 9. 预算债与延后项（2026-11-15 登记，本轮不支付）

| # | 项 | 实测 | 处置 |
| --- | --- | --- | --- |
| D1 | **`AGENTS.md` 超预算** | `wc -c AGENTS.md` = 69283（soft 12000 / hard 20000） | 本轮只做索引与指针收敛（73293 → 69283，−4010）；不重写正文。下一轮按 `.agents/memory/README.md` 的压缩配方搬：§2 硬约束 → 各拥有者文件、§11 术语 → 计划中的 `TERMS.md`（记忆文件，尚未建立）、§12 组件清单 → `COMPONENTS.md` / `ARCHITECTURE.md`、§5 纪律细则 → 各记忆文件 |
| D2 | `DESIGN.md` 超预算 | 119491 bytes（预算 80000） | 待下一轮按主题拆分（色板/尺寸/交互范式各一份，ARCHITECTURE 留索引） |
| D3 | `FUTURE.md` 超预算 | 141451 bytes（预算 80000） | 待按 A–H 分节拆出 `.agents/memory/archive/<year>/` 或独立文件；每项补齐触发条件 |
| D4 | 变更记录合流 | 6 个文件各有「本文件沿革」表（共 97 行，逐行事实互不重复） | **未合并**：并入单一文件会把该文件顶过 80000 预算；本轮只把标题改成文件作用域，跨文件变更事实的唯一家仍是 `.agents/implementations/` |
| D5 | 审计 `W-ORDER-01`（FINISHED.md 附录后有章节） | 5 个小节位于「附录 A/B」之后 | 未搬：属纯顺序整理，动 10k 字符而无线索收益；下一轮与 FINISHED 压缩一起做 |
| D6 | 审计 `W-REC-01`（20 条近期记录文件名不带分钟） | 最早一条为 `.agents/implementations/2026-11-13_paintress-040-to-051-upgrade.md` | 不改历史文件名（记录不可重写）；新记录一律走 `record_change` |
| D7 | 审计 `W-UAT-02`（360 条记录待验收） | `.agents/uat/ledger.jsonl` | 需崔总跑 `/ask-uat`；Agent 不改账本 |
| D8 | 审计 `W-PLAN-01`（3 份规格打勾齐全却仍被 PLAN 提到） | `M4-finalization.md` / `photo-meta-and-tile-display.md` / `website-homepage.md` | **不销**：三处引用性质不同（M4 真机验收清单、§8 需求存档、2026-09-17 沿革行），打勾齐全只说明当时的施工清单走完，不代表真机验收已过（M4 仍在本文件 §2）；逐条销号要先定「施工清单完成」与「验收通过」的区分口径，属专门的收口轮 |
| D9 | 审计 info：实施记录 380 条（预算 200） | `.agents/implementations/`（380 个文件） | 待按里程碑汇总进 FINISHED 后把原件移入 `.agents/implementations/archive/<year>/`；本轮不动历史记录 |

