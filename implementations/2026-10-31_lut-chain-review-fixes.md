# 多层 LUT Review 修复（2026-10-31）

2026-10-31 18:05:35 CST 完成。

## 范围

针对本轮「LUT 应用层链 + GPU 化」改动的第三方代码评审。只修改本轮引入的代码与测试；未动其它会话在途的窗口布局等文件。

## 已确认并修复的问题

1. **1D-only LUT 在 GPU 上传会越界**
   - `render/creative_lut.rs::upload_atlas` 对没有 3D 立方表的层仍按立方表索引，空切片会 panic。
   - 修复：纯 1D 层跳过图集写入；shader 继续走 `has_cube = false` 分支。
   - 证据：`examples/lut-offscreen.rs` 新增 `1D LUT：GPU vs CPU`，通过。

2. **总开关被链编辑强制打开**
   - `store.commitChain` 原先对任何非空链都设置 `lut_enabled = true`，导致关掉总开关后调强度/排序又被打开。
   - 修复：只有「空链 → 有层」或显式换底层才自动开启；调强度、排序、加减层保留用户选择。
   - 同时保留旧行为：点新 LUT 换底层仍然立即应用。

3. **0% 强度被导出误判为 LUT 缺失**
   - `DevelopStack::lut_chain_active()` 原先只看 enabled，不看强度；0% 链会让导出校验报「LUT 不可用」。
   - 修复：active 需要 enabled 且 strength > 0（并排除非有限值）。

4. **旧 `lut_id` 会在清空链后复活**
   - `load` 会在 `lut_chain` 为空时无条件把旧 `lut_id` 并回链；而保存/清空链没有清掉旧列。
   - 修复：显式存在 `lut_chain`（即使空数组）时不再采纳旧 id；`save` 写入 `lut_id = NULL`；`Setting::LutChain` 同步清掉旧列。
   - 新增 `clearing_the_chain_does_not_resurrect_the_legacy_lut_id` 回归测试。

5. **旧撤销记录 `lutId` 无法还原**
   - 新 `Setting::LutChain` 只接受 JSON 数组，旧撤销值是纯 id 字符串。
   - 修复：`parse_lut_chain_setting` 同时接受新 JSON 与旧纯 id；写库统一归一化为新链 JSON，并清掉旧列。

6. **`source_base` 经 wire 读入时被静默改成 RAW**
   - 新的 `DevelopStackWire` 原先 `and_then(EditBase::parse).unwrap_or(Raw)`，非法值会改变管线。
   - 修复：改为 `try_from`，未知值报错；同时容忍 `lut_chain: null`（IPC / preset 的 Option 形状）。

7. **预设边界不完整**
   - 已关闭但文件丢失的层不应让整组 LUT 预设失效；重复 id / 超 8 层不能绕过 store 校验。
   - 修复：只要求启用层的文件存在；重复或超上限的链按 missing 处理，不写入当前编辑栈。
   - 新增预设与总开关关闭状态、重复/超限测试。

8. **界面与画稿/需求不一致及交互遗漏**
   - 应用层移到与分类列表**同一个滚动容器**（原先固定在容器外）。
   - 强度拉杆增加双击恢复 100%。
   - 删除按钮复用全项目 `RemoveButton`，不再单独使用 `IconBan`。
   - 拖动离开行时清除 drop 高亮。

9. **兼容细节**
   - `legacy_lut_id` 改为精确 `strength == 1.0`，避免把近似但非 100% 的强度写成旧单支形状。
   - `resolve_chain` 对外部超过 8 层的链拒绝渲染，避免 CPU 与 GPU 各画一张图。

## 验证

```
cargo test --workspace
  → core 1606 passed / 0 failed / 11 ignored
  → desktop 188 passed / 0 failed / 2 ignored
  → 其它 crate 与 doctest 全部通过

pnpm typecheck                 → 通过
pnpm test                      → 1351 passed / 0 failed
pnpm lint:colors / lint:arch / lint:i18n → 三条全绿

cargo run -p raybend --example lut-offscreen
  → 关 LUT、cube、shaper+cube、1D、三层链均与 CPU 对照通过（容差 3/255）
  → 三层链 max=1/255；开/关自检 max=38/255，证明着色器确实执行了链
```

## 未修复 / 剩余风险

- **真实 Windows 观感与帧率尚未由崔总验证**：这里只有 WSL/lavapipe；RAW 解码与首帧准备的耗时没有因 LUT GPU 化而消除。
- **多层链改强度会重建图集**：这是低频的松手动作，33³ 几层约十几毫秒；65³ 大表可能几十毫秒。若真机仍卡，再把强度从图集重建键中拆出，只更新 uniform。
- **旧设计稿派生帧未同步**：`States / Editor / 裁切|旋转|对比` 的左列仍是旧版，已在 `design/editor.md` §1 标为待同步项。
- **Review 工具提示**：`lut-offscreen.rs` 中的 `expect/unwrap` 是示例/测试探针，不是生产路径；没有作为生产缺陷处理。

## 结论

本轮需求的核心功能已实现，评审发现的兼容、边界与交互问题已修复；单元测试与离屏证据通过。可以进入崔总的 Windows 真机 E2E。
