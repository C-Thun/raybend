完成时间：2026-09-29 22:29:31 +0800

# tiles 小图读取、慢盘 import 与定稿缩略图生命周期

## 结论与根因

- browse tiles 原先已有共享 thumb → preview → 原片重建；本次保留刚完成的 browse 优化。注意选中一张仍为 flowbar/信息栏读取该张 EXIF，不能概括为“只有 view 才读任何元数据”；完整照片像素与元数据是两件事。
- editor 胶片带原先取 384；本次取 192。主视口保持真实 SOOC/RAW 线性源与原生 GPU 编辑，过渡帧仍按原规则。SOOC/RAW/latest 定稿卡片原先只是空 span，未接图片。
- export 的 variant 小图原先先建专用 1920 AVIF，再读回、缩小、再次编码。现复用既有 issue/latest/SOOC 小图及预览，避免为小格子额外制作整张 preview。
- import 原先 ready 等整目录 dirMetaEnsure，EXIF 头部读取及找不到时的兜底叠加全图解码；RAW 候选数组还会同时求值 thumbnail/preview，尺寸不足可能升级传感器解码。这些磁盘访问和计算比方向标签本身贵。
- 原有导入只排 thumbnail jobs，没有桌面消费入口；只有按需小图生成，SOOC preview 与第二档小图未在导入节点落实。

## 最终读取规则

| 场景 | 像素读取 |
| --- | --- |
| import tiles | 缓存小图优先；JPG 从 EXIF 提取内嵌 JPEG，没有才解主 JPG；RAW worker 先有界提取，再用兼容内嵌接口，禁止小图请求回退传感器解码 |
| browse tiles / browse、import film | 保留共享 384 队列；命中小图不读 1920；缺失可由有效 preview 缩小 |
| editor film / right 定稿卡片 | 192 thumb；卡片共用 ThumbQueue 和 variant API，latest 未编辑时显示 SOOC（RAW-only 用 RAW） |
| editor 主编辑区 | 原图真实解码与 GPU 渲染，未改 |
| export latest / issue / SOOC tiles | 共享 thumb → 已有 preview → 按请求尺寸重建，镜头/LUT 只在必要重建时解析 |
| export 原始 RAW | 真实 RAW thumb 优先；缺失时内嵌图 + 照片 img CSS brightness(0.72) saturate(0.35)；排序命名定稿 → SOOC → RAW |
| 实际导出 | 既有 render_captured / 输出管线，从实际源文件生成；CSS 和内嵌模拟不参与 |

JPG 不保证带内嵌图。Windows Shell 会先查持久化 thumbnail cache，JPEG/TIFF 也支持内嵌缩略图；方向可随缩略图处理，无需为方向解码全部照片。不能据 Explorer 已热缓存的表现承诺所有冷目录瞬间完成。
参考：
- https://learn.microsoft.com/en-us/windows/win32/shell/thumbnail-providers
- https://github.com/MicrosoftDocs/winrt-api/blob/docs/windows.graphics.imaging/bitmapdecoder_getthumbnailasync_241227233.md

## 生成、更新和失效

1. 导入拷贝/登记完成后，串行消费该库持久化 thumbnail jobs，从库内文件生成 SOOC 1920 preview + 384/192 thumb。初始 latest 显示复用 SOOC，不多存相同副本。RAW-only 仅备内嵌小图。
2. 当前编辑真帧就绪并停留 300ms 后，后台串行补原始 RAW 384/192。快速切过的未开始任务丢弃；只生成真小图，不另编码原始 RAW 1920。旧库缺 SOOC preview 在此入口补齐。
3. 编辑提交、进/出编辑刷新 latest preview 时，同时补两档最新 thumb；当前胶片带和定稿卡片更新 revision。保存命名 issue 沿用独立 preview + 两档 thumb。
4. RAW 真小图独立缓存键与源签名，不能被内嵌图冒充；API 先探测真实缓存，未命中后的模拟请求显式锁定来源，字节与 approximate 标记一起发布。加载/失败时旧图保留旧滤镜及尺寸，新图准备好后原子替换。
5. 源签名/编辑栈签名照旧失效。小图读取修订使用 `embedded-v1` 签名，保留管线 v14 与已有有效 1920 缓存，避免为了更快的小图作废整库预览。
6. app.db 首次开放前将中断的 running thumbnail jobs 退回 pending；下一次同库导入消费积压。按需 tiles/编辑入口仍可修复缺失图。没有新增启动时全库预热。

## import 调度与方向

- 扫描目录返回清单即 ready；不等待全目录元数据。Rust 在小图首帧前按方向摆正，WebView 解码后的宽高只作为显示比例；原片自然尺寸仍由 view/compare 按需取得。
- 共用队列取消未开始的离屏请求；clear 不再清零真实 inflight，换目录也遵守并发上限。已开始的 I/O 不假装已经取消。
- 显式按拍摄时间分组仍需读取内容，每次 IPC 8 张；关闭、重开或换目录后旧批次停止继续排队。
- JPEG APP1 提取遇 SOS 即停；TIFF 家族 RAW 有界头部 256KiB，IFD 链最多 16 层/节点，指定 JPEG 至多 16MiB；越界与截断回退兼容接口。方向与 TIFF 字节序复用既有解析器。
- RAW 请求新增 embedded_only，worker 协议升 v4；主程序和 worker 必须一起更新。

## 涉及文件与复用

- Rust 核心：`media/embedded.rs`（新增最小文件读取适配）、`media/tiff.rs`（复用既有 IFD 解析器）、`raw/{backend,rawler_backend,worker}.rs`、`thumbnail/{render,worker}.rs`。
- 外壳：`src-tauri/src/{thumbs,issues,export,import,develop,db,lib}.rs`，复用 FullCache、SourcesThumbs、持久化 jobs 与 catalog 租约，没有数据库 schema 改动。
- 前端：共用 `thumb-queue`、`Tile`、`tiles/source`、`photo-grid`；`api/{export,issues}`；editor/export workspace 与 editor panels；`export-model` 排序；`tokens.css` 模拟滤镜。
- 规格/记忆：`specs/tiles-disk-read.md`、`memory/FUNCTION-IMAGING.md`。
- Pencil：`design/editor.pen` 的 `JMkww` 定稿卡片；`design/export.pen` 的 `wLvOF` RAW 状态对照；同步同名 md。通过 Pencil 截图核对。未新增布局或交互。
- 命令体系：不新增命令或默认键；全部是既有读取、显示与后台缓存生成行为。
- 保留开工前已存在的 browse、迁移、发布等未提交改动，不推送、不发版。

## 验证

已验证（冒烟/单元）：
- `pnpm typecheck`；`pnpm test`：1135 通过。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：通过。
- `pnpm build`：通过，保留已有大 bundle 提示。
- `cargo test --workspace --lib --offline`：核心 1230 通过 / 6 ignore，桌面 97 通过 / 1 ignore。
- `cargo check --workspace --offline`：通过；现有 store/develop.rs 测试 unused_mut 警告与本次无关。
- `pnpm smoke:ui http://127.0.0.1:1430/`：通过，problems=[]。
- 单测覆盖：只有内嵌 JPEG/没有有效主图仍能显示、8 种方向尺寸、微小内嵌 RAW 不需传感器/机型元数据、中文路径、截断/越界/无 EXIF、按库认领、500 张清单不触发全目录元数据、时间小批次取消、切目录真实并发、离屏取消、真实/模拟来源及刷新中的滤镜原子状态、RAW 排在 SOOC 后。
- `pnpm debug:win` 最终增量构建与内置 `check:win`：通过，主程序 2026-09-29T14:25:59.551Z、RAW worker 2026-09-29T14:22:20.011Z（协议 v4），4 个 dist 资源引用全部命中。产物 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。

- Windows debug 已启动，日志确认 app.db 正常打开；出现“闪屏已露满 4 秒但前端仍未报就绪，按兜底显示主窗口”提示。没有把进程存活或这一兜底提示当作窗口视觉验收；未捕获到 Rust panic。浏览器冒烟服务 1430 已停止，Windows debug 窗口留给人类验证。

未经人类验证/限制：
- 旧机械硬盘真实照片库冷启动、快速拖动、方向与颜色目视、RAW 模拟观感；测试不等同实际磁盘性能。没有承诺“快如闪电”的量化倍数。
- 没有内嵌图的 JPG 仍需读取解码主图；部分 RAW 的兼容内嵌接口可能比标准 TIFF 指针提取读更多数据。没有可用内嵌 RAW 图时保持占位，不升级到完整 RAW 解码。
- 后台已开始的一次 RAW 真图生成不能中断；未开始的旧选择跳过。导入收尾生成是后台任务，导入成功消息不表示全部预览已经编码完。
- 缓存任务失败有原有退避/重试计数；进程关闭后不会自动全库预热，后续同库导入或按需入口恢复。
