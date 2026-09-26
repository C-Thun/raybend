# M4 路线重排提案
完成时间：2026-09-26 13:57:14 CST

## 本次范围

仅整理待确认路线，未实施 M4 功能、未修改 Pencil 画稿、未提交或发布。
修改 `PLAN.md`：M4 总览、七波路线与 W0 前置、当前事实/缺口、待决项、DoD；
原四波正文归附录 B。顺手校正同文件“编辑一律后置”的过时原则与设计前置状态，
给 2026-09-20 路线表加历史标记，避免与已批准的 2026-09-21 路线相矛盾。
其它既有工作区改动均保留。

## 调研依据

- `PLAN.md` / `REVIEW.md` / `IMAGING.md` / `REPOSITORY.md` / `FUTURE.md` / `ARCHITECTURE.md`。
- `design/export.md`、实际原版口述 `prompts/exports.pd`（会话提到的 `prompt/exports.md` 不存在）。
- Pencil MCP 读取 `design/export.pen`：活动文档正确，核对主稿、运行中、工具栏、预设、空态、浮层；
  读取实际 bounds 与文字，未读写 .pen 文件原文、未改节点、未声称 GUI 视觉验收。
- M3-W6a/W6b/W6c/W7 与最新定稿/WebView/LUT 封面修复记录；
  `store/issues`、`src-tauri/issues`、`develop/pipeline`、`thumbnail/render`、RAW/显示接口。
- `App.tsx`、browse 的目录/计数逻辑、统一 PhotoGrid/Tile/TilesSource/状态栏、
  SplitStack/stack-resize、tokens/window min size、命令注册表与选择算法。
- 扫描/diff/assets/rebuild、app settings/jobs schema、thumbnail worker、fs_atomic。

## 为什么这样切

1. SOOC/latest/不可变定稿已经落地，沿用完整 profile；导出选择不能依靠浏览切稿来改写 latest。
2. 进目录只同步计数还不够；手动全库重建忽略扫描完整性且未带文件身份，不能直接用于局部实时刷新。
3. CPU 显影先输出 RGB8，后续锐化/LUT/几何也是 RGB8；16-bit TIFF 需要前置改造共享高精度出口，不能包装 8 位预览。
4. jobs 的通用操作嵌在缩略图模块；本轮抽取复用，导出只增加业务控制与载荷。
5. 原版是持续运行、多预设独立队列，空队列等待新任务；计划据此区分控制开关和条目状态。
6. 1200×800 最小窗扣外壳后两窗不能同时硬设 384；右栏示意 360 也与全局 276/288 相冲突。
7. 五格式、元数据、命名发布和资源预算各有真实缺口，拆波后更容易交付与定位回归。

## 命令与热键评估

只提出设计侧建议；尚未注册 M4 命令。建议扩充已有全选/取消选中命令，
Esc 清除、Enter 入队、Mod+Enter 运行开关需在实施时做作用域/冲突/输入态检测。
其余动作留空需说明理由。原设计声称已有“复制”命令冲突，当前注册表未读到该条；
本提案改以原生复制习惯/输入态为理由，等待崔总定案，不擅自改口述热键。

## 验证与边界

- 文档结构、原四波内容保留与工作区差分检查；无代码改动，不重跑构建/单测。
- 本轮引用 M3 实施记录的既有测试结论，不当成本轮重测，也不当成 Windows E2E 已验收。
- REVIEW 全部待决方向未自行采纳；未扩围 ICC、集合、XMP、Shift/旗标等功能。
- 默认 sandbox 因 WSLg socket mount 冲突无法启动；获准使用 require_escalated 读取确定路径和写入本次文档，
  未修改 sandbox 配置、未全盘搜索。无剩余工具阻塞。

## 交接

请崔总确认 `PLAN.md` 的 M4 路线与待决口径。确认后再同步所属规格/Pencil 与旧编号引用，
按完整波次写开工计划、评审后实施；本次不创建七份未来详细实施方案。
