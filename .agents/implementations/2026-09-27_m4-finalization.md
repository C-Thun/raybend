# M4 最终右栏、browse 显示定稿、外部编辑与 W7 收口
完成时间：2026-09-27 01:02:03 CST

## 授权与范围

崔总要求先修右栏与保存门槛，随后连续完成 M4；追加要求外部唤起通过简单 adapter，为多系统预留。最终任务单 `specs/M4-finalization.md` 已完成开发和 Agent 冒烟，真实环境验收单列。保留其它 Agent 的 M3/M5/选择语义改动，未修改 `src/lib/selection.ts`，未 push/tag/发布/生成安装包。

## 实现与关键决定

- **导出右栏**：上部预设列表 flex 填满余高，只有溢出滚动；下部设置按内容紧缩沉底，极矮窗口内部滚动。紧凑新增/变更 + 弹性开始/停止同行，下面保留一条保存提示。未保存、名称防抖未完成、设置校验/保存忙时不能开始；store 动作与统一命令同样拦截，已经开启的预设仍可停止。名称输入 1.5 秒防抖、点选即载入沿用前轮定案。
- **browse 显示定稿**：拍摄信息上方下拉；选择只存 browse 组件内存，不提交编辑栈、不调用 issue_select、不写 latest。切目录/换库/卸载/重启恢复 latest。共享 TilesSource 增加 imageKey/exportVariant 的可选数据适配，网格/胶片带/查看器/全屏共用该图片身份与捕获快照；尺寸与直方图由 Rust 完整定稿渲染取数。
- **外部入口**：仅 browse toolsbar right。统一命令 `browse.externalEditor` 属文件菜单，只有 browse 上下文可见/启用；defaultKey 明确留空（弹窗、秒级出图、非高频，不抢已有键）。定稿下拉属组件选择，不额外登记全局命令；弹窗确认/取消和候选勾选同理。
- **简单平台 adapter**：核心 `external_editor::Platform` 统一 discover / available / launch；Windows 注册表与唤起在 `windows.rs`，界面与 TIFF 管线不依赖 Windows API。复用已有 windows-sys，不新增 opener/大依赖。Windows App Paths 四组合 HKCU/HKLM × 32/64 位视图 + 九候选的有界已知路径；每目录最多读 2048 项、每路径模式最多保留 256 条，不全盘扫描。Windows 路径按大小写/分隔符去重，POSIX 保留大小写和字面反斜线。macOS/Linux 的发现留给各自 adapter，已有 POSIX 直接 argv 后备。
- **应用与目录**：设备设置 `external-editor.preferences.v1` 保留应用列表、上次应用和上次目录；版本不认识/损坏不静默覆盖。序列化写入、失败回滚；初始载入期间禁改，避免擦除已有登记。发现候选勾选加入、已添加禁用重复、便携版 exe 手动兜底；启动检查卸载残留，实际执行失效时报错并清登记、选择下一项。
- **TIFF 单向交接**：共享全尺寸 RGB16 渲染、几何、元数据、源签名检查与不覆盖原子发布。每次捕获当前显示定稿，写出原尺寸 TIFF，重名追加序号；ShellExecuteW 独立引用一个 TIFF 参数，不经 cmd/bat。已保存但打开失败时保留路径回执；原源不改、产物不登记 catalog、不自动取回、不新增 Gallery。
- **后台作业**：Rust 独立线程，App 根层弹窗状态，关闭弹窗/切 flow 不中止；准备/渲染/写出/唤起仅阶段信息，无伪百分比。取消在阶段边界合作生效，不强行中断渲染；已经发布的 TIFF 保留。执行槽原子预留、任务 id/revision 隔离旧回调、捕获 panic 转失败；更新安装保护检查该作业。任务/输出状态不持久化。

## 回归与工具问题处理

1. 共享浏览回归发现新增同步跟踪整个分页源时，短暂空页会关闭查看器。改为只在显示定稿 choices 变化时同步查看器，分页/元数据刷新保留视图。查看器单测验证同一文件不同定稿键重新取图、不改文件路径，未变定稿保留缩放；完整 browse 回归覆盖该实际调用链。
2. 外部 DOM 假后端最初缺少 FileExif.tags，按真实 IPC 形状补为数组，避免把坏 fixture 当产品问题。Vite 冷模块载入在并行编译时偶尔超过 20 秒，冒烟启动等待改为 60 秒，超时停止后续断言，不再产生满屏连带假红。最终功能回归在生产 dist preview 上验证。
3. Pencil MCP 已同步 export 四个右列状态、去除旧预设交换/预览/完成摘要浮层；browse 下拉置顶、toolbar right 入口和两个外部弹窗入稿。MCP 修改不自动落盘；computer-use 因 WSL cwd URI 失败不可用。请崔总保存后核对 browse.pen/export.pen 的文件大小与 00:46 文件时间。弹窗置于 browse.pen，配对 browse.md，不另造一个不能落盘的 external-editor.pen。
4. 一次把完整 smoke:ui 指向生产 preview 的尝试失败，因为该脚本必须先测 `/dev/kitchen-sink`，生产构建不包含开发陈列室。随后按正确开发入口重跑并通过；生产 browse/export/external 三条独立回归均通过。这不是产品遗留失败。
5. 整仓 cargo fmt --check 被其它并行改动的格式挡住，本次 Rust 文件单独 rustfmt；未重排他人的文件。生产构建现有大 chunk 提示保留，不影响编译与产物核对。

## 文件

- 右栏与开始门槛：`src/workspaces/export/ExportWorkspace.tsx`、`store.ts`及 store.test.ts、语言包、check-export-boot.mjs。
- 显示适配：`src/lib/display-variant.ts`、`src/features/browse/display-variants.ts` 和测试，BrowseWorkspace/BrowsePanels/ViewerReadout，共享 tiles/source、PhotoGrid、FilmStrip、viewer/photos/store 及 viewer 测试。
- 外部交接：`crates/raybend/src/external_editor/{mod,windows}.rs`，`src-tauri/src/external_editor.rs`、共享 export metadata/details glue，`src/api/external-editor.ts`、`src/lib/external-editor.ts`、`src/features/external-editor/` 与测试，App/actions/catalog/update 保护。
- 验证与文档：核心 export/batch_smoke.rs、check-external-editor.mjs、check-browse-boot.mjs、package.json，Pencil export.pen/browse.pen 与同名说明，PLAN/FUTURE/BROWSE、当前 specs 与本记录。

## 已验证（Agent 冒烟）

| 检查 | 结果 |
| --- | --- |
| pnpm test | **1059 passed / 0 failed**，4.4 秒 |
| pnpm typecheck + lint:colors + lint:arch + lint:i18n | 全通过 |
| pnpm build | 通过，生产 dist 最新；仅已有 chunk 大小提示 |
| cargo test -p raybend --lib | **1164 passed / 0 failed / 5 ignored**，21.49 秒；后续平台路径调整的 6 个外部测试再次通过 |
| cargo test -p raybend-desktop --lib | **95 passed / 0 failed / 1 ignored**，0.08 秒；含并发启动槽、取消等待、旧回调隔离与溢出边界 |
| cargo test -p raybend one_and_thousand -- --ignored --nocapture | 1 与 1000 张 8×6 RGB16 PNG、四预设共享真实输出：通过。1 张 10ms；1000 张 2505ms（一次模拟磁盘写满后重试成功），源 SHA256 不变、重复入队幂等；进程 VmHWM 约 12.8 / 31.8MB。不能用小图数据推断真实 RAW 耗时/内存或长期泄漏。 |
| Windows cargo test -p raybend --lib external_editor | **7 passed**，含 argv 的空格/Unicode/引号、执行别名模式、路径登记、取消和 RGB16 TIFF 源哈希 |
| Windows cargo test -p raybend --lib export::output::tests | **2 passed**，五编码精度/元数据、非法路径及同名并发发布 |
| pnpm migrate:drill /tmp/raybend-m4-migrate-20260927 | 8 阶段通过：快照/损坏拒绝/恢复/旧 schema 升级/轮转/未来版本拒绝，不动用户库 |
| pnpm check:browse | 开发入口及 APP_URL=http://127.0.0.1:1421/ 生产入口均通过；含滑块连续跟手/节点身份与适合窗口 context 两面、查看器/胶片带/对比 |
| pnpm check:export http://127.0.0.1:1421/ | 通过；有数据/分页、稳定节点、选择/队列移除/重试、质量/缩放、保存门槛与右栏高度 |
| pnpm check:external http://127.0.0.1:1421/ | 通过；显示选择零 latest 写入、应用登记门槛、捕获定稿、关闭/切 flow 后后台完成、返回 browse 默认 latest |
| pnpm smoke:ui http://localhost:1420/ | 通过，problems=[]；开发陈列室 + 壳 + 全屏假后端 |
| pnpm debug:win / pnpm check:win | 最终均通过；exe 2026-09-27 **00:58:44 CST**，dist 00:57:17，4 引用资源命中；worker 00:50:23，含当前 raybend-worker-proto-v3 且不比相关源码旧 |
| git diff --check | 通过 |

日志在 `/tmp/m4-final-*.log`；构建产物 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。以上是编译/单测/合成 API 与 DOM 检查，未把这些声称为真实 GUI E2E。

## 真实环境尚未验收

崔总从最新 Windows exe 跑完整导入→浏览/评级筛选→编辑/定稿→导出流程：三照片形态与多定稿四格式，原尺寸/百分比/最大边/元数据/几何，回焦点和切 flow 的上部稳定性；右栏最小窗/两密度/DPI，四预设持续运行/停止/重置与失败原因，退出后的内存队列丢弃；browse 显示切换不改变 latest，当前定稿送外部 RGB16 TIFF，照片画质/颜色/真实耗时和原源哈希。

Windows 当前支持传统 exe 路径和实际存在的 MSIX 执行别名，不枚举包清单或创建 bat。九候选的各版本实际启动/打开 TIFF 尚未验证；MSIX 无别名等发现不到的安装可手动选择，启动失败仍保留 TIFF 路径。macOS/Linux 自动发现尚未实施，adapter 边界已预留。Gallery/自动取回、ICC/XMP 等未获批准的扩围保持未实施。
