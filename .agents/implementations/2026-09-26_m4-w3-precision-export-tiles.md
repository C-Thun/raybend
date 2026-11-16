# M4-W3：高精度出图与导出 Tiles 修订
完成时间：2026-09-26 19:44:46 CST

## 范围与复用

按崔总暂停后确认的新设计实施上下 Tiles，并收完原 W3 修复与 RGB16 出图。不新建网格/选择/滑杆/缩略图算法；复用 PhotoGrid、Tile、VirtualGrid、行模型、TilesShell、共享选择、Dialog、命令注册表与既有 RAW worker。没有修改并行 Agent 的 lib/selection.ts 或编辑器修改。

## 结果

- 修正空态恒为真挡住网格、无法请求定稿的问题；非空数据始终挂载网格。库列表仅首次展示加载，稳定 ID 维护卡片身份，后台刷新不挤动目录；catalog 变更显式重取已知摘要，避免可见范围未变就不再取稿。
- 摘要增加 main/edited/createdAt：最新稿为主图，匹配命名稿时使用其引用/名称；RAW-only 可用原始渲染主图，非 RAW 基准小图区去重主稿。三个按钮仅筛照片，已入选照片完整显示其定稿。上部两列、最多六张小图，更多通过全部定稿弹窗；已选/入队优先、创建时间倒序、未选 SOOC 最后。行高取组内最大值。
- Tile 扩展静态名称、小图与主色选框/辅色锁框，方形区域保留图片比例。上下独立选择和辅色焦点；待处理点击只选择，按钮/Delete 显式移除；运行/完成锁定，状态按当前预设变化。顶部操作按可执行子集禁用。重置隔离正在取快照与选择的晚回调，已启动处理的晚状态不会重建队列。
- 共享 TilesShell 新增边长配置，默认128–320不变，范围内复用原预设档、非标准端点补档；导出上部240–320共六档，下部原十七档，滑杆连续。网格、滚轮、状态条、缩放命令、fit同一坐标；v1/v2→v3按物理尺寸迁移并抬高上部下限，v2保留筛选偏好。
- TIFF/PNG隐藏完整质量行，有损JPEG/WebP/AVIF显示并保留值；后端校验同步无损质量参数不生效。
- 入队与队列预览仍用不可变快照；预览跟随稳定资产身份解析当前文件位置。队列双击/全屏携带捕获快照，不因latest更新/命名稿删除而换稿。队列条目仍留在App内存，跨工作流存活。
- 修正共享缩略图请求自订阅错误：队列写入/失败不能让调用effect立即无限重试；请求读写在untrack内，失败等待显式请求或刷新。底部状态栏内容稳定创建；Dialog children/尾部在插入点untrack，避免选择变化重建弹窗内容。
- Rust唯一显影管线泛化u8/u16与浮点输入；空间阶段/LUT/锐化/几何不经过RGB8。共享display/output严格完整解码（RAW不以预览兜底）、镜头/曲线/降噪/动态反差/几何/缩放，RGB8在最终消费点量化。缓存管线13→14。
- 核心render_captured/encode_tiff16/write_tiff16与薄壳IPC：源选择、profile/签名执行前后校验，0原尺寸、只缩小不放大，RGB16无损TIFF，按资产角色跟随移动、不偷偷换源。源目标相同/非法路径拒绝；fs_atomic::write_new复用临时写入，原子hard_link无覆盖发布，冲突不破坏现有文件、仅清理自己临时文件。

## 命令与热键

复用edit.delete=Delete，在导出改为移出待处理定稿，并按上下当前焦点执行；export.enqueue=Enter。修复Tile吃掉Enter的问题，通过keyboardActivate配置保留双击看图并让工作区持有Enter。Ctrl+A/Esc继续共用选择命令，弹窗只授权Ctrl+A/Delete/Enter对应的既有命令，输入框与其他模态继续拦截；弹窗Esc关闭、取消选中走按钮。
缩放复用既有命令与快捷键配置，不新造键位；保存/三态/重置/全部停止保持defaultKey明确留空（避免误触、菜单可用），队列尚未执行时无运行命令。内部焦点、全部定稿弹窗不新增全局命令。

## 验证（冒烟）

- pnpm typecheck、pnpm test：1022前端单测通过，包括筛选/排序、独立选择、锁定/移除、预设切换、重置竞态、偏好迁移、尺寸边界/连续反函数、命令冲突与模态限制。
- cargo test -p raybend --lib：1145通过，4 ignored；16位渐变保留超过256级、几何/极端参数、RGB16 TIFF回读、源字节不变、同名并发写入、源移动/替换/缺失边界均有单测。
- cargo test -p raybend-desktop --lib：90通过，1 ignored；全屏可选快照/旧清单契约回归通过。
- pnpm lint:colors / lint:arch / lint:i18n、cargo check --workspace、git diff --check通过；pnpm build通过，仍有既有大JS分块警告。
- 新增可重复pnpm check:export，使用既有CDP驱动和带数据假后端：摘要按需取稿、三态实际筛除/恢复、最多六张/全部弹窗、点选/Enter入队/Delete移除、弹窗Ctrl+A/Enter/Delete、上下缩放范围、滑杆/外框/状态栏节点身份、fit Provider实际传达、TIFF/PNG质量隐藏/恢复、库目录刷新位置/节点不变、失败缩略图不自重试、中文定稿文案、未捕获异常与console.error全绿。
- pnpm smoke:ui http://localhost:1420/ --export-only通过；完整pnpm check:browse仍失败，信息条强制层/右栏预览比例/标题栏模态层级/假后端EXIF缺cameraMake，与W2记录相同，日志/tmp/m4-browse-smoke.log，不宣称整体GUI回归通过。
- 最终pnpm debug:win构建主程序+RAW worker并check:win资源、时间/协议通过：exe 2026-09-26 19:43:01.087 CST，worker协议raybend-worker-proto-v3；产物/mnt/c/rb-target/raybend/debug/raybend-desktop.exe。第一次构建期间源码更新导致worker时间检查拒绝，完成修改后完整重建通过，没有绕过检查。

## 涉及文件

核心develop/{sample,pipeline,lut,sharpen,geometry}、display/output、thumbnail/render、export、fs_atomic；薄壳export/fullscreen/contract；共享Tile/Dialog/TilesShell/TilesControlBar/fit/PhotoGrid/thumb-queue；export-model/export-prefs与工作区store/source/Toolbar/ExportWorkspace；BrowsePanels、App、commands、i18n、fullscreen契约；design/export.pen+md、专项回归脚本与单测。

## 仍未完成

W4五格式/命名/元数据与W5真实多预设后台执行尚未接入，开始导出按钮仍禁用。四预设开启上限、四执行线程与失败重试弹窗在W5落实，不将状态单测冒充实际运行。W6外部编辑器仍待实现。真实照片、色彩、Windows视觉/DPI/操作体验和性能归崔总完整流程验收。本轮没有发布、push或替换其他Agent改动。
