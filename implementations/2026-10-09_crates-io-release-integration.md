# crates.io 0.1.1 上线 + 核心库同步发布接入发版流程

完成时间：2026-10-09 22:18:40 CST

## 一、`raybend 0.1.1` 已上线（崔总手工执行）

时间线：2026-10-09 21:59 CST（crates.io 记 `2026-10-09T13:59:06Z`）发布成功；第 3 次尝试才过——
第 1 次 400「未验证邮箱」（见 `implementations/2026-10-09_crates-io-publish-recheck.md`），
第 2 次在 crates.io 设置里验证邮箱后直接成功。发布账号是崔总本人账号（crates.io user `andares`，id 6674）。

落地核对（只读 API）：

| 项 | 值 |
| --- | --- |
| 版本 | `0.1.1`（`max_version` / `newest_version` 一致，无其它版本） |
| keywords / categories | `image, photo, photography, raw` / `multimedia::images` |
| checksum / 体积 | `12119d2cba3ae7b8c3fcb2d0ada27333124eec8acaf37d74ba567ad9004c67c1` / 1 144 195 B |
| 包内来源 | `.cargo_vcs_info.json` → `acd1324`（= 发布时的 `HEAD`，即发布前最后一次提交） |
| docs.rs | **构建成功**（`https://docs.rs/raybend/0.1.1/raybend/` 200，列出 Modules/Enums）——原先担心的 Linux 系统库问题没有发生 |

`src-tauri` 的 `publish = false` 守门生效：工作区里只有 `crates/raybend` 能发。

## 二、两条实测事实（接入设计的依据）

1. **`cargo package` 在同一 commit 上字节确定**：同一 `HEAD` 连打两次，sha256 完全相同
   （`12119d2c…`）。tarball 里没有会漂移的时间戳/随机量。
2. **本地重打包 == 已发布内容**：本地重打的 `.crate` sha256 与 crates.io 索引里的 `cksum` 完全一致。

于是「重跑发布指令」有了硬判据：拿索引里的 `cksum` 与本地重打包比对即可判断「是不是同一份东西」。

（对比一下另一条路：`cargo publish --dry-run` 只证明「能打包能编译」，不证明「与线上是同一份」，
且要在干净树上跑；上面这两条才让幂等成立。）

## 三、接入发版流程（崔总 2026-10-09 拍板：正式版默认发；beta 不发）

规格 `specs/m5-crates-publish.md`，操作说明 `docs/release.md` §9。

* **入口唯一**：`pnpm release:publish <release-out/vX> --execute`。`pnpm release`（准备）只多打印一行
  「本通道会不会同步核心库」；真正发出去仍在人类显式执行的那一步（AGENTS.md §2.1）。
* **顺序**：GitHub Release 公开之后、官网工作流等待之前。理由——最不可撤回的一步放最后；
  此时 `HEAD` 就是 tag 指向的发布提交（crate 可追溯，见上文 `.cargo_vcs_info.json`）；
  官网等待只是监控，失败不该拖住核心库，而 crate 失败时重跑同一指令只补未完成的部分。
* **开关**：正式版默认发，`--no-crates` 跳过；beta / test 不发；预览（不带 `--execute`）**保持零联网零 git 写入**，
  只打印执行时的计划（含 crate 那一段的结论）。
* **幂等**：索引无该版本 → `cargo publish`；有且 `cksum` 与本地重打包一致 → 跳过；
  有但不等 → 硬报错（版本号不可覆盖，只能升版）；已 yank → 硬报错；
  索引不可读 → 警告后照发，重复由 cargo 拒绝（若 cargo 报「已上传」再回到比对）。
  上传后轮询索引最多 10 次 × 2s 核对，索引延迟只警告不判失败。
* **凭证前置**：`~/.cargo/credentials.toml` 或 `CARGO_REGISTRY_TOKEN` 缺失 → 动手前报错（不会发一半）。

落点：

| 文件 | 改动 |
| --- | --- |
| `scripts/lib/crates-release.mjs` | 新增：索引读取（curl 经注入 `run`）、`crateEligibility()`、`publishCrate()` |
| `scripts/lib/crates-release.test.mjs` | 新增：10 例离线回归 |
| `scripts/lib/release-publish.mjs` | 接入调用点与 `noCrates` / `crates` 注入；预览文案；收尾汇总行 |
| `scripts/lib/release-publish.test.mjs` | 夹具参数化（版本/通道）＋ 2 例接线回归 |
| `scripts/release-publish.mjs` | CLI 允许 `--no-crates`，三个开关可任意顺序组合 |
| `scripts/release.mjs` | 准备阶段打印本通道是否同步核心库 |
| `scripts/lib/json-text.mjs` | 新增 `parseJson(text, label)`：发布链读外部 JSON 失败时点名来源 |

**版本号同步无需新增**：`versionEdits()`（`release-files.mjs`）已经把 `package.json`、
`Cargo.toml [workspace.package]`、`Cargo.lock` 三处一起改并断言 `package.json` 与 workspace 一致；
`crates/raybend` / `src-tauri` 都 `version.workspace = true`，`tauri.conf.json` 指向 `../package.json`。
所以「release 时同步所有版本号」本来就已经成立，这次只补上「把 crate 发出去」。

顺带：把 `release.mjs` / `release-publish.mjs` 里三处裸 `JSON.parse` 收敛到 `parseJson()`（§2.12 单一实现），
坏文件时报出是哪个来源坏了，而不是没有上下文的 SyntaxError。

## 四、验证

* `node --test scripts/lib/crates-release.test.mjs` ✅ 10 通过（索引路径/解析、四项判定、yank、降级、
  凭证缺失、索引延迟、cargo 报「已上传」）
* `node --test scripts/lib/release-publish.test.mjs` ✅ 8 通过（含新增 2 例：正式版发一次且位置正确、
  `--no-crates` 跳过、beta 不发、预览不触发）
* `pnpm test:release` ✅ 90 通过 0 失败
* `pnpm test` ✅ 1239 通过 0 失败
* `pnpm typecheck` ✅ ｜ `pnpm lint:arch` ✅ ｜ `pnpm lint:i18n` ✅ ｜ `pnpm lint:colors` ✅

## 五、遗留 / 未决（都不是本轮的欠账）

* **真实发版验证**：下一次正式发版由崔总执行，观察核心库是否同步出现在 crates.io（规格 §7 未勾）。
  本轮所有验证都是离线合成回归，**没有真的走一遍带 crate 的完整发布**。
* **Trusted Publishing**（CI 免长期 token）未启用：首次发布用不上（crate 必须先存在），
  是否启用属发布方式变更，要崔总拍板。
* **crate owner 目前是个人账号** `andares`；要加协作 owner 得在 crates.io 设置里加人（不阻塞）。
* **docs.rs**：本次构建成功，但它会随依赖与系统库变化；失败只影响页面上的 API 文档。
* `crates/raybend/assets/ai/` 的 ONNX 模型与 DLL 不进包（`.gitignore` 排除，1.1 MiB ≪ 10 MiB 上限）——
  对外发布的 crate 因此不带本地 AI 推理资源，`ai` 特性只对源码仓内的构建有意义。
