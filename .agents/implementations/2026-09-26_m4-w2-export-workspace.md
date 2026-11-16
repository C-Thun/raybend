# M4-W2：导出工作区、issue 画廊与预设
完成时间：2026-09-26 17:01:08 CST

## 范围与结果

崔总已授权继续 W2。交付真实导出工作区、预设创建/修改与校验、issue 多选、入队快照、各预设私有队列展示。相同照片的多个 issue 可以一次入队。W2 未实现文件编码或后台执行，开始/停止按钮禁用并给出阶段说明；不能把可入队等同于已经能导出文件。

队列计数为「待完成 / 总数含完成」；failed 仍未完成，只有 running 触发 flowbar 导出文字闪烁。闪烁复用 import 的 rb-shimmer-mask，没有新增动画算法。运行开关、队列及完成记录仅保存在 App 生命周期内，不写 jobs / DB / localStorage；重启队列为空、开关 off。W3/W4 实施统一高精度出图与编码，W5 接 Rust 独立线程和状态通知。

## 实现与复用

- `crates/raybend/src/export.rs`：最多 128 资产的批量摘要，命名 issue 只读摘要；明确取 SOOC / RAW 原始项 / latest / 命名稿的快照，验证完整 profile 哈希、源路径、角色与源签名。不通过切稿改写 latest。预设验证名称、五格式、质量、最大边、目录与模板，模板复用 import::template，目录可写探针即建即删。
- `store/develop.rs::has_edits` 改为共享 DevelopStack::is_empty 判定，消除镜头默认开关等旧 SQL 判定差异。浏览与导出使用同一非空判据。
- `src-tauri/src/export.rs` 仅做 IPC/阻塞任务转发；issue 预览复用 thumbs 的 render_profile_cached、RAW worker 与 FullCache。队列预览读捕获 profile；源变更显式报错。LUT 解析失败不再静默丢弃 LUT。
- `src/lib/export-model.ts` / `export-prefs.ts`、`src/workspaces/export/store.ts`：稳定 preset ID 与名称分离，相同规范化名称更新；参数序列化白名单。参数保存 app.db 既有设置的 export.presets.v1，设备偏好用 raybend.export-display.v1；本波不改 schema。队列深拷贝 profile 与已保存参数，后续编辑不污染旧条目；以库/variant/profile 哈希去重，最新入队在上。批量取稿失败整批不入队；异步范围选择有代次守卫。
- `source.ts` / `ExportWorkspace.tsx` / `Toolbar.tsx`：复用 BrowseLeftColumn、唯一 PhotoGrid/Tile、缩略图限流/LRU、时间分组、SplitStack/SplitHandle、两个 TilesShell 状态栏及全屏看图。扩展数据适配与附加槽，不创建第二套网格。超过四个 issue 自动换行，行高取同一行的最大附加高度；队列列表模式仍用同一 PhotoGrid。两个缩放与上下比例独立保存。仅 SOOC 范围只含真实位图，RAW-only 中性渲染项出现在全部范围。
- 全屏清单增加可选 exportVariant，同照片不同稿不会合并；既有普通照片载荷保持三字段。DTO 契约与往返测试同步。
- Pencil MCP 修订 `design/export.pen`：公共外壳与右列宽度、卡片计数、W2 禁用主按钮，补紧凑/宽松 1200×800、空白创建表单、长中文名/多 issue 换行。六个关注状态的结构检查无裁切问题，已查看截图；此为设计结构检查，不是真机视觉验收。同步 `design/export.md` / `PLAN.md` / `specs/M4-W2.md`。

## 命令与共享选择

Enter 新增 export.enqueue；Esc 与 Mod+A 扩充现有 edit.clearSelection / edit.selectAll，保持已确认路线的键位，未另建同义取消命令。范围、预设保存、全部停止、重置接统一命令面板/标题栏菜单，默认热键留空：范围适合鼠标循环、保存是表单操作、停止暂未接执行、重置需要确认且避免误触。运行开关到 W5 实施时登记。默认冲突与保留键检查有单测。

Shift / Ctrl / 组合修饰键直接使用共享 clickMode / applySelection；照片组与日/时间片开关调用共享 toggleGroupSelection，焦点移动调用 focusSelection。崔总告知大肥鱼在并行规整选择逻辑，本波不改其共享算法或回归测试，导出仅做 issue 身份与分组适配。

## 已验证（单元与冒烟）

- pnpm test：1008 项全部通过。覆盖同照片多 issue、未加载页范围选择、128 分批/并发去重/晚响应、私有队列跨上下文、快照深拷贝、配置写失败/损坏/Unicode、无队列持久化、重启 off、仅真实 running 动效、可变行高、全屏 variant 与命令默认键。
- pnpm typecheck、lint:colors、lint:arch、lint:i18n：通过。
- cargo check --workspace：通过。
- cargo test -p raybend --lib：1138 通过，4 ignored；本波 export 六项测试覆盖缺源、基准角色、稿不存在、非空判据、快照不变/源变更、目录模板/Unicode/数值边界。耗时 21.66 秒，未使用真实大照片库。
- cargo test -p raybend-desktop --lib：90 通过，1 ignored；全屏向后兼容与可选 variant 契约通过。为已有并行 geometry_tests 补 rusqlite.workspace dev-dependency，复用已经锁定的依赖，无新框架。
- pnpm smoke:ui http://127.0.0.1:1420/ --export-only：工作区、双 TilesShell、空白表单、禁用执行、无错误闪烁与无未捕获异常均通过。此模式扩展既有 CDP 驱动，没有另建测试驱动。
- pnpm debug:win：最终前端构建及 Windows desktop / RAW worker 成功；内置 check:win 确认 4 个资源内容/时间与 worker 协议 raybend-worker-proto-v3，target 清理无失效产物。exe 时间 2026-09-26 16:57:13.571 CST，路径 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。构建仍有既有大 JS 分块警告。

## 未通过与边界

完整 pnpm smoke:ui 和 pnpm check:browse 本次未通过，不宣称整体 GUI 回归通过。完整 smoke:ui 出现主题、导入取消/摘要、看图返回与导航超时；check:browse 出现信息条强制显示层、预览 4:3 断言、标题栏/模态层级与假后端 EXIF 缺字段异常。导出启动单独复跑已通过；完整脚本的失败未经足够隔离，不能断言全是既有问题，也不在并行选择调整期间盲改共享交互。失败日志分别保存在 `/tmp/raybend-w2-ui.log` 与 `/tmp/raybend-w2-browse.log`，后续整合需继续处理。

此前导出启动出现 createMemo 提前读取 base 的初始化错误，已把摘要/数据源创建顺序修正，独立脚本复跑无异常；没有通过忽略控制台错误绕过。

## 待崔总真实环境验收

1. Windows 最小窗、紧凑/宽松密度、左右与上下把手、两个连续缩放和适合窗口，确认尺寸与节点状态不跳。
2. 真实 JPG/RAW 配对及 RAW-only，SOOC/latest/中文命名稿小图和全屏实际对应所选稿；超过四稿换行、长名可读。
3. 同一照片多 issue 一次入队；切目录/库/工作流仍保留各预设私有队列与计数，重复入队不重复。
4. 创建/同名更新预设、非法名称/模板/离线或不可写目录的提示；重启预设参数和展示偏好保留、队列清空。
5. 与大肥鱼那边的统一选择语义一起确认导出点选、Ctrl、Shift、日/时间片和 Esc / Ctrl+A / Enter。这里只接共享逻辑，不做另一套行为。
6. 本波执行按钮仍禁用、flowbar 不闪。实际后台跨工作流运行、空队列 on 不闪、编码精度/色彩/性能在 W3–W5 完成后验证。
