# 评审修复：浏览右栏元数据输入与地点建议

> 完成时间：**2026-11-04 13:46:50 +08:00**
> 范围：仅限本轮「右栏元数据字段 / 建议输入 / GPS 填入」改动；不是全仓审计。
> 评审对象：`specs/browse-info-fields.md` 与本轮实现代码。

## 1. 已确认并修复的问题

| 问题 | 位置 | 修复 |
| --- | --- | --- |
| 点击字段后输入框不自动聚焦，必须再点一次才能输入 | `src/components/ui/SuggestInput.tsx` | `onMount` 恢复 focus + select；描述行内编辑也复用同一行为 |
| 建议列表容器统一 `preventDefault`，可能挡住滚动条拖动 | `SuggestInput.tsx` | 只在候选项上阻止 pointer 默认行为；列表容器保留滚动条交互；行高定位改用 `--row-h` 令牌 |
| 候选尚未加载时 focus，候选到达后不会自动展开 | `SuggestInput.tsx` | 增加 focused/dismissed 状态：候选晚到且仍在编辑时自动展开，Esc 后不在本次编辑中抢回列表 |
| GPS 逆地理编码异步返回后可能写入刚切换的另一张照片 | `src/features/browse/BrowsePanels.tsx` | 返回后核对锚点 id 与坐标；变化则放弃本次写入 |
| 库内统计请求被普通 `refresh()` 误判过期；候选缓存键用 `|` 拼接可能碰撞 | `src/features/browse/store.ts` | 候选按库 id 防止跨库回写；普通刷新不作废；键改 JSON 元组；写入 revision 递增并在旧响应到达时重取 |
| 跨库异步写入结果可能按相同资产 id 回填到当前库 | `store.ts` | 写入返回后核对 repository id；换库则只返回结果，不修改当前条目 |
| `fill_geo` 把已有空字符串当成已填写，与前端空白语义不一致 | `crates/raybend/src/store/marking.rs` | 空白串按空字段处理；撤销保留原始空白值；新增回归测试 |
| 只读/待结算时「展开编辑」「应用到所选 N 张」仍可点 | `BrowsePanels.tsx` | 按钮、描述 textarea 与确认按钮统一受 disabled 控制 |
| GPS 非有限/越界坐标仍可能生成显示文本或地图链接 | `BrowsePanels.tsx` | 只读展示与地图入口都先校验经纬度范围与有限性 |
| 描述弹窗打开期间外部值更新会覆盖正在编辑的草稿 | `BrowsePanels.tsx` | 只在打开瞬间取初值，不再跟踪外部 value 重写草稿 |
| 大批量写入时跳过锁的筛选逐个 `includes` | `store.ts` | 改为 Set 查找 |

## 2. 测试与质量门

- `cargo test --workspace`：raybend lib **1644 passed**，raybend-desktop **199 passed**，其余目标也通过，0 failed。
- `pnpm test`：**1415 passed / 0 failed**。
- `pnpm typecheck`：通过。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过。
- `pnpm build`：通过；只有既有的 chunk >500 kB 提示。

新增/更新测试覆盖：候选缓存跨刷新、写入途中旧响应、跨库回填、竖线键碰撞、空白地理字段与撤销保留原值；此前的回填、批量、锁跳过、GPS 填入、最近输入与筛选测试继续通过。

## 3. 未修复 / 剩余风险

1. **未做真机 E2E**：Solid 实际 focus、IME 组合输入、Portal 列表滚动与 Windows 浏览器打开行为仍需人工验证。
2. **`design/browse.pen` 未补帧**：本轮 Pencil 应用不可连接，仅更新 `design/browse.md` 文字规格。
3. 城市表是随版本发布的静态数据，地名会随时间变旧；重建入口为 `scripts/build-geo-data.mjs`。
4. 本轮只登记了 H7「导入/导出自动元数据模板」与 H11「只读自家 XMP」，**没有把这两个未来功能提前实现**；这是需求范围内的刻意取舍。

## 4. 评审结论

**通过（单元测试与静态质量门范围）**。本轮需求对应的实现完整，发现的确认缺陷已直接修复；剩余风险主要是人工 E2E 与设计稿补帧，不涉及已知的未修复代码阻断项。
