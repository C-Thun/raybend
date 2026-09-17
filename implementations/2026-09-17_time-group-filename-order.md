# 修：按时间模式下 RAW 与位图分层（第二次上报，这回修到位）

完成时间：2026-09-17 19:19:13 CST

范围：`crates/raybend/src/store/query.rs`、`src-tauri/src/browse.rs`、`src/api/{dto-contract.json,types.ts}`、
新增 `src/lib/natural-order.ts`、`src/features/browse/{rows.ts,BrowseGrid.tsx,store.ts}`、
`src/lib/time-group.ts`、`src/features/photo-grid/store.ts` 及相关测试
状态：已修并全绿；Windows 产物已重建，观感由人类确认

---

## 一、为什么「又出现了」——上次只修了一半

人类口径（两次都一样）：**一个时间段内的就按文件名自然排**。

上次（`92f2b7a`）我把 `groupByTime` 的片内顺序从「按拍摄时间」改成「**保持调用方传入的顺序**」，
理由写的是「导入网格按扫描顺序（文件名自然序）传进来」。这句话对**导入**成立，
对**浏览**不成立：

| 网格 | 片内的输入顺序来自哪 | 上次那个改动 |
| --- | --- | --- |
| 导入 | 扫描结果（大致是文件名序） | 有效 |
| **浏览** | **后端时间线** `ORDER BY a.taken_at DESC, a.id DESC` | **等于没改** —— 输入就是时间序 ✗ |

而浏览的时间线条目只有 `{id, takenAt}`，连文件名都没有 —— 前端**没法**按名字排 ✗。
于是 RAW（拍摄时间来自 mtime 兜底、比 JPG 晚几秒）照旧沉到片尾，看起来就是「按格式分层」。

## 二、这次的做法

### 1. 时间线带上路径（Rust）

`query::timeline` 现在返回 `TimelineRow { id, taken_at, rel_path }`（`SELECT … COALESCE(f.rel_path, '')`），
DTO/契约/TS 类型同步。**代价与取舍写进了代码注释**：多一列文本（10 万张约 +3–4MB），
换来的是「同一时间段内 JPG 与 RAW 挨着」这个基本期待。

### 2. 自然序比较器（新 `lib/natural-order.ts`）

数字段按**数值**比（字符串比较会把 `P1000019` 排到 `P1000020` 之后），非数字段忽略大小写，
全等时用原始串兜底保证顺序不漂移。只认 ASCII 数字（相机文件名就是 ASCII）。
配了 8 条测试：数值序、前导零、JPG/RAW 相邻、大小写、前缀、无数字串、空串、含目录的整路径。

### 3. 片内重排 → 交给 store 当「显示序」

`features/browse/rows.ts` 新增 `sliceOrder(timeline, groups)`：**片与片的先后、每片的边界都不动**
（那是时间的事），只在**每一片内部**按文件名自然序重排，返回一个置换。

这个置换交给 `store.setDisplayOrder()`。关键设计：**店面只维护一套下标（显示序）**，
只在两个边界上换算 ——

- `itemAt(displayIndex)` → `entries[order[displayIndex]]`；
- `ensureRange(start, end)` → 先把可见区间映射回**后端下标**再算页号（取的是超集，多取几张无妨）。

不这么做就会出现更难查的 bug：页是按后端顺序取的，而界面按显示序画 ——
**取下来的页与屏幕上那一格错位**（显示错照片）。同理，`orderedIds()`（Shift 区间选择、
键盘导航用的顺序）也必须按显示序，否则用户 Shift 连选会选中他没看见的照片。

### 4. 导入网格同步改（同一个 bug 的另一半）

`groupByTime` 的 `TimePhotoLike` 增加可选 `name`：
**给了名字就按自然序排，不给才退回传入顺序**。导入侧传入 `item.path`。
导图那条路从「碰巧对」变成「明确对」。

## 三、验证

```text
npx tsc --noEmit                     0 error
pnpm test                            502 passed（比原先 +15 条：自然序 8、片内序 3、映射 2 …）
pnpm lint:colors / lint:arch / lint:i18n   通过
pnpm build                           通过
pnpm smoke:ui                        problems: []
pnpm check:browse                    ✓ 库非空时进浏览正常
cargo test -p raybend                759 passed / 1 ignored
cargo test -p raybend-desktop        35 passed
cargo clippy --workspace --all-targets     0 warning
```

**未经人类验证**：真实库里按时间模式看过去，同一时间段内 JPG 与 RAW 是否真的挨着、
滚动/选中/Shift 连选是否与看到的顺序一致 —— 由人类在 Windows 上目视（`AGENTS.md` §2.8）。

## 四、遗留

- 时间线多带一列路径后，10 万张的实际体积没实测（估算 +3–4MB）。等有了十万张的真实库，
  用 `examples/query-bench.rs` 量一下；真嫌大就改成「只在分组模式下带」。
- 自然序目前用于**片内**。若以后「未分组」也要按文件名排，那条路走的是后端
  `SortKey::FileName`（`rel_path_folded`，字符串序而非自然序）—— 到时统一到自然序上。
