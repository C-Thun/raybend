# 2026-11-17T01:06+08:00 — 补齐 alloc-stdlib@0.3.0 与 tauri-plugin@2.7.2 的许可证原文，解除发版阻断

发版在 `pnpm licenses:generate` 处中断：`Error: 许可证原文缺失：alloc-stdlib@0.3.0 (BSD-3-Clause)`。发布脚本已回滚版本写入（工作树恢复干净），但构建没继续。

## 根因

`scripts/lib/license-inventory.mjs::cargoLicenses` 对每个 cargo 依赖先扫包内 LICENSE/COPYING/NOTICE 文件；扫不到就用 `standardTexts` 兜底，而 `scripts/generate-licenses.mjs` 里的兜底表**只有 MIT 与 MPL-2.0** 两条，别的 SPDX id 直接抛错。

这次的地基升级（Tauri 2.12 / image 链路）带进两个新版本，它们都**不随包发许可证文件**：

| 依赖 | 许可证 | 为什么现在才炸 |
| --- | --- | --- |
| `alloc-stdlib@0.3.0` | BSD-3-Clause | 0.2.4 → 0.3.0；override 表里只有 0.2.4 那条，键含版本号，所以不命中 |
| `tauri-plugin@2.7.2` | Apache-2.0 OR MIT | 2.6.3 → 2.7.2，同样是版本键不命中 |

## 修法

在 `legal/upstream-license-overrides.json` 补两条 override（`{source, text}`，与该文件既有条目同形），文本取上游原文而不是 SPDX 模板，保留真实版权行：

- `alloc-stdlib@0.3.0` ← `dropbox/rust-alloc-no-stdlib` 的 `0.3.0` tag `LICENSE`（与 0.2.4 那条逐字节相同，已 diff 确认）；
- `tauri-plugin@2.7.2` ← `tauri-apps/tauri` 的 `tauri-v2.12.4` tag `LICENSE-APACHE-2.0`（crate 声明 Apache-2.0 OR MIT，取 Apache 一版，与表内 `gl_generator` 的既有做法一致）。

写入时保持文件原有键序（只插 8 行），避免整表重排把 review 淹没（第一次 `json.dumps(sort_keys)` 造成 23+/15− 的假 diff，已按 HEAD 顺序重写）。

## 验证

- 全量排查脚本（照 `cargoLicenses` 的判定逻辑复跑）确认 486 个依赖里**只有这两条**缺原文，没有第三个坑；
- `pnpm licenses:generate` → `许可清单：592 项；SPDX 原文补足：libwebp-sys@0.9.6, selectors@0.38.0, simd_helpers@0.1.0, webview2-com-sys@0.39.1, webview2-com@0.39.1`（这 5 条是本来就有 MIT/BSD 兜底的，未新增缺口）；
- 连跑两次 `third-party.json` 的 sha256 一致（幂等）；
- `pnpm build` 重新出前端产物（发布脚本要求重试前重建），通过。

## 遗留

- `webview2-com@0.38.2` 系列 override 已随 0.39.1 失去作用，留着无害（当前均在用 `standardTexts`），未删除以免误伤回退场景。
- 兜底表只有 MIT / MPL-2.0 是设计选择（其余走 override 保留真实版权行）；若以后再被新依赖挡住，按本文同法补 override。
