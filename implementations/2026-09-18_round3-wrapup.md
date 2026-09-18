# 第三轮收尾：RAW 时间错位的真根因、view 方向、配对继承、以及边界修正

完成时间：2026-09-18 13:17:00 CST

范围：`crates/raybend/src/media/{tiff,exif,pairing,source}.rs`、`crates/raybend/src/thumbnail/render.rs`、
`crates/raybend/src/import/fsops.rs`、`crates/raybend/src/store/assets.rs`、
`src-tauri/src/source.rs`、`src/api/types.ts`、`src/features/{browse,photo-grid}/*`、`AGENTS.md`、`FUTURE.md`
状态：三条上报全部定位并修复；边界按人类口径改回；仍未做的一条已明确移交给 W2

---

## 一、RAW 排序：真根因是**时间读错**，不是排序

**人类两次上报的现象**：按时间模式下同一天的分组标题出现两次、一次纯位图一次纯 RAW。

**真根因**（用 `thumb-probe` 对同一张照片的两个文件实测）：

```text
修前  P1000019.JPG  millis 1789232068000  offset_min Some(480)   ← 主路按 +08:00 换算
      P1000019.RW2  millis 1789260868000  offset_min None        ← 兜底把墙上时间当 UTC（差 8 小时）
修后  两者都是       millis 1789232068000  offset_min Some(480)
```

- `media/tiff.rs`（RAW 兜底解析器）**根本没读 `OffsetTime*` 标签**，
  而 `exif.rs` 的兜底还写着「TIFF 家族外层一般不带 OffsetTime，所以只能是 None」——
  对 Panasonic RW2 是错的（EXIF 子 IFD 里明明写着 `+08:00`）。
- 加了 RAW 支持之后，RAW 的时间从「mtime 兜底（与 JPG 的墙上时间一致）」变成
  「墙上时间当 UTC」→ 位置变了、8 小时还跨天 → 日期标题重复。
  **这正好解释人类说的「加 RAW 支持之前顺序是正常的」**。
- 修法：`tiff.rs` 读 0x9010/0x9011/0x9012（Original > 通用 > Digitized），
  `exif.rs` 兜底复用主路同一套换算。两条回归测试钉住。

**我前两轮改错了地方**（`92f2b7a` 保持传入顺序、`de3a3e4` 片内自然序）：
片**边界**早就把两种格式分开了，片内怎么排都没用。方向从头就是错的。

## 二、view 里 RAW 原图歪着：方向优先级反了（已修）

`render_raw_file` 里是 `decoded.orientation.or_else(读文件头)` —— **解码器报的值优先**。
而解码器从不代为摆正（`PixelSource::applies_orientation` 恒为 `false`），对 TIFF 家族并不可靠：
实测 Panasonic RW2 报 `1`，文件头里写着 `8` → `Some(1)` 盖掉真方向 → 不转。
tiles 走内嵌预览那条路时 worker 给的是 `None`、回退到文件头 → 反而转了 —— 这就是「tiles 正、view 歪」。
改成**文件头优先**，抽 `pick_orientation()` + 测试钉住优先级。

**同时又踩了一次缓存版本**：v4 是 11:44 抬的，而方向修复是 12:57 才进来的 →
11:44–12:57 之间渲染出的 v4 缩略图带着旧行为却仍有效（人类：「感觉和以前一样」）。
已抬到 **v5**，并把这条教训**第三次**写进 `PIPELINE_VERSION` 的注释：
**改渲染行为必须和抬版本号在同一个改动里完成。**

## 三、配对继承（位图 / RAW）

新增 `media/pairing.rs`（纯函数 + 8 条单测）：RAW 自己没读到时间时，从**同名位图**继承。
配对键 = 折叠目录 + 折叠主名（复用 `kind::stem_folded`），并去掉库内 `_RAW/` 那一级；
键的拆解**两种分隔符都自己认**（不能用 `Path::parent()`：应用跑在 Windows，
而 `Path` 在 Linux/macOS 上不把 `\` 当分隔符 —— 混用风格时会把 `C:\Photos\x.JPG` 判成「没有目录」）。
新增 `TakenAtSource::Sibling` 诚实标注「这条时间是从姊妹文件继承的」。
接线：`media::source::read_times`（导入网格按时间时走）与 `import::fsops::enrich`（入库前定时间）。

**顺带查实**：库里 `测试库` 那 8 张的 JPG 明明有 EXIF 时间（2026:09:13 14:25:19），
而数据库里是 NULL —— 说明**导入时没存下时间**（很可能取的是 RAW 那一侧，而那时 RAW 还读不到）。
配对继承覆盖这种情形：新导入不会再 NULL。老库回填按人类决定单独立项。

## 四、边界修正（人类 2026-09-18 定的架构口径）

> 「排序在显示端做是正确的，数据源怎么可能需要去理解展示的逻辑。」
> 「像按时间这种，涉及到额外循环聚合的逻辑，肯定都是在显示层做。」

我这两轮把「片内自然序」塞进了 store（数据层）—— 越界。已改回：

| 层 | 现在做什么 |
| --- | --- |
| store（数据层） | 只按**查询顺序**给数据（`itemAt`/`ensureRange`/`orderedIds` 回到页与下标）；只多三个**可选**入参（`select`/`selectAll`/`selectedCount` 的 `order`），顺序由显示层给 |
| rows.ts（显示层） | 行模型带 `slots`（每格的数据下标，已是显示序）；`buildBrowseRows` 接受 `order`（来自 `sliceOrder`） |
| BrowseGrid（显示层） | 自己持有 `order`；逐格 `store.itemAt(row.slots[i])`；取数把可视行的 slots 汇成 min..max 超集；区间选择把可见顺序传给 store |

验收口径：**store 里搜不到「显示/排序/分组」这类词**。

## 五、术语约定（写进 `AGENTS.md` §11.4）

`tiles` = 图片列表组件；`view` = 单张查看组件；`film` = 上查看 + 下横向胶片带；
`editor` = Rust 原生 GPU 编辑界面；四工作流 `import/browse/edit/export`；
workspace 三列用「工作流 + 位置」组合表达（`import left` / `browse mid` / `export right`），
中间更常说 `browse tiles` / `browse view` / `browse film`。

## 六、验证

```text
npx tsc --noEmit                     0 error
pnpm test                            503 passed
pnpm lint:colors / lint:arch / lint:i18n   通过
cargo test -p raybend                770 passed / 1 ignored
cargo test -p raybend-desktop        35 passed
cargo clippy --workspace --all-targets     0 warning
pnpm build / smoke:ui / check:browse 通过（problems: []）
node scripts/spike-win.mjs --dry-run 通过（本次亲眼确认它真的 Compiling 了 raybend-desktop，
                                     而不是只比时间戳 —— exe 13:16:37）
```

**待人类验证**：view 里纵拍 RAW 的方向（v5 会让旧缩略图失效、重新出图）、
DPI 拖拽的日志（日志已做足，人类手拖后把 `/tmp/raybend-desktop.log` 发回）。

## 七、交接给下一轮（W2 的第一件事）

**分组 key 换成「格式化后的本地时间串」**（人类的设计指正，计划 §6 已登记）：
只要格式化出来的字串相同，就必须落进同一个片 —— 这条不变量一旦有测试，
本次这类「同一个可见时刻、两个篮子」的错在单测阶段就会红。
另外老库 `taken_at` 回填要**先修解析器再回填**（解析器已修 ✓，可以做了）。
