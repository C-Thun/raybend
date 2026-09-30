完成时间：2026-09-30 22:11:00 CST（Asia/Shanghai）

# 色彩管理方向规划与系统 adapter 边界

## 范围与文件

本轮仅更新规划文档；没有修改产品代码、Pencil、数据库 schema、依赖或正在并行设计的 XMP 契约，也没有构建/发布/推送。

- `specs/color-management.md`：直接修订已有主题规格，覆盖 SDR 目标、Rust/GPU 准备、Windows、面板、全局默认与批量、预设、issue/XMP、资源保全、兼容与验证边界。
- `memory/ARCHITECTURE.md` §2.2：记录崔总明确追加的系统互操作 adapter 约束。
- `memory/FUTURE.md` C1/C2：更新主题索引，区分 HDR 照片功能与 HDR 桌面下 SDR 显示兼容。
- `memory/PLAN.md` §4、`memory/REVIEW.md` 当前汇总 R1-01：仅刷新该主题的规格指向/状态，保留既有优先级与历史决策；不把建议升级成已采纳。

## 请求与关键判断

本轮先要求在 editor right 的预设右侧规划色彩管理，形成完整方向图景；随后明确追加：

> 与系统相关的互操作部分均要以adapter架构做提前设计

已确认项只有界面位置与上述 adapter 原则。浮点线性 Rec.2020、首个 SDR 闭环范围、预设默认不带输入色彩组等均标为建议待定案。

读取现有显影、RAW worker、GPU 呈现、导出、LUT 文件资产、预设与 issue 源码后，修正旧稿中的关键问题：当前中间像素不是一直保留 f32，而是交接成 u16 0–1；显示末端仍是 RGBA8。必须规划像素范围和算法域，而非只加显示 LUT。旧 issue 必须保持处理语义，派生缓存可以失效、定稿资产不能作废。

复用已有主题规格、`PresentationAdapter`、窗口胶水、显影/导出能力、codec ICC 接口、LUT 的受限扫描/原子复制/内容哈希底座与预设分组。ICC/DCP 属于各自的数据适配，不复刻资源管理器；本轮不新建实现。

系统边界提前区分环境快照/事件、原生呈现、系统资源与设置入口；提供能力和错误状态、订阅释放、状态版本与 fake adapter。真实 ICC 显示转换和 Windows ACM 的最终映射不能无条件同时做。

XMP 的后续扩展须区分照片色彩状态、处理版本、外部资源身份与输出配方；屏幕状态不进 issue。自定义配置需字节保全，名称/哈希不是文件本体。只在主题规格写衔接建议，未联系或改写其它并行任务。

## 查证与验证

已验证（文档/源码检查）：

- 阅读 `memory/FUTURE.md`、已有 `specs/color-management.md`、issue/XMP/预设规格及当前源代码；本地核对 `image 0.25.10` 的 ICC 接口、`wgpu-types 30.0.1` 的 surface 色彩声明。
- 查阅 Microsoft 的 Advanced Color/ICC、DirectX、`ColorProfileGetDisplayDefault`、校准管线，以及 Adobe XMP `photoshop:ICCProfile`、ICC、Little CMS 官方资料；引用落在主题规格相关段落。
- Firecrawl CLI 已安装且有凭据，但 `firecrawl --status` 报 account info fetch failed；改用可用的内置 web 工具核对官方资料，没有安装工具或改认证配置。
- `git diff --check -- specs/color-management.md memory/FUTURE.md memory/ARCHITECTURE.md memory/PLAN.md memory/REVIEW.md` 通过。
- Python 文档检查确认主题一级章节 1–12 连续、adapter 小节唯一、规格引用文件存在、索引相对链接有效。

仅文档变更，不运行 cargo/pnpm 产品测试，不声称任何色彩、平台兼容或 GUI 已通过冒烟。未做真机 E2E。

## 命令体系

规划已明确：面板入口、配置文件管理、批量指定/恢复、软打样与色域警告在实施时接统一命令；设置沿用 `Ctrl+,`，其余建议默认键留空（专业低频且避免占用既有编辑键），提供命令面板与自定义入口。本轮没有新增可执行功能，故不改命令表。

## 边界与遗留

方向文档已完成。实现仍需后续单工作单元规格及 Pencil 定案；色彩正确性、多屏、ACM/HDR、DOM/原生一致性由崔总真机确认。scRGB 在本地依赖中有接口，但当前硬件/合成路径是否可用尚未验证。自定义资源打包方式、DCP 支持子集、具体 schema/SQL 和格式覆盖留给对应实施单元。

会话没有可调用的 pi todo 工具，未另造进度表。工作区开始时已有大量并行修改，本轮仅对上述文档做定点更新，未撤销其它改动。
