# M5 · crates.io 核心库同步发布（`specs/m5-crates-publish.md`）

状态：**已实现并冒烟**（2026-10-09）；首次发布由崔总手工完成（`raybend 0.1.1`，2026-10-09T13:59:06Z）。
归属：M5 分发与发布（`memory/PLAN.md` §3）。操作说明在 `docs/release.md` §9。

## 1. 目标与非目标

**目标**：正式发版时把核心库 `crates/raybend` **同步**发到 crates.io——一个动作发全部，且**永不重复、永不覆盖**。

**非目标**：

* 不做「crate 版本与产品版本解耦」：两条版本线同号，同步由现有 `versionEdits()`
  （`scripts/lib/release-files.mjs`，改 `package.json` + `Cargo.toml [workspace.package]` + `Cargo.lock`
  并断言一致）保证；`crates/raybend` 与 `src-tauri` 都 `version.workspace = true`。
* **不发预发布版**（beta / test）：beta 迭代频繁，而 crates.io 版本号不可撤回、只能 yank，占上去就回不来。
* 不代替人类动作：提交 / tag / push / GitHub Release / crates.io 全部只发生在崔总显式
  `release:publish <目录> --execute` 的路径里（AGENTS.md §2.1）。
* 不接 Trusted Publishing（CI 免 token）——首次发布用不上（crate 必须先存在），是否启用属发布方式变更。
* 不涉及命令注册表：这是发布工具链，不是用户可触发功能（§2.15 不适用）。

## 2. 触发与顺序（唯一入口）

```bash
pnpm release <patch|minor|major> [--win-*]            # 准备：同步版本、构建产物；无外部副作用
pnpm release:publish <release-out/vX> --execute       # 发布：提交 → tag → 原子 push → Release 公开 → crates.io → 官网等待
```

`crates/raybend` 是**倒数第二段**：GitHub Release 公开之后、官网工作流等待之前。三条理由：

1. **最不可撤回的一步放最后**：前面每一步（提交来源核对、资产字节核对、Release 公开）都已经验完。
2. **可追溯**：crate 的 `.cargo_vcs_info.json` 记录 `HEAD` 的 commit sha，此时 `HEAD` 正是 tag 指向的
   发布提交（0.1.1 实测记的 `acd1324` 就是当时的 `HEAD`）。
3. **失败可续**：官网等待只是监控，失败不该拖住核心库；crate 失败时 Release 已公开，重跑同一指令只补未完成的部分。

## 3. 判定与幂等（唯一判据）

注册表真相源用 **sparse index**：`https://index.crates.io/ra/yb/raybend`（`indexPath()` 负责 1/2/3/4+ 位的
路径规则）。用 `curl` 同步读（发布链全是同步代码，不把 async 传染进来）；索引读不到时降级为警告。

| 索引状态 | 动作 |
| --- | --- |
| 没有该版本 | `cargo publish -p raybend` |
| 有该版本，本地重打包 sha256 **等于**索引 `cksum` | 视为已发布，**跳过**（重跑安全） |
| 有该版本，sha256 **不等** | **硬报错**：内容不同、版本号不可覆盖，只能升版 |
| 该版本已 `yanked` | **硬报错**（版本号不可复用） |
| 索引不可读 / 未响应 | 警告后照发；重复由 cargo 拒绝；若 cargo 报「已上传」再回到上表的比对 |

* 「本地重打包」= `cargo package -p raybend --no-verify`（不编译，秒级），产物 `target/package/raybend-<v>.crate`。
  **依据**：`cargo package` 在同一 commit 上字节确定（2026-10-09 同日两次 sha256 相同），
  且发布出去的 0.1.1 与本地重打的 sha256 一致（`12119d2c…`，索引 `cksum` 与 `crate_size` 双向对上）。
* 上传后轮询索引（最多 10 次 × `wait()`，默认 2s）核对 `cksum`；只警告、不把延迟当失败。
* 凭证前置检查：`~/.cargo/credentials.toml` 或 `CARGO_REGISTRY_TOKEN` 缺一不可，缺了在**动手前**报错
  并提示 `cargo login`（不带参数，token 从 stdin 读）。

## 4. 开关（崔总 2026-10-09 拍板）

* 正式版（`channel === release`）**默认发**；`--no-crates` 跳过。
* beta / test **不发**。
* 不带 `--execute` 的预览**保持零联网、零 git 写入**：只打印「执行时会做什么」，含 crate 那一段的结论。

## 5. 前提与已知事实

* crates.io 首次发布硬门槛：**它自己的邮箱必须已验证**（400 `A verified email address is required`），
  GitHub 侧验证过不算；失败无副作用（未落盘、token 不受影响）。
* 单包上限 10 MiB；本包 1.1 MiB / 236 文件。`crates/raybend/assets/ai/` 的 ONNX 模型与 DLL 被
  `.gitignore` 排除，不进包。
* docs.rs 异步构建，失败不影响发布本体。
* 发布账号目前是崔总本人账号（crates.io 用户 `andares`）；将来要加协作 owner 时在 crate 设置里加人。

## 6. 落点

| 文件 | 职责 |
| --- | --- |
| `scripts/lib/crates-release.mjs` | 索引读取、`crateEligibility()`、`publishCrate()`（含幂等与降级） |
| `scripts/lib/release-publish.mjs` | 在 `publishRelease()` 里按 §2 的顺序调用；`noCrates` / `crates` 注入 |
| `scripts/release-publish.mjs` | CLI：`--no-crates`（与 `--execute` / `--dry-run` 可任意顺序组合） |
| `scripts/release.mjs` | 准备阶段打印本通道是否会同步核心库 |
| `scripts/lib/json-text.mjs` | `parseJson(text, label)`：发布链读外部 JSON 失败时点名来源 |

## 7. 验收清单

- [x] 单测（`scripts/lib/crates-release.test.mjs`，10 例）：索引路径/解析、四项判定、yank、降级、
      凭证缺失、索引延迟、cargo 报「已上传」的回归
- [x] 接线（`scripts/lib/release-publish.test.mjs`）：正式版发一次且位置正确、`--no-crates` 跳过、beta 不发、
      预览不触发
- [x] `pnpm test:release` 90 通过 / `pnpm test` 1239 通过 / `pnpm typecheck` / `lint:arch|i18n|colors`
- [ ] **真实发版验证**：下一次正式发版时由崔总执行，观察核心库是否同步出现在 crates.io（人类动作）
- [ ] docs.rs 页面确认（异步，事后核对）
