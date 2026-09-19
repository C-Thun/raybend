# 数据库升级阻塞遮罩 + 看图「返回」按钮回归修复

完成时间：2026-09-19 17:14:38 CST

本单元两件事一起提交（同一轮工作、互相独立的两处）：
**(一)** 把「升级时挡住用户操作」这条要求落成完整链路；
**(二)** 修掉上一个提交 `448fa4b` 抽共享控件时留下的真 bug（返回按钮点了没反应，
对比态干脆没有返回按钮）。

---

## 一、数据库升级 → 阻塞遮罩（人类 2026-09-19 的要求）

### 需求口径（人类原话）

> 从现在开始就给每个数据库打个版本编号，并且内置系统在载入对应的数据库时（包括 app、
> catalog，以及其他数据库如果有的话）能根据版本号调用对应的 migration 进行升级。
> 注意 catalog 这种外部数据库，只有在具体调用时才检查+升级；升级时可以弹一个窗
> 禁一个用户行动等升级完再可用。这套机制可以预先建立起来。

### 盘点结论（先查再动手）

机制在 M1 就**已经齐了**（`store/migration.rs`：版本闸门 + `VACUUM INTO` 快照 +
逐条事务 + 完整性检查；`CatalogDb::open` 只在真正调用时检查升级）。
缺的不是机制，而是**外壳与界面那一段**：升级时前端完全不知道，遮罩无从谈起。

### 改动

| 层 | 文件 | 改动 |
| --- | --- | --- |
| store | `crates/raybend/src/store/migration.rs` | 通知从「只有开始」变成**有头有尾**：`MigrationPhase::{Start,Done}` + `MigrationNotice{kind,from,to,phase}`；`apply_list_with_hook` 拆出正文 `run()`，保证**失败也发 Done**（否则前端遮罩永远撤不掉） |
| 外壳 | `src-tauri/src/migration.rs`（新） | 注册钩子 → `emit("db://migration", MigrationEvent{kind,label,from,to,running})`；`install()` 在 `lib.rs` 的 `setup()` 里、**`db::warm_up` 之前**调用（晚了就听不到 `app.db` 的迁移） |
| 契约 | `src-tauri/src/contract.rs`、`src/api/dto-contract.json`、`src/api/types.ts`、`src/api/dto-contract.test.ts` | `MigrationNotice` 进 IPC 契约（事件也走契约：字段名漂了就会出现「升级完界面还是死的」这类静默故障） |
| 前端 | `src/features/migration/notice.ts` | 纯状态机：`Map<kind, notice>`，**按库种类记账** —— 一类结束不会把另一类的遮罩撤掉 |
| 前端 | `src/features/migration/MigrationGate.tsx` | 全窗口阻塞遮罩（`bg-scrim` + `surface-layer`，不给出口）；**鼠标 + 键盘都挡** |
| 组装 | `src/App.tsx` | 订阅 `db.onMigrationNotice`，根层挂 `<MigrationGate>` |
| 演示/冒烟 | `src/dev/migration-gate-demo.tsx`、`src/dev/KitchenSink.tsx`、`scripts/ui-smoke.mjs` | 真状态机 + 假通知的画廊演示；冒烟断言「出现 → 键盘被吃掉 → 收到 Done 后撤掉」 |

### 两个容易漏的点（都已落地并断言）

1. **失败也要撤遮罩**：`run()` 出错时仍发 `Done`，界面不会卡死；错误由那条命令的 `Result` 报。
2. **键盘那一半不能省**：只挡鼠标时，用户按住方向键/数字键照样能动一个「结构正在被改写」的库。
   拦截挂 `document` + `window` 的**捕获阶段**；冒烟把按键派发在 `document.body` 上
   （真实路径 window → document → body → …）—— 派发到 `window` 是 target 阶段，
   同名节点按注册顺序触发，测不出捕获拦截（第一版就这么写的，冒烟当场抓出来）。

## 二、看图「返回」按钮点了没反应（`448fa4b` 的回归）

- **根因**：抽共享控件时 `Viewer` 把 `onClose={props.onClose}` 给了 `ViewerControls` ——
  那是「通知外面」的回调，**没有关 store**。`pnpm smoke:ui` 的看图演示当场抓到
  （`看图左上角的返回按钮点了没反应`）。
- **同一处还有第二个洞**：`CompareView` 自己有 `onClose` 属性，但两个工作区都没传 ⇒
  **对比态根本没有返回按钮**，正是人类 08:00 那条抱怨
  （「import 中 compare …… 左上角的返回和右下角的缩放工具都没了」）没修干净的部分。
- **修法**（两个视图同一条规矩）：视图内部 `close()` = `store.close()` + `props.onClose?.()`；
  工作区只负责复位外壳状态（`setChrome("default")`）。browse / import 都照此对齐。
- **新增回归断言**：`scripts/check-browse-boot.mjs` 在对比态断言
  「整个对比区只有一组返回 + 一组缩放」并且**点得动**（点完退出看图、三态复位）。

## 三、验证（Agent 侧冒烟，均已实测）

```text
cargo test -p raybend  → 804 passed; 0 failed; 1 ignored
cargo test -p raybend-desktop → 43 + 2 + 2 + 6 passed
cargo clippy -p raybend -p raybend-desktop --all-targets → 无告警
pnpm typecheck → 0
pnpm test → 670 passed / 0 failed
pnpm lint:colors / lint:arch / lint:i18n → 全绿
pnpm smoke:ui → problems: []（含新增的升级遮罩断言）
pnpm check:browse → 通过（含新增的对比态返回按钮断言）
```

新增单测：`migration.rs` 的 `reports_start_then_done_around_a_real_migration` /
`no_notice_when_there_is_nothing_to_migrate` / `failing_migration_still_reports_done`；
`features/migration/notice.test.ts` 8 条（含「没 Start 的 Done」「两类交错」「入参不被改」）；
`src-tauri/src/migration.rs` 2 条 + 契约 1 条。

**未经人类验证**（`AGENTS.md` §2.8）：真机上「库比程序旧时弹遮罩」的目视效果、
遮罩文案排版、以及升级耗时的体感 —— 要造一个旧版 schema 的库才能看到（`pnpm migrate:drill`
可以造，但那是终端演练，不是界面）。

## 四、过程记录：检查器陈旧快照（本仓已知现象）

`migration.*` 三个键加进语言包后，pi-lens 反复报「键不存在」。按 `src/i18n/index.ts`
文件头记的判据核实过两次：

- `pnpm typecheck` 退出码 0，并且**临时 `MessageKey[]` 证明文件**（列出这三个键）也过；
- `lens_diagnostics source=lsp` 主动探针对 4 个文件实测 **0 诊断**。

⇒ 陈旧快照误报，未改任何代码；顺手在 `src/i18n/index.ts` 里加了一行缓存刷新锚点注释。
