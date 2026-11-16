# M4-W4/W5：五格式导出与内存后台队列
完成时间：2026-09-26 20:52:15 CST

## 范围与结果

连续完成崔总授权的 W4/W5，并解决 W3 留下的 browse/UI 回归失败。五格式输出、模板命名、子目录、元数据、预设交换/预览及真实后台队列均已接入。没有实施 W6 外部编辑器，也未把 W7 真机验收判为完成。

- 唯一 RGB16 出图入口接 JPEG/TIFF/PNG/WebP/AVIF：TIFF/PNG 无损 RGB16，另三种在编码边界量化 RGB8；质量与最大边进入真实编码。WebP 复用官方 libwebp；AVIF 复用 ravif，缓存质量仍为 90/4:4:4，单编码器内部线程限为一条。
- 模板复用 import::template，重名共同使用 collision_stem 的 `_01` 规则。目标子目录逐层规范化，拒绝穿越/设备名/非法名称/源目标相同；并发输出不覆盖已有文件。fs_atomic 在 Windows 用 MoveFileExW 不覆盖发布，Unix 保留不覆盖发布，复用同目录临时文件与清理契约。
- 预设文件使用版本化严格 JSON，只包含参数，拒绝未知字段、未来版本、重复 ID/名称、超限/非法值；导入后重新校验本机目录，不自动开启。目标预览给出模板路径、实际渲染尺寸与元数据策略。
- Rust Engine 是运行状态唯一来源，App 生命周期订阅 export://state，切预设、目录或工作流不停止后台工作。每预设最老待处理先执行，最多四个预设开启、四项同时执行；空开启不占线程、不播放闪烁。完成记录保留到重置，队列、完成记录和开关均不持久化。
- 失败继续后续项，可单项/批量重试失败项；完成项锁定且不会重复执行。停止阻止下一项启动。重置立即清前端选择/状态并隔离旧 generation/revision，实际在途出图继续，其槽位到结束才释放，旧回调不恢复条目。
- 采用崔总的定稿哈希边界：保存原引用与 profile 哈希，执行前再次核对。定稿已变更、删除或照片缺失标 skipped 并继续；已失效项须重新入队。执行中源签名变化仍按错误处理，数据库/读取故障不能伪装成正常跳过。
- 右列增加预设导入/导出、预览、真实开始/停止及四预设上限提示；失败/失效清单复用 Dialog。队列错误显示原因，当前预设卡显示待完成/总数，flowbar 仅有真实 running 时闪烁。

## 元数据字段与格式矩阵

| 内容 | JPEG | TIFF | PNG | WebP | AVIF |
| --- | --- | --- | --- | --- | --- |
| 拍摄 EXIF / GPS、方向与成片尺寸 | APP1 EXIF | TIFF/EXIF IFD | eXIf | EXIF chunk | Exif item |
| 关键词、版权、作者、说明 XMP | APP1 XMP | tag 700 | iTXt | XMP chunk | MIME RDF item + cdsc |
| UTF-8 IPTC | APP13 Photoshop IRB | 对应语义写 XMP | 对应语义写 XMP | 对应语义写 XMP | 对应语义写 XMP |
| 像素 | RGB8 | RGB16 无损 | RGB16 无损 | RGB8 | RGB8 |

拍摄字段取源图主 IFD；方向统一为 1，尺寸更新，移除 MakerNote、旧缩略图、偏移与旧编码字段。关键词合并源图及库标签并去重；作者/说明优先库字段。Unicode/XML 转义、源 JPEG/TIFF IPTC、原生容器 XMP 回读与中文字段均有合成单测。PNG 压缩 XMP 解压上限 1 MiB。AVIF 元数据封装支持本项目现有编码器生成的 iinf/iloc 布局；其他未知布局明确报错，不声称支持任意外部 AVIF 重写。异常或扩展 IPTC 结构不能静默吞掉。

复用现有 kamadak-exif experimental Writer 写 EXIF/TIFF；直接暴露已锁定的传递依赖 roxmltree 0.20.0 和 flate2 1.1.10，用于 XML 与压缩 XMP，已补第三方许可记录，没有引入新顶层框架或大型依赖。

## 行为口径与命令

校对 PLAN「已确认的行为口径」，将三态照片筛选、latest 主图/两列最多六张/全部定稿、点击仅选择、可配置缩放、内存四预设队列与不中断重置更新为当前定案。取消选中只保留 Esc，范围控制段落原文保留。同步 design/export.md，移除失效章节引用；Pencil 补目标预览、失败重试、预设交换/开启上限状态，沿用已有样式与公共组件。

- `export.run` 默认 Mod+Enter；`export.enqueue` 继续 Enter，移出复用 Delete，清除选择复用 Esc。
- `export.preview/importPresets/exportPresets/failures` 接入命令面板/快捷键设置/菜单，defaultKey 明确留空：文件交换和检查弹窗不是高频动作，避免误触；现有保存/重置/全部停止键位决定不变。
- 全部定稿弹窗与焦点移动属于内部交互，不另造全局命令。
- 没有修改并行 Agent 的 `lib/selection.ts`，也没有接管其 Shift 规整工作。

## 回归修复

- browse 假后端补导出状态、EXIF 类型与事件注销契约；信息条断言先清选择/移出 hover。预览比例按 BROWSE 现行 3:1–3:4 夹取，标题栏层级按 DESIGN 的 scrim 60/titlebar 70/modal 80；撤销旧固定 4:3 与标题栏压过模态的假设，没有降低产品规则。
- export 冒烟接确定性的权威队列/事件假后端，新增空开启无闪烁、实际执行有闪烁、切浏览期间后台完成后仍保留/锁定、重置清权威状态；修正选图准备条件。
- 完整 UI 冒烟传根 URL 时先归一到 kitchen-sink，再跑既有应用壳检查；此前把根应用页当组件展厅造成假失败。
- 状态订阅失败仍载入预设并显示错误；乱序事件不回退，重置确认前也拒绝旧 generation，重置失败重新同步；入队按捕获 generation 分块，避免晚返回队列污染。

## 验证（单元与冒烟）

- `pnpm test`：1030 项通过；日志 `/tmp/m4-front-final.log`。
- `cargo test -p raybend --lib`：1154 通过、4 ignored，12.44 秒；日志 `/tmp/m4-core-final3.log`。新增导出合成数据测试覆盖五格式实际解码/尺寸/原生元数据回读、中文、RGB16 梯度、非法路径、并发不覆盖、失效跳过、四执行预算、旧重置回调、空开启、失败继续与重试幂等。
- `cargo test -p raybend-desktop --lib`：90 通过、1 ignored；日志 `/tmp/m4-shell-final.log`。
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`cargo check --workspace` 均通过。
- `pnpm check:browse`、`pnpm check:export`、`pnpm smoke:ui http://localhost:1420/` 完整通过，无未捕获异常/console.error；日志分别为 `/tmp/m4-browse-final.log`、`/tmp/m4-export-final2.log`、`/tmp/m4-ui-final2.log`。这是可重复的合成后端冒烟，不代表真实照片/GUI E2E。
- `pnpm debug:win` 最终构建与 `check:win` 通过；主程序与 RAW worker 一起构建，嵌入资源、dist 时间、worker 协议内容与时间均核对，未绕过任何检查。日志 `/tmp/m4-win-w4w5-final.log`。
- Windows 主程序 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`：2026-09-26 20:43:44.070 CST；worker 同目录 `raybend-raw-worker.exe`：20:43:02.755 CST，协议 `raybend-worker-proto-v3`；dist：20:42:13.413 CST。首次构建期间元数据源码继续修改造成 worker 时间检查拒绝，定稿后重建通过。
- `pnpm build` 通过，仍有既有大 JS 分块提示。Pencil 主右列与新增 Dialog 状态截图检查通过；交换按钮横排、预设列表限高，底部开始与提示均在画框内。

## 涉及文件

核心 `export.rs` 与 `export/{metadata,output,jobs,presets}.rs`、`fs_atomic.rs`、`import/plan.rs`、`thumbnail/render.rs`；外壳 `src-tauri/src/{export,lib}.rs`；前端 `api/{export,dialog}`、`lib/export-model`、`workspaces/export/{store,actions,ExportWorkspace}`、`App`、命令 catalog 与 i18n；对应单测、browse/export/UI 脚本；Cargo 依赖/许可、Pencil 画稿与说明、PLAN 与 `specs/M4-W4-W5.md`。此前 W3 与其他 Agent 的已有修改保留，没有将它们重写为本次交付。

## 待人类验收与剩余范围

真实 RAW/JPG、多定稿、真实目录、五格式色彩/元数据兼容性、Windows DPI/视觉交互和长队列性能，仍由崔总在真实产品环境验收。冒烟没有证明真实照片质量或内存体感。W6 外部编辑器、W7 包括千项资源基线与真机闭环仍未完成。未发布、push、打 tag，也未生成发布安装包；并行 M5 工作不属于本记录。
