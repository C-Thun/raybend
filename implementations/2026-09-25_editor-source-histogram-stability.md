# 编辑来源、直方图与视口稳定性
完成时间：2026-09-25 14:49:58 CST

## 范围

- 总览的 SOOC/RAW 切换保留其它调整参数和曲线；镜头配置、镜头开关按两侧各保留一份内存状态。缩放控件依照本轮最新要求保持原位。动态反差移到影调第一根拉杆，设计说明同步到 `design/editor.md`，Pencil 画布中的主稿与调整面板也已在编辑器里调整。
- catalog v7 通过现有迁移框架给 `develop_stacks` 增加 `source_base`，旧栈按 RAW 文件可用性推断来源；签名、撤销与前后端 DTO 均包含来源。`thumb_get` 与大图 latest 预览按这一来源选源并渲染。来源查库和 lensfun 解析放进后台线程。
- 切图时先读旧编辑栈与来源，再送首帧显影，避免 RAW→SOOC 重复解码；编辑栈尚未读取时禁用编辑。GPU 过渡帧沿用原有链，窗口移动事件会请求 surface 重绘。
- 参数拖动只更新 GPU 视口。松手后串行落库、生成 latest preview，再刷新当前照片的胶片带与总览；缩略图队列保留旧 URL 直到新 URL 可解码，并优先加载当前照片。撤销/重做会作废受影响的 latest 大图缓存并重取当前缩略图。
- 直方图从显影管线最终 RGB 帧计算；RAW/SOOC 切换后更新，拖动中保留前一份，松手真帧更新。曲线背景与总览共用此结果。

## 验证（Agent 冒烟）

- `pnpm typecheck`、`pnpm test`：912/912，通过。
- `pnpm lint:arch`、`pnpm lint:i18n`、`pnpm lint:colors`、`pnpm build`：通过。
- `cargo test -p raybend --lib store::develop::tests`：23/23，通过；`cargo test -p raybend-desktop --lib`：78/78 通过，1 个真照片测试按项目约定忽略。
- `pnpm debug:win`：Windows exe 与 dist、RAW worker 协议检查通过，产物 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。
- `cargo fmt --all -- --check` 显示大量全仓既有格式差异；本轮未对全仓施加格式化，避免污染其它工作。

## 崩溃证据与遗留

- 用户本轮的 `/mnt/c/src/tmp/rb-editor.log` 大小为 0 字节。Windows Application Error 在 11:46 与 11:53 记录 `raybend-desktop.exe` 异常 `0xc0000409`，没有可用调用栈；无法据此判定具体崩溃点，也不能声称已修复崩溃。
- 拖窗镂空、切图体感、色彩与 RAW/SOOC 实际取图须由人类按项目约定做 Windows 真机 E2E。若复现仍崩溃，应以前台运行保留 stderr 或收集崩溃转储。
- Pencil MCP 已同步主稿与调整面板组件帧中的动态反差顺序；`design/editor.pen` 与同名 `design/editor.md` 均已写盘。
- 本轮只调整了现有 SOOC/RAW 控件与拉杆顺序，无新增独立用户命令；命令注册表与热键不增项。
