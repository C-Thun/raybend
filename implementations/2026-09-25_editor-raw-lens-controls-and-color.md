# Editor RAW 缩略图、镜头、信息与控件修正
完成时间：2026-09-25 15:54:31 CST

## 范围与原因

- 用户截图（`P1000019.RW2`）显示主视窗是暗一些的 RAW 显影，总览与胶片带是明亮的机内 JPEG。根因在 `thumbnail/render.rs`：带编辑栈的 RAW 缩略图仍调用允许内嵌预览的 `DecodeSpec::thumb`，随后把参数套在 SOOC JPEG 上。现在带编辑栈时调用 worker 的线性 RAW 解码（限制返回长边，且禁用内嵌 JPEG），用与位图共用的 `apply_develop_linear` 跑管线。缩略图缓存签名从 v7 升到 v8，旧图会重建；未编辑的浏览缩略图仍可走内嵌预览。`thumb-probe --edited` 可复跑这一路。
- 高光正向调整原先逐通道执行，亮蓝通道可能得到更大的增量，使高光泛蓝；超出 1 的通道执行曲线也会产生不正常的负增量。曲线现在对越界值保持原样；正向高光额外用无高光调整的基线计算统一亮度增量，已达白点的像素保持原色。黑区曲线保留原有单调形状并验证边界。
- 曲线底纹原先把未经归一化的直方图计数直接送入 0..1 SVG 路径，几乎所有峰值都被夹到顶部；现在归一化各通道，并在 Rust 直方图里增加真实 Rec.709 亮度分布供 RGB 曲线底纹使用。
- `CompactChoice` 统一直方图、曲线与降噪方式的小型选择样式；降噪方式与标题同一行。影调、色彩、清晰度、镜头面板底部的统一重置与说明移除；toolsbar 右端提供重置修改并弹确认框。SOOC/RAW 放在 toolsbar 左端，其后是撤销/重做；缩放控件的位置保持不动。SOOC/RAW 在统一命令表各有命令，无默认热键，以免输入中误切；重置沿用既有命令。Pencil `design/editor.pen` 和同名说明同步。
- 色温拉杆按本张 RAW 的 as-shot 色温初始化，重置也回到该基线，显式选择与基线相同值时仍能区分。信息页按调整相关、拍摄、文件排序，只展示原始数据或 RAW 拍摄白平衡基线；增加解析器已有的软件、GPS、原始时间，最高 24rem，内部滚动。镜头元数据优先从 RAW 的 MakerNote/rawler 元数据补齐，旧目录记录也能按需补读；真实样本识别出 `M.Zuiko Digital ED 12mm F2.0` 并匹配 lensfun。编辑时 flowbar 与信息页使用该结果。
- 镜头配置改成可搜索、可滚动的对话框，列自动、无、相近焦段且同变焦类型的推荐及全部同卡口候选。移除长段说明。手动畸变、暗角、色差在参数契约中启用，配置开关关掉后仍可独立调节；配置、开关和降噪方式改变后立即落库。只关闭自动镜头校正也算可见编辑。
- 重置时的视窗错位可由布局位置变化但尺寸不变触发：原 `ResizeObserver` 只报尺寸。视口现也在工作区布局变动、滚动后按下一帧上报矩形；上报器去重，位置不变时无额外 IPC。Windows 窗口移动的 native redraw 通路仍保留。

## 主要文件

`crates/raybend/src/thumbnail/render.rs`、`crates/raybend/src/develop/pipeline.rs`、`crates/raybend/src/display/histogram.rs`、`crates/raybend/src/raw/{rawler_backend,worker}.rs`、`crates/raybend/src/lens/mod.rs`、`src-tauri/src/{source,lens,thumbs}.rs`、`src/features/editor/{panels,store,toolbar,viewport,lens-options}.ts(x)`、`src/features/commands/catalog.ts`、`src/workspaces/editor/EditorWorkspace.tsx`、`src/App.tsx`、`src/components/ui/CompactChoice.tsx`、`design/editor.pen` 与 `design/editor.md`。同步更新 DTO、i18n 与对应单元测试；catalog 第 7 版迁移的旧测试断言更新至新版本。

## 验证

- `pnpm typecheck`：通过；`pnpm test`：917 通过；`pnpm lint:arch`、`lint:i18n`、`lint:colors`、`git diff --check`：通过。
- `cargo test -p raybend --lib`：1011 通过，1 ignored；`cargo test -p raybend-desktop --lib`：78 通过，1 ignored。
- `cargo run -q -p raybend --example thumb-probe -- /mnt/c/src/tmp/pic/P1000019.RW2 --edited`：384×288 AVIF，有效且非占位图。
- `target/debug/examples/lens-probe /mnt/c/src/tmp/pic/P1000019.RW2`：读出上述镜头，匹配 Olympus 12mm f/2.0，解析出畸变、色差和暗角系数。
- `pnpm debug:win`、内含 `pnpm build` 与 `pnpm check:win`：通过；主程序和协议 v3 RAW worker 均为新产物，路径 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。

## 遗留与人工验证

用户留下的 `/mnt/c/src/tmp/rb-editor.log` 大小为 0，无法读到前次 stderr 诊断。Windows 事件记录 2026-09-25 11:53:38 的 `raybend-desktop.exe` 崩溃，异常码 `0xc0000409`、附加码 `7`，现有 WER 报告没有调用栈，故不能把崩溃归因到某一函数。缩略图线性解码返回尺寸已受限，降低主进程内存峰值；这不是对崩溃已修复的证据。

GUI 视觉与交互、窗口拖动及多显示器 DPI、真实照片库下的性能和这次崩溃是否消失，依项目分工由人类在 Windows 真机复验。建议用新 exe 重做切换照片、高光增减、镜头配置搜索和手动微调、重置后的洞口位置与拖窗；复现崩溃时保留 stderr 和 WER 报告。全仓 `cargo fmt --all -- --check` 仍会列出未涉及文件的旧格式差异，本次未全仓改写。
