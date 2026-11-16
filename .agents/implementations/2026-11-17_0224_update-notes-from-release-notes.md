# 2026-11-17T02:24+08:00 — latest.json 的 notes 改为取 RELEASE-NOTES.md 的本版变更段（并修好写死版本的发布测试）

`latest.json` 的 `notes` 原来是硬编码空串（`scripts/lib/release-artifacts.mjs:21`），所以应用内更新弹窗没有任何更新说明。

## 改法

新增共用提取函数 `releaseNotesSummary(releaseNotes)`（`scripts/lib/release-artifacts.mjs`）：从 `RELEASE-NOTES.md` 取 `## 本版变更` 段，遇到下一个 `## ` 标题即止；返回空串的情况有三种（没有该段 / 该段是「详见提交记录：…」的兜底链接 / 空正文）；超过 20000 字符按行截断并加 `…`（`MAX_UPDATE_NOTES_CHARS`）。

- `scripts/finalize-release.mjs`：为消除循环依赖（模板里要用「有无更新清单」的布尔），把 `hasUpdater` 提到前面，先拼 `releaseNotes` 文本、再算 `metadata`（`notes: releaseNotesSummary(releaseNotes)`），最后原样写出同一个 `releaseNotes`。
- `scripts/lib/release-upload.mjs`：发布前要重算 `latest.json` 并与磁盘逐字节比对，所以这里也必须用**同一个**函数、从磁盘的 `RELEASE-NOTES.md` 取值，否则每次发布都会报「更新清单与最终安装器不一致」。

## 顺带修掉一个既有测试缺陷

`scripts/lib/release-run.test.mjs` 把 Windows `cargo tauri --version` 的输出写死成 `tauri-cli 2.11.4`；11-10 地基升级把 `@tauri-apps/cli` 提到 2.12.4 后，`release.mjs` 的版本一致性硬闸就让这 8 处 mock 全部假失败（`pnpm test:release` 不在常规门里，所以一直没被看见）。改成读真实 `node_modules/@tauri-apps/cli/package.json` 的版本，`pnpm test:release` 恢复全绿。

## 验证

- `pnpm test:release`：**112 项全绿**（含新增的 notes 提取与传播用例：正常段、提交链接兜底、缺段、空文件、超长截断、`notes` 缺省为空）。
- 端到端手跑：用真实 `docs/release-notes/0.2.0.md` + 合成安装器跑 `finalizeRelease` → `githubReleasePlan`，字节比对通过，`latest.json.notes` 为 1766 字符（正是「本版变更」那段，首行即「本版重点在编辑侧……」）。
- `node --check` 四个改动文件通过；`docs/release.md` 补了两处说明（`--with-updater` 的 notes 来源、上传前的逐字节比对约束）。
