# W1：编辑区（area）的类型、迁移与控制点色板

完成时间：2026-10-31 20:26:49

## 1. 做了什么

把编辑区的**数据形状**落进代码：`Area` / `AreaKind` / `DevelopStack.areas`、参数表的 `target` 分类、
DB 迁移、定稿 schema v3、控制点八色板与自动分配。**不碰 UI、不改任何渲染行为**。

## 2. 一处实现层面的改动（与规格 §3.1 不同，已回写规格）

规格原本把默认层做成 `areas[0]`。实际**没有**这么做：

* `DevelopStack.params` / `curves` 在本仓被几十处引用（管线、XMP、预设、缩略图、undo…），
  搬进 `Area` 等于全仓改名 —— W1 的「零行为变化」就守不住了；
* 改成：**默认层就是 `DevelopStack` 自己**（那两个字段 + 全局那几项），
  `areas` 只装**默认层以外的层**，另加 `default_index: Option<u8>` 记住默认层在列表里的位置。

于是「退化 = 只有默认层」时 `areas` / `default_index` **整个不序列化**，旧字节、旧 `profile_hash`、
旧缓存签名逐字不变 —— 三条不变量里最硬的那条（不变量 1）零成本达成。

## 3. 改动的文件

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/src/develop/params.rs` | `ParamTarget { Global, Layered, Mask }`；`ParamSpec.target`；17 条全部标注（镜头 5 + 清晰度 4 + `dynamicContrast` = Global，其余 Layered）；`target(id)` 查询；契约单测加断言 |
| `crates/raybend/assets/develop-params.json` | 同步 17 条 `"target"` + 文件头说明 |
| `crates/raybend/src/store/develop.rs` | `Area` / `AreaKind` / `MAX_AREAS=32` / `POINT_COLORS=8`；`DevelopStack.areas` + `default_index`；wire / out / signature / validate / is_empty / load / save / prune；`next_point_color()`；6 条新单测 |
| `crates/raybend/src/store/issues.rs` | `profile_schema_version` 加 v3 分支（有额外层才是 3，按内容选版本） |
| `crates/raybend/src/store/migrations/catalog_0023_areas.sql` | `ALTER TABLE develop_stacks ADD COLUMN areas TEXT`（只加列、不回填） |
| `crates/raybend/src/store/migration.rs` | 注册 v23 |
| `src-tauri/src/develop.rs` | DTO → stack 补两个字段（**W4 必须把它们加进 DTO**，否则一次普通编辑会把层抹掉 —— 代码里留了注释） |

## 4. 三条不变量与落点

| 不变量 | 落点 | 测试 |
| --- | --- | --- |
| 退化时字节/哈希不变 | `DevelopStackOut` 的 `skip_serializing_if` | `areas_stay_out_of_the_legacy_serialized_form` |
| 阶段边界（全局参数不进层） | `Area::valid` 查 `ParamTarget::Global` | `global_params_are_rejected_inside_an_area` |
| 有层就要换缓存签名 | `signature()` 里追加的那一段（`areas:`） | `areas_change_the_cache_signature` |

另有 `invalid_area_shapes_are_rejected`（上限 / 重复 ID / 颜色越界 / 默认层位置越界）
与 `point_colors_go_to_the_least_used_slot`。

## 5. 控制点色板（崔总 2026-10-31 口径）

固定八色，存**槽位下标**不存色值；创建时取「已用次数最少」的槽位，并列取下标最小；只数 `Point` 层。
八色在 `memory/DESIGN.md` §15.5，`design/editor.pen` 里同步成 `point-0..7` 变量。

色相是算出来的：与 browse 的五个色标（`--label-*`）**至少隔 18.6°**，等距 40° 排一圈，
并按 22→184→62→232… 的顺序分配（连续两次分配在色轮上跳到对面）。

## 6. 验证

* `cargo check --workspace` —— 通过
* `cargo test --workspace` —— **1611 通过 / 0 失败**（新增 6 条）
* `pnpm test` —— **1359 通过 / 0 失败**
* `pnpm typecheck`、`pnpm lint:i18n` —— 通过

## 7. 遗留

* `DevelopStackDto` 还没有 `areas`（W4 加；不加就会在普通编辑时把层抹掉）。
* 四条 `Mask` 参数（`areaRadius` / `areaOpacity` / `areaChroma` / `areaLuma`）**推迟到 W2**
  —— W1 不碰前端参数契约，避免面板里冒出四条无消费者的滑杆。
* XMP 归类没动：本次没有新增参数 id，`every_develop_param_is_classified_for_crs` 仍然绿。
