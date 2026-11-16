# 0.1.4 发布中断：草稿读取轮询 + 发布失败不回滚版本写入

2026-10-31 14:39:20 CST 完成。

## 背景

`pnpm release ... --win-msi --publish` 一键发版在 v0.1.4 首次执行时中断：

```text
✗ 构建失败，已恢复本次版本写入；重试前重建前端：创建后未找到 Release，请重跑同一指令
```

但现场核对显示发布其实走完了一半：

| 事实 | 证据 |
| --- | --- |
| 版本提交 `5a1299f chore: 发布版本 0.1.4` 已推送，本地 = `origin/master` | `git status -sb`、`git ls-remote` |
| tag `v0.1.4`（annotated `cabf4f2`）已推送 | `git ls-remote --tags origin` |
| 草稿 Release `v0.1.4` 已建成（21:15:31Z，0 资产） | `gh release view v0.1.4 --json isDraft` |
| `release-out/v0.1.4/` 完整（msi 120 651 776 B，哈希与 `SHA256SUMS` 一致） | `sha256sum`、`raybend-build.json` |

## 两个缺陷

### ① 草稿创建后只读一次列表（`scripts/lib/release-publish.mjs`）

`gh release create --draft` 返回 0 之后，`readRemote()` 立刻查
`repos/C-Thun/raybend/releases?per_page=100`，这一次读到空 → 抛「创建后未找到 Release」。

根因是**列表端点对刚创建的草稿有秒级索引延迟**（2026-10-15 那次是 `/releases/tags/<tag>` 对 draft 固定 404，
换成列表端点解决了 404，但没有解决索引延迟）。一次读定生死，抖动一次就中断整条链。

**修法**：create 之后轮询（复用已有的 `wait` 注入，10 次、每次 2s，约 18s）再判失败；
失败文案补上「草稿可能已建成」，仍按 `docs/release.md` §6.5 重跑同一指令（提交/tag/草稿/资产全部幂等复用）。

### ② 发布失败回滚版本写入（`scripts/release.mjs`）

`--publish` 的发布步骤跑在「构建失败 → `rollback()`」的 `try` 里。发布已经把版本提交推送出去，
回滚却把 `package.json` / `Cargo.toml` / `Cargo.lock` 恢复成 0.1.3 —— 工作树比 HEAD 落后三个版本文件。
后果不是「没发出去」，而是**重跑被自己的升版校验拒掉**：`publishRelease` 先按
`versionEdits(base)` 算出「升版后应有的内容」，工作树是 0.1.3 就对不上，报「版本文件含升版以外的改动」。
也就是说报错里那句「重跑同一指令」在实践中不成立，必须先人工 `git restore`。

**修法**：进入发布步骤前置 `handedToPublish = true`；外层 `catch` 见到它就直接重抛，不回滚、不改写文案；
发布步骤自身包一层 `catch`，给出明确语义与可重跑的指令：

```text
发布未完成（构建产物与版本提交保留，不回收工作树）：<原因>
排障后由崔总重跑：pnpm release:publish release-out/v0.1.4 --execute
```

发布仍**持锁执行**（`.release/build.lock` + Windows 镜像锁），所以「发布进行中禁止改工作树」的保护没有放松 ——
只是失败时不再回收版本写入。顺手把 `unlocks.reverse()` 改成 `[...unlocks].reverse()`（不改原数组）。

## 本次发布的实际收口（人为执行）

崔总执行 `pnpm release:publish release-out/v0.1.4 --execute` 前，Agent 先把脚本自己回滚的三个文件恢复成 HEAD
（`git restore -- Cargo.toml Cargo.lock package.json`），并逐字节核对：四个 `RELEASE_SOURCE_FILES` 全部命中
`raybend-build.json` 的 `sourceFiles`，`third-party.json` 的 `locks.cargo` / `locks.pnpm` 与锁文件一致，
`pnpm release:publish release-out/v0.1.4`（只读预览）通过。之后发布一次成功：

- GitHub Release `v0.1.4`：public、Latest、4 个资产（msi / `SHA256SUMS` / `release-index.json` / `raybend-build.json`），
  msi 120 651 776 B（与 `SHA256SUMS` 一致）
- crates.io `raybend 0.1.4`：21:28:29Z 进索引，checksum 核对通过（0.1.3 因核心库无改动被规则跳过）
- 官网：`website.yml` run `41131128302` success（headSha `5a1299f`），部署产物内含 `v0.1.4` 与 msi 直链
- 符号：`release-out/symbols/v0.1.4/`（3 个 PDB，不上传）

## 涉及文件

| 文件 | 改动 |
| --- | --- |
| `scripts/release.mjs` | `handedToPublish` 闸门；发布步骤自带 catch 与重跑指令；`unlocks` 反转不改原数组 |
| `scripts/lib/release-publish.mjs` | create 后轮询 `readRemote()`（10 次 × 2s）再判失败 |
| `scripts/lib/release-run.test.mjs` | 新增「发布失败不回滚版本写入」回归（版本仍为 0.1.1、锁已释放、文案指向 `release:publish`） |
| `scripts/lib/release-publish.test.mjs` | 新增「草稿索引延迟」回归（lag 1/3/9 次仍发布且确实等待、lag 10 次才报错、不重复 create、草稿可见时空等为 0） |
| `docs/release.md` | §6.5 补「发布步骤失败不回滚版本写入」与受伤后的 `git restore` 核对法；§8 坑表合并两种「创建后未找到」 |

## 验证

- `pnpm test:release`：110 tests / 110 pass（含两条新回归；改动前该套件为 108）
- 发布链路的事实核对全部用只读命令完成（`gh release view`、`gh api releases/latest`、`git ls-remote`、
  `curl crates.io/api/v1/crates/raybend`、抓官网部署产物里的 `assets/*.js`）

## 遗留

- 真机 E2E 由崔总执行：安装 `RayBend_0.1.4_x64_en-US.msi`，确认安装、启动、浏览/编辑基本可用（未做，属人类验收）。
- 一键发版在「发布步骤之后」失败仍不会回收 `release-out/`（有意如此：产物已收口、可能已上传）；
  重新发同版本号需先核对 `release-out` 是否该保留 —— 由人判断，脚本不擅自删。
