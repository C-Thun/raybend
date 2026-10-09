# crates.io 发布复验：0.1.1 打包可发 + crate 页 README + 发布步骤固化

完成时间：2026-10-09 12:58:11 CST

## 目的

崔总问「发 raybend 的 rust 包占位，哪些流程需要我亲自做」。2026-09-27 的准备
（`implementations/2026-09-27_crates-io-publish-prep.md`）之后过了一个波次，代码与资产都变了
（相片整理 AI、色彩管理、CLI；版本 0.1.0 → 0.1.1），**当时的 dry-run 证据已经过期**，
本次只做三件事：复验当前 revision 仍可发布、补 crates.io 页面用的 README、把发布步骤写进发布手册。

## 改动范围

1. **新增 `crates/raybend/README.md`**（cargo 自动识别为 `readme`，进包后在 crates.io 页面上呈现）。
   内容：这是什么 / **不是**什么（不是可独立运行的应用、0.x 不给 SemVer 承诺、不建议外部依赖）、
   模块速览表、许可。仓库根 README 是给用户看的（下载、安装），crate README 是给 crates.io 访客看的，
   两者受众不同，不算重复（§2.12 判据是「同一能力的第二套实现」，这里不是）。
2. **`docs/release.md` 新增 §9「crates.io 核心库发布（`cargo publish`）」**：首次一次性步骤
   （GitHub OAuth 登录 → 建 token → `cargo login`）、每次发布两条命令、干净树要求、
   不可撤回语义、10 MiB 体积红线、docs.rs 失败不影响发布。发布手册是这条流程的长期住址，
   `implementations/` 只留这次的复验证据。

## 验证

* `cargo publish -p raybend --dry-run --allow-dirty` ✅
  —— `Packaged 236 files, 3.9MiB (1.1MiB compressed)`；验证编译 3m17s 通过
  （`Compiling raybend v0.1.1 (…/target/package/raybend-0.1.1)`）；收尾
  `Uploading raybend v0.1.1` → `warning: aborting upload due to dry run`（未上传，符合 dry-run 语义）。
* `target/package/raybend-0.1.1/Cargo.toml`（cargo 重写后的发布清单）✅
  `name = raybend` / `version = 0.1.1` / `readme = "README.md"` / `license = "AGPL-3.0-only"` /
  `repository` / `keywords` / `categories` 齐；workspace 继承已在打包时展开。
* 体积红线 ✅ 1.1 MiB ≪ crates.io 单包上限 10 MiB。
  `crates/raybend/assets/ai/` 下 158.9 MB 的 `image_encoder.onnx` 与 15.8 MB 的 `onnxruntime.dll`
  被 `.gitignore` 排除（`git check-ignore -v` 逐条确认），**没有进包**——这是本次最需要排除的假阻塞：
  `du -sh assets` 有 168 MB，只看目录会误判「超限」。
* 名字仍未被占用 ✅ `GET https://crates.io/api/v1/crates/raybend` → `crate 'raybend' does not exist`（2026-10-09 查）。

## 崔总需要亲自做的（发布本体，AGENTS.md §2.1）

1. 用 GitHub 账号登录 https://crates.io （**没有独立注册**，OAuth 即账号；GitHub 邮箱须已验证）。
2. 在 https://crates.io/settings/tokens 建 token：作用域 `publish-new` + `publish-update`，
   crate 范围 `raybend`，有效期取最短；token 只显示一次。
3. 本机 `cargo login <token>`（当前 `~/.cargo/credentials.toml` 不存在 = 还没登录过）。
4. 干净树上 `cargo publish -p raybend`。

## 发布前最终确认（2026-10-09 21:40:51 CST，去掉 `--allow-dirty` 后的第一次真闸门复验）

`crates/raybend` 内已提交干净（提交 `ae07a01` / `4947fcd`），于是在**干净树**上重跑了不带 `--allow-dirty` 的检查：

* `cargo publish -p raybend --dry-run` ✅ 无脏树报错；`Packaged 236 files, 3.9MiB (1.1MiB compressed)`；
  验证编译 9.78s（`target/package/raybend-0.1.1` 已热）；收尾 `Uploading raybend v0.1.1` →
  `warning: aborting upload due to dry run`。**这是发布前的通过判据。**
* `cargo test -p raybend --lib` ✅ 1461 通过 / 0 失败 / 12 ignored（15.17s）。
* `cargo test -p raybend --test raw_worker` ✅ 7 通过 / 0 失败（worker 协议 v7 握手正常）。
* 名字 2026-10-09 21:40 仍未被占（`does not exist`）。
* 待发布版本：**`0.1.1`**（`Cargo.toml` 里 `version.workspace = true`）。

结论：**可以发布**，只剩崔总的 `cargo publish -p raybend` 本体（§2.1）。

## 遗留 / 未决

* **工作树是脏的**（41 个文件未提交，M3-W5/编辑预设那一批）：`cargo publish` 会因此拒绝。
  崔总 2026-10-09 拍板：**等这批做完、提交之后再发**，不加 `--allow-dirty`。
  届时版本号按当时的 `Cargo.toml` 走（现在 0.1.1），发布动作仍由崔总执行（§2.1）。
* docs.rs 可能因 Linux 侧系统库缺失而构建失败：本机 `/usr/lib/x86_64-linux-gnu/libdav1d.so` 存在，
  所以本地 Linux 编译能过；docs.rs 容器是否装了 dav1d 未知。失败只影响页面上的 API 文档。
* `LICENSE` 文本未打进包（沿用 2026-09-27 的决定：`license` 字段已声明，根目录是唯一文本，不制造第二份拷贝）。
* 与本次无关的既有诊断：`vite.config.ts:1` import `./scripts/lib/ai-build.mjs` 缺类型声明，
  该文件在 HEAD 里（2026-10-04 的 AI 提交），非本次引入，`pnpm build` 正常。
