完成时间：2026-10-04 03:06:09 CST

# XMP 系统审查与修复

## 范围与依据

按崔总要求核对现有 XMP 写出、读回、生命周期和导入接线，并直接修复确认的问题。依据为 `specs/xmp-sidecar.md`、`specs/issue-xmp-contract.md` 与 `implementations/2026-09-30_xmp-sidecar-w1.md`。数据库继续是真相源，sidecar 用于开放表达与自家成果恢复；沿用冻结的跨家导入边界。

仓库在本轮开始时已有大量未提交的 XMP、色彩管理和相片整理改动，期间其它会话继续修改共享文件。本轮保留这些改动，在当前接口上修复，未提交或推送。

## 确认的问题与修复

1. **空 SOOC 工作副本丢失编辑源**：`DevelopStack::is_empty` 表示无像素调整，不能据此省略明确选择的 SOOC。写出现在保留其 latest 和 sourceBase；标准 crs 层仍仅表达 RAW 基调整。
2. **文字换行丢失，非法 XML 字符遗漏过滤**：共享 escape 原先过滤所有控制字符，把合法换行、回车、制表符也去掉，却留下 U+FFFE/U+FFFF。改为按 XML 1.0 合法字符处理，以数字实体保留文本和属性中的空白。导出内嵌 XMP 复用同一函数。
3. **清空会删除新版或损坏的自家 sidecar**：移除原先只判断归属。现在发布与清空共用可替换判据，未知版本、非法版本号、坏编码/JSON/哈希、未知资源均保留原文件并记录。无效 UTF-8 仍按命名空间识别自家文件，避免误备份接管。
4. **归属只看首个 Description 和已知元素**：按实际 rb 命名空间识别文件，合并当前照片的多个 RDF Description；其它 subject 的自家资源、未知结构留下警告并阻止覆盖。命名空间声明和纯元数据标记不会被误判成外来文件。
5. **大小写规则只用于读回**：统一 resolve_sidecar，读回、发布、备份、清空和回收都复用同一实际文件路径。没有精确路径而存在多个折叠同名文件时返回歧义错误，避免任意选一个覆盖。回收侧将 sidecar 路径错误记录后保留，不阻塞已经成功回收的照片记录清理。
6. **缺失位图抢在在线 RAW 前面被选中**：代表文件改为在线位图、在线 RAW、缺失位图、缺失 RAW 的顺序；补齐根目录相对文件名、空名称和长 Unicode 路径边界。
7. **编辑栈采纳校验不完整**：原外壳只认参数范围和曲线通道，漏掉控制点、LUT 配对、基础曲线快照和几何。抽出并复用 DevelopStack::validate，交互保存与 issue 导入使用同一规则；解析逐条跳过非法编辑而保留有效元数据，创建时间无效的 issue 不再悄悄变成 epoch 0。
8. **latest 和 issue 对 sourceBase 的处理不同**：先验证原 JSON 的指纹，再以资源声明对齐 sourceBase，两个路径采用同一口径。
9. **缺省元数据清空已有列，说明恢复后搜不到**：元数据落列只覆盖提供的值，rating=0 不清除原评级；说明更新同时刷新 FTS。
10. **采纳存在竞态和半份恢复**：业务移入核心 xmp/adopt.rs，在同一 catalog 事务内核对资产仍为空并恢复 latest、issue、元数据和标签关联。已有标记、文字、标签、SOOC 或自动调整基线均阻止覆盖；任何数据库错误整体回滚。文件读取、app.db 词典对齐与资源探测均在该事务外。
11. **读取错误被当成空数据写出**：元数据查询错误向上传递，标签内容读取失败不会再生成缺字段的镜像；异步同步保留原文件并记录失败数量，下一次动作触发重试。
12. **照片回收失败仍动 sidecar**：只有照片全部成功回收才移动自家 sidecar，动作前核对库会话。失败照片的恢复资料与记录保留，外来文件不动。
13. **导入恢复后使用旧 catalog 句柄采纳**：导入结束使用 TaskCatalog.current()，采纳、计数刷新和会话观察指向恢复后的同一 catalog。
14. **缺 LUT 引用没有采纳诊断**：复用应用 LUT 路径入口，记录缺失资源，保留稳定 ID 和完整 profile，等待资源补齐。

15. **JSON 浮点读回产生指纹漂移**：全量测试实际复现 exposure 的有效小数在往返后相差 1 ULP，导致 issue 和自家 XMP 校验失败。现有 serde_json 启用 float_roundtrip 特性，保持序列化格式与哈希算法不变，精确读取已写出的原值。回归覆盖 100 个有效曝光值、正负零和极小浮点数的逐位一致及指纹一致；不是靠改成整数测试夹具绕开问题。

## 并行开发的兼容收口

另一会话在本轮中加入照片级色彩状态，把可读 profile 上限提升到 2；XMP 沿用 issues::profile_schema_version，旧配置继续写 1，含色彩状态的配置写 2，并检查版本和配置匹配，不统一升级旧配置。另一个会话加入 rb:tags 和照片标签来源快照，本轮文件保护规则已适配已知 tags 资源，并补纯标签文件不会误触未知结构保护的回归。色彩与标签模块及其迁移由对应会话维护。

## 涉及文件与复用

- 依赖特性：`Cargo.toml` 的 serde_json float_roundtrip。
- 核心：`crates/raybend/src/xmp/{mod,packet,path,adopt}.rs`；`store/{develop,issues,assets,delete}.rs`。
- 外壳：`src-tauri/src/sidecar.rs`、`import.rs`、`lut.rs`（仅开放既有 root 供资源检查复用）。
- 规格与状态：`specs/xmp-sidecar.md`、`specs/issue-xmp-contract.md`、`memory/FUTURE.md`、`memory/PLAN.md`。
- 新增 adopt.rs 是从外壳移动业务，旧外壳采纳和 stack_importable 已收敛到核心，没有保留第二套实现。路径、原子发布、XML 转义、参数/曲线校验、issue 指纹与编号分配均复用现有实现。
- 本轮无需数据库 schema 或新增依赖；Cargo.toml 仅开启既有 serde_json 的 float_roundtrip 特性（没有额外依赖）。并行会话的色彩与照片标签迁移不计入本轮。

## 命令、XMP 与状态评估

本次修正的都是既有动作后的自动同步，没有新增用户操作，因此不登记新全局命令，也不设默认热键；触发入口仍是 develop_commit、issue_create/delete、browse_mark/undo/redo。评级、编辑栈、文字、标签与定稿的 sidecar 写出/读回同时覆盖，不留后续接线欠账。

按崔总 2026-10-04 追加要求，FUTURE 明确标识 XMP 与 editor presets 为“已完成，待验收”；预设审计报告仍保留真实发现，该状态不等于其审计问题已经修复。当前完善色彩管理和文件管理，等二者完成与验收后最后推进 CI 与发版，由崔总在其它会话处理；PLAN 的当前 XMP 状态与依赖文字同步。

## 验证

已验证（Agent 单元/冒烟）：

- `cargo test -p raybend --lib --offline --quiet -- xmp:: store::issues:: store::develop:: store::assets:: store::delete::`：**141 通过，1 忽略，0 失败**。包含新增 JSON 浮点逐位往返、新旧 profile 版本和纯标签保护测试，4.25 秒。
- `cargo test -p raybend-desktop --lib --offline --quiet`：**104 通过，1 忽略，0 失败**。包含本轮队列 FIFO 与批量标记去重/排除私有状态测试。
- `cargo check --workspace --offline`：通过。剩余 warnings 为开发中模块的未使用代码/变量，不影响 XMP 验证。
- 前端全部 `src/**/*.test.ts` 通过显式动态 import 实际执行：**1191 通过，0 失败**。复现命令：`node --input-type=module -e 'import {globSync} from "node:fs"; import {resolve} from "node:path"; import {pathToFileURL} from "node:url"; for (const file of globSync("src/**/*.test.ts")) await import(pathToFileURL(resolve(file)).href);'`。避免仅文件级成功被误记为内部测试通过。
- `./node_modules/.bin/tsc --noEmit`：通过。
- `node scripts/check-hardcoded-colors.mjs`、`node scripts/check-architecture.mjs`、`node scripts/check-i18n.mjs`：全部通过。
- 涉及文件 `git diff --check`：通过。

全局验证的实际限制：启用精确 JSON 解析后的一轮核心全量测试为 **1373 通过、1 失败、9 忽略**；失败是正在开发的 `color::display::tests::screen_description_matches_independent_lcms_file_transform`（`crates/raybend/src/color/display.rs`），比较屏幕转换描述与独立 LCMS 结果不符。XMP、定稿编号上限和迁移用例在该轮已通过；未把整轮写成全绿。色彩显示转换由相应会话继续完善。之前的旧 fixture 参数超限、缺基础曲线快照、前端缺 color=null 断言已经随并行开发更新；实际浮点指纹问题在本轮修正，没有仅替换测试数值来掩盖。


测试包括实际临时文件的发布/备份/删除，真实内存 SQLite 的读回、FTS、幂等和事务失败注入；回收使用临时文件与假 Trasher，不触碰系统回收站。后台队列和批量标记范围有单元测试。上述结果属于 Agent 单元/冒烟验证。

执行通道：普通 sandbox 命令启动报 CreateProcess / No such file or directory；同一 WSL 路径通过自动审核后的执行通道可以正常读写、编译和测试。问题定位为受限启动器，WSL 文件系统本身正常。pnpm 启动器此前受 registry identity 校验限制，本轮前端验证直接调用已安装 Node/tsc 和仓库检查脚本，未安装工具或修改包管理配置。

并发过程中曾出现未完成的接口/模型改动和旧测试夹具导致编译或测试失败，均按当时状态记录，未用降低校验来换测试通过。原生 profile 类型和标签快照以最新接口联调，最终结果以上述命令为准。

## 待崔总验收

Windows 真实照片库的写出/读回、SOOC/RAW 切换后的实际画面、跨目录导入、未知文件保留、真实回收站及资源缺失反馈仍需人类验收；本轮未把单元/编译通过记成 GUI 功能正确。色彩管理及文件管理的后续新增数据仍须随对应功能更新 XMP 往返与资源依赖测试。
