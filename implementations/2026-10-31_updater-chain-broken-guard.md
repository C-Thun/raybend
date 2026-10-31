# 更新链被 0.1.4 打断：诊断 + 正式版断链防护

2026-10-31 15:02:11 CST 完成。

## 现象

已装 0.1.3 的用户点「检查更新」得到：

```text
更新失败：Could not fetch a valid release JSON from the remote
```

## 诊断（三个独立事实）

1. **更新的取数地址是「永远指向最新 Release」**：`src/lib/update-source.ts` 的稳定源是
   `https://github.com/C-Thun/raybend/releases/latest/download/latest.json`。
2. **`latest.json` 现在 404**（curl 实测）：

   | URL | 结果 |
   | --- | --- |
   | `releases/latest/download/latest.json`（当前 = v0.1.4） | **404** |
   | `releases/download/v0.1.4/latest.json` | **404** |
   | `releases/download/v0.1.3/latest.json` | 200 |

   v0.1.3 的资产里有 `latest.json` + `RayBend_0.1.3_x64_en-US.msi.sig`；
   **v0.1.4 只有 msi 与三份清单** —— 那次构建是 `--win-msi --unsigned`，**没有 `--with-updater`**。
3. **0.1.4 自己也没有更新能力**：`release.mjs` 里 `publicKey = request.withUpdater ? updaterPublicKey(...) : ""`，
   `vite.config.ts` 把它注入成 `__RAYBEND_UPDATER_PUBLIC_KEY__`。对已构建的 exe 做字节检查：

   ```
   grep -a -o "raybend-worker-proto-v7" raybend-desktop.exe | wc -l   # 1（对照组，说明 grep 有效）
   grep -a -o "dW50cnVzdGVkIGNvbW1lbnQ6" raybend-desktop.exe | wc -l   # 0（minisign 公钥前缀）
   ```

   公钥为空 → `updateSourceForBuild()` 返回 `null` → 0.1.4 连「检查更新」都不会发起。

结论：这不是网络问题，是 **0.1.4 这版没带更新产物**，而更新地址按「最新 Release」解析，
于是 0.1.3 的更新链被 0.1.4 整体截断；同时 0.1.4 自身的更新入口是关的。

## 处置

### A. 恢复链路（需崔总执行，发布本体归人）

发一版带更新产物的版本，`releases/latest` 指向它之后 `latest.json` 重新可用，0.1.3 用户直接跳到新版：

```fish
pnpm release patch --win-msi --with-updater --unsigned --publish
```

- 需要 `TAURI_SIGNING_PRIVATE_KEY`（`$HOME/.config/raybend/updater.key`）与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  —— 这对密钥与 0.1.3 用的是同一对（`release.config.json` 公钥未变），版本才验得过。
- crates.io 会自动跳过（`crates/raybend` 自 0.1.4 以来无改动）。
- **0.1.4 用户仍要手动装一次**：那版公钥为空，自身不具备更新能力。0.1.3 用户不受影响（可自动更新到新版）。

### B. 防护：正式版断链守卫（本次实现）

`pnpm release` 在**升版与构建之前**判一次：通道是 `release`、要打 Windows 安装器、`release.config.json`
（或 `RAYBEND_UPDATER_PUBLIC_KEY`）里**配了更新公钥**，却没有 `--with-updater` → 直接拒绝：

```bash
正式版缺 --with-updater：新版公开后 releases/latest/download/latest.json 会 404，
已装用户的自动更新全部失效（带更新发版请加 --with-updater，需要 TAURI_SIGNING_PRIVATE_KEY；
确认这次就是不带更新产物才加 --no-updater）
```

- 新开关 `--no-updater`：显式声明「这次就是不带更新产物」，与 `--with-updater` 互斥、需要安装器目标。
- `--dry-run` 只打印警告、不阻断（预览仍能看到完整计划）。
- 守卫不依赖网络（只读仓内 `release.config.json`），没有公钥的项目（第三方自建、test 通道）不受影响。

## 涉及文件

| 文件 | 改动 |
| --- | --- |
| `scripts/release.mjs` | `configuredUpdaterKey` + `updaterGap` 守卫（警告在 dry-run 前、阻断在 dry-run 之后） |
| `src/lib/release-plan.ts` | 新参数 `--no-updater`（与 `--with-updater` 互斥、需安装器目标）；`ReleaseRequest.noUpdater`；USAGE 更新 |
| `src/lib/release-plan.test.ts` | 参数解析：`--no-updater` 生效、缺安装器报错、与 `--with-updater` 冲突 |
| `scripts/lib/release-run.test.mjs` | 守卫回归：有公钥 + 正式版 + 无 `--with-updater` → 升版前拦住；`--no-updater` 放行；`--dry-run` 只警告 |
| `docs/release.md` | §6.3 参数表加 `--no-updater`；§8 坑表加「更新失败：Could not fetch a valid release JSON」一行 |

## 验证

- `pnpm test:release` → **111 / 111 pass**（新增 1 个守卫用例）
- `node --test --conditions=browser src/lib/release-plan.test.ts` → 29 / 29 pass
- `pnpm typecheck` → 通过
- 断链证据全部为可复查的实测：curl 三个 URL 的状态码、exe 内字符串命中数（含 worker 协议对照组）

## 遗留

- 真正的复原动作是 A（发一版带 `--with-updater` 的版本），由崔总执行；本次只落地了守卫与文档。
- 0.1.4 用户在复原版发布后**仍需手动安装一次**（公钥为空不能自更新），发布说明里要写清。
- 工作树另有并行改动（`Dialog.tsx` / `tokens.css` / `AGENTS.md` / `memory/DESIGN.md` / `specs/dialog-stack-and-border.md` /
  `MigrationGate.tsx`，来自另一会话的对话框描边调整），本记录涉及的提交只含上表五个文件 + 本记录。
  发版前必须先收口，否则 `pnpm release` 会因工作树不干净而阻断。
