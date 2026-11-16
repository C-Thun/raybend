# 镜头查询响应式循环修复与显式配置选择
完成时间：2026-09-26 02:45:15 CST

## 根因与证据

本次在新构建的 Windows 版上先查询真实照片仓资产，再改代码。基线 `lens_match` IPC 已成功返回 `ready=true`、1447 个去重候选、`Panasonic|LEICA DG 12-60/F2.8-4.0`，因此不能把旧构建或库缺失当作此次根因。内置库原始条目为 1543 支镜头、1041 台机身；候选数较少是稳定键去重。

`EditorWorkspace` 在接到镜头结果后通过 effect 调用共享元数据的 `enrich`；旧 `enrich` 同步读取 `fallback()` 再写入新对象，意外使调用 effect 订阅该信号并反复触发自身。随后旧查询 Promise 的 catch 又把前端响应式异常当作 IPC 失败，清空匹配结果；UI 把 null 当作加载中，于是搜索候选一起消失。

新增回归使用 Solid 的 browser 条件导出运行真实响应式 effect（默认 Node 导出不执行 effect，无法抓到此问题）。修复前明确触发 `metadata enrichment subscribed to itself`；修复后同一结果只回填一次，无关元数据变化不再触发镜头 effect，重复补充相同值不重新发布。

## 改动

- 复用共享 `src/features/exif-strip/selected.ts`，通过函数式 setter 无订阅读取旧值，并忽略内容相同的补充。没有再造编辑器专用元数据源。
- 新增 `src/features/editor/lens-query.ts` 作为唯一镜头请求状态：idle/loading/ready/error；错误保留原因、15 秒超时、显式重试、并发合并、切图/卸载作废迟到结果、刷新失败保留已有候选。成功发布移出传输异常 catch，避免再掩盖前端错误。
- `EditorWorkspace.tsx` 不再进入照片就请求并隐式应用镜头。打开配置弹窗才刷新推荐，最多 8 支；选择前不改编辑栈。按钮未选择时显示“选择镜头配置文件”，选择后显示品牌和型号，上方继续显示当前应用状态。已保存配置可由稳定键直接显示名称，不必等新请求。
- 工具栏增加“自动调整”，目前自动能力为根据拍摄信息匹配并应用镜头。按钮提示说明这一点；匹配成功保存具体 key、启用校正，失败或未匹配不覆盖已有选择。请求期间切图、切换编辑源、手选镜头时不覆盖新状态；请求代次防止离开后返回同一照片的旧任务误写。
- `editor.develop.autoAdjust` 接入统一命令面板与编辑菜单；默认热键明确留空，避免误触改变光学校正。弹窗重试属于局部交互，不登记独立命令。
- `crates/raybend/src/lens/mod.rs` 的缓存只永久保存成功结果；加载失败保留具体原因，显式查询可重试，冷加载由互斥锁串行化。增加失败恢复、并发只加载一次测试。
- `src-tauri/src/lens.rs` 记录查询进入、库就绪、元数据警告、完成/失败与耗时。元数据失败不再遮住独立可用的全库候选；`warnings` 进入 Rust/TS DTO 双边契约测试。
- 配置选择的 null/None 明确表示未选择，不再默认自动套用。手选和自动调整均保存具体 key；无配置时不为渲染额外读 RAW 镜头信息。显影预览缓存管线版本升级为 12，避免沿用旧隐式校正生成的缓存。旧 null 记录之后按未选择处理，这是本次交互语义调整。
- `design/editor.pen` 已用 Pencil MCP 更新配置按钮、弹窗、状态和自动调整入口；配套 `design/editor.md` 已同步。崔总确认保存，磁盘时间为 2026-09-26 02:31:46 CST。

涉及文件还包括 editor 的 store/actions/toolbar/index、App 命令接线、语言包、lens-options 与测试、API 契约和相关参数文档注释。

## 验证（冒烟与单元测试）

- `pnpm typecheck`、`pnpm test`：948 项通过。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过。
- `cargo test -p raybend --lib lens::`：41 项通过（含镜头数学、匹配、缓存与显式应用边界）。
- `cargo check -p raybend-desktop`：通过。
- `cargo test -p raybend-desktop --lib contract::`：30 项通过。新增 DTO 最初漏登记契约覆盖清单，测试捕获后已补齐并重跑通过。
- 修改涉及的 lens / contract Rust 文件单独 rustfmt；全包 fmt 检查暴露了其它在途文件已有格式差异，没有批量格式化整个共享工作区。
- `pnpm debug:win` 完成前端生产构建、Windows 主程序与 worker 构建、产物核对；随后 `pnpm check:win` 再次通过。主程序时间 2026-09-26 02:40:06 CST，worker 02:39:11，包含协议 v3。产物：`C:\rb-target\raybend\debug\raybend-desktop.exe`。
- Vite 保留已有的大 chunk 提示，无构建错误。所改文件 `git diff --check` 通过。

### 可复用的 Windows IPC 探针

新增 `scripts/check-lens-ipc-win.mjs`，复用 `scripts/lib/cdp.mjs`，只调用只读 IPC，不驱动 GUI。共享 CDP 工具增加可选页面过滤，以明确选择主窗口；首次探针连到启动闪屏，闪屏销毁后请求响应丢失，此工装问题修复后复测通过。

实际照片（相片仓 `00LjOuNLBTyYccxF`，资产 1）结果：ready=true、1447 个候选、检测到 `Panasonic|LEICA DG 12-60/F2.8-4.0`，RAW 镜头名为 `DG Vario-Elmarit 12-60mm F2.8-4 Asph. Power OIS`，warnings 为空。主窗口复核耗时 74 ms（worker 已热），启动首查后端日志为 1227 ms。

不存在的资产 -1：ready=true、1447 个候选、detected=null，并有“读拍摄参数失败：数据库错误：Query returned no rows”警告；耗时 7 ms，证明元数据失败仍可检索镜头库。

```bash
pnpm exec node scripts/check-lens-ipc-win.mjs --launch 00LjOuNLBTyYccxF 1 '--expect-profile=Panasonic|LEICA DG 12-60/F2.8-4.0'
pnpm exec node scripts/check-lens-ipc-win.mjs --launch 00LjOuNLBTyYccxF -1 --expect-metadata-warning
```

本机输出位于 `/tmp/raybend-lens-ipc-after.json` 与 `/tmp/raybend-lens-ipc-missing-asset.json`；启动日志为 `/tmp/raybend-lens-ipc-win.log`。探针支持外部 CDP 主机/端口环境变量；`--launch` 会在结束时关闭它启动的窗口。

## 尚需目视确认

没有进行 GUI E2E。配置弹窗搜索/选择、“自动调整”之后的实际校正画面、重进编辑器后的显示，以及失败状态布局，仍需崔总在 Windows 真实使用中确认。本次没有发布、推送或提交混有其它会话改动的工作区。
