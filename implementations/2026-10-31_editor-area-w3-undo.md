# W3 —— 撤销与持久化：`area_id` + 结构 op

> 完成时间：2026-10-31 20:21:37
> 规格：`specs/editor-area.md` §3 / §5.6 ｜ 交接书 `specs/editor-area-handoff.md` §3

## 改动范围

| 文件 | 改了什么 |
| --- | --- |
| `crates/raybend/src/store/marking.rs` | `Op::DevelopParam` / `DevelopCurve` 加 `area_id: Option<u32>`；新增 `AreaAdd` / `AreaRemove` / `AreaMove` / `AreaRename` / `AreaToggle` / `AreaColor` / `AreaPoint`；`apply` 分发；`ChangeSet::invert` 改成**逆序** |
| `crates/raybend/src/store/develop.rs` | 层写入口：`mutate_stack`（唯一外壳）、`area_rows` / `rows_to_stack`、`set_area_param`、`set_area_curve`、`add_area`、`remove_area`、`move_area`、`rename_area`、`toggle_area`、`set_area_color`、`set_area_point`、`point_bits` / `point_from_bits` / `area_point`；`prune_empty_stack` 补 `base_curve_points IS NULL` |
| `src-tauri/src/develop.rs` | `DevelopStackDto.areas/defaultIndex`（**覆盖式落库必须带上**，否则一次普通编辑就把层抹掉）；`diff_stack` 拆成 `diff_values` + `diff_areas`（增删 → 排序 → 内容）；`undo_label` 认结构动作 |
| `src-tauri/src/browse.rs` | 测试里的 `Op::DevelopParam` / `DevelopCurve` 补 `area_id: None` |

## 关键决策与理由

1. **旧历史兼容不需要迁移**：`area_id: Option<u32>` 缺省为 `None` = 默认层，
   旧的撤销栈（JSON / 内存）照旧解析、照旧作用在默认层上。
2. **`ChangeSet::invert` 逆序**：一步改动里的 op 是按先后执行的，层排序是多步
   `AreaMove`（选择排序式），正着反跑还原不了。对互不相关的 op（打星 / 标签）倒序没区别。
3. **结构 op 存整段层 JSON**（照抄 `DevelopCurve` 的手法）：撤销要的是"原样还原"，
   字符串最不容易出错；`AreaRemove` 还存**原行号**，撤销时插回原位（不是追加到末尾）。
4. **控制点六个数一起进一个 op**（`AreaPoint { before/after: [u32; 6] }`，f32 位模式）：
   采样位 / 种子 / 容差是同一组事实，拆成两个入口会在撤销栈里留下"一半旧一半新"的中间态。
5. **`diff_areas` 的顺序**：删（行号从大到小）→ 插（从小到大）→ 在骨架上算移动（最多 n−1 步）
   → 改名 / 启停 / 换色 / 控制点 / 逐层参数。正着跑能从 before 走到 after，逆序重放能走回来。
6. **所有层写入口共用一个外壳**（`mutate_stack`：读栈 → 改 → `validate` → `save`）：
   截上限、默认层位置、参数归属（Global 不许进层）只在一个地方判。
7. **`prune_empty_stack` 补 `base_curve_points`**：撤销现在是逆序重放的，
   "先写 points、再写 profile"这两个中间态之间行会被误删（W3 真跑出来的回归）；
   补上之后 `prune` 的判据与 `DevelopStack::is_empty` 一致。

## 单测

* `src-tauri/src/develop.rs::area_structural_edits_round_trip_through_the_shared_undo`：
  删层 + 改名 + 启停 + 换色 + 改容差 + 新建层 + 默认层移位，一次 confirm 里全做；
  撤销逐字段回到 before（位置 / 名字 / 颜色 / 容差），重做回到 after。
* `legacy_param_ops_still_target_the_default_layer`：没有 `area_id` 的旧 op 只动默认层。
* `crates/raybend/src/store/marking.rs` 现有"位模式精确还原"等测试继续通过。

## 验证

```bash
cargo test -p raybend-desktop --lib run  # 192 passed; 0 failed
cargo test --workspace                   # 全绿（完整读数见 W6 记录）
```

## 遗留

* 一次拖动排序若跨多行，`diff_areas` 会拆成多步 `AreaMove`（正确但撤销标签只说"编辑区"）。
* 层不透明度暂无 UI（字段与 op 都已就位）。
