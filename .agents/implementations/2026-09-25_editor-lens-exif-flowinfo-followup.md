# 编辑器镜头自动匹配、完整 EXIF 与共享拍摄信息
完成时间：2026-09-25 20:58:34 CST

## 范围与文件

- `crates/raybend/src/lens/mod.rs`：自动匹配改为结构化镜头特征（英文厂商、定焦或变焦焦段、最大光圈范围、代数默认 1），结合镜头族名称与机身卡口筛选；人工查询保留原操作。缺光圈且同焦段有多支时不强猜。
- `src/features/editor/panels.tsx`：BM3D 后台任务显示辅色圆角状态块、转动图标和“高质量降噪中”的显式文案。
- `crates/raybend/src/media/exif.rs`、`media/tiff.rs`、`src-tauri/src/source.rs`、`src/api/{types.ts,db.ts,dto-contract.json,dto-contract.test.ts}`：文件级 EXIF 增加完整标签列表；标准格式由 kamadak-exif 枚举，RW2/ORF 由受限 TIFF 目录遍历兜底，保留未识别标签的十六进制编号，读取 GPS/Exif/Interop/IFD1。大型二进制载荷显示长度摘要，过长文本限制到 4096 字符，避免 IPC 与面板被 MakerNote 占满。
- `src/components/ui/MetadataRows.tsx`、`src/features/editor/panels.tsx`、`src/features/browse/BrowsePanels.tsx`：两处信息栏共用逐值 EasyCopy 行；编辑器将调节相关字段置顶并保留全部原始 EXIF，浏览右栏拍摄信息最多 384px 并内部滚动，不为调节字段另排序。
- `src/features/exif-strip/{selected.ts,from-file.ts,from-asset.ts,grouping.ts,types.ts,ExifStrip.tsx}`、`src/App.tsx`、`src/workspaces/{browse/BrowseWorkspace.tsx,editor/EditorWorkspace.tsx}`：统一当前选中文件内存信息源，索引值先显示、磁盘 EXIF 异步补齐，迟到结果不可覆盖新选择；配对 RAW 可补位图缺失的镜头信息。FlowBar 中相机品牌在机型左侧同组 EasyCopy，镜头另成一组，导入/浏览/编辑共用一套渲染。
- `design/{main,browse,editor}.md` 与同名 `.pen` 同步交互说明和三个工作区的 FlowBar 示例；编辑器补了高质量降噪等待态，编辑器和浏览信息栏补了完整 EXIF 示意。三份画稿均已由人类在 Pencil 中保存，并经文件修改时间复核。

## 决策与验证

- 当前信息源按“选中文件路径 + 已有索引元数据”接线。RAW 路径只通过现有 `develop_edit_target` 解析，前端没有另写 `_RAW` 路径规则。位图文件自己的 EXIF 标签仍来自该文件；配对 RAW 只补 FlowBar 缺失的字段。
- 真实样本 `/mnt/c/src/tmp/pic/P1000021.RW2`：Panasonic DC-G9 的 `DG Vario-Elmarit 12-60mm F2.8-4 Asph. Power OIS` 匹配到 `Panasonic LEICA DG 12-60/F2.8-4.0`；12mm F2 定焦样本未误匹配变焦。
- `cargo test -p raybend --lib lens::tests`：38 通过；`cargo test -p raybend --lib media::`：159 通过。并发改动稳定后复跑全量：`cargo test -p raybend --lib` 1057 通过、2 忽略；`cargo test -p raybend-desktop --lib` 81 通过、1 忽略。
- `pnpm test`：933 通过；`pnpm typecheck`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm lint:colors`、`pnpm build` 均通过；`git diff --check` 对本次涉及的已跟踪文件通过。机型自带厂商前缀时，FlowBar 只显示一次品牌，且有单元回归。
- `pnpm debug:win` 完成 Windows debug 构建，内置及独立 `pnpm check:win` 均核对通过前端资源、主 exe 与 RAW worker 协议。并发会话先后更新 `dist` 和 Rust 源码使两次中间校验报旧，按最新源码重建后独立复查已通过。最终 exe 时间为 2026-09-25 20:57:36 CST。产物：`/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。
- 本次为被动信息展示及自动识别规则，无新增用户触发动作；统一命令目录及默认热键不适用。无 schema 变更、无新增依赖。

## 遗留与人工确认

- 真实 GUI 的视觉正确性、降噪过程提示、滚动和 EasyCopy 交互仍由人类在 Windows 真机确认，Agent 未声称完成 E2E。
- Pencil MCP 的画布更新需在桌面应用中保存；三份画稿现均已由人类保存并由 Agent 核实写回。尝试使用 computer-use 自动保存时，`node_repl` 在初始化阶段报 `sandboxCwd is not a local file URI`，因此改用 Pencil 画布修改加人工保存。
