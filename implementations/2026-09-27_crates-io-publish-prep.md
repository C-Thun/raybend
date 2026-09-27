# crates.io 发布准备：占名 `raybend`（元数据补全 + 契约 JSON 迁入 crate）

完成时间：2026-09-27 15:36:15 CST

## 目的

崔总拍板：把核心库 `crates/raybend` 发上 crates.io，**主要目的是占名**（发布的是真实核心库源码，
不是空壳，符合 crates.io 反占名政策——纯占坑包可能被官方回收名字）。发布本体（`cargo publish`）
按 `AGENTS.md` §2.1 由崔总执行，本次只做发布前准备并验证。

## 改动范围

### 1. 元数据（commit bb3d243）

* `src-tauri/Cargo.toml`：`[package]` 加 `publish = false` —— 桌面外壳不进 crates.io
  （安装包走 M5 分发链路，见 `docs/release.md`），此字段是防误发守门；工作区里只有
  `crates/raybend` 允许发布。
* `crates/raybend/Cargo.toml`：补 `keywords`（photo / photography / raw / image）与
  `categories`（multimedia::images），crates.io 页面呈现用。其余元数据原本已齐
  （description / license=AGPL-3.0-only / repository / version 0.1.0）。
* 名字核查：`raybend` 在 crates.io 未被占用（API 返回 does not exist，2026-09-27 查）。

### 2. 契约 JSON 迁入 crate（commit de35a07）—— dry-run 抓出的真阻塞

`cargo publish --dry-run` 首跑即失败：crates.io 发布包**只能含 crate 目录内的文件**，
而两条 `include_str!` 用 `../../..` 逃出 crate 引前端目录里的文件，发布包里必然编译不过：

* `develop/params.rs` → `src/api/develop-params.json`（显影参数数字契约，唯一真相）
* `develop/curve.rs` → `src/lib/curve-vectors.json`（曲线求值外部测试向量）

处理：**唯一真相搬进 `crates/raybend/assets/`**（该目录已在发布包内），前端跨树 import 同一份，
不做同步副本（§2.12：两份拷贝 = bug）。`dto-contract.json` Rust 侧未 include，不受影响。

涉及文件：

* `git mv src/api/develop-params.json crates/raybend/assets/develop-params.json`
* `git mv src/lib/curve-vectors.json crates/raybend/assets/curve-vectors.json`
* Rust：`develop/params.rs`（include 路径 + 模块头注释）、`develop/curve.rs`（同）、
  `develop/mod.rs`、`store/develop.rs`、`store/migrations/catalog_0005_develop.sql`（注释里的路径）
* TS：`features/editor/params.ts`（import + 头注释）、`features/editor/model.test.ts`、
  `lib/curve.test.ts`（import + 头注释）、`lib/curve.ts`、`features/editor/CurveEditor.tsx`（注释）
* 两份 JSON 的 `_comment` 头部各加一行住址说明（含 crates.io 约束原因）

跨树 import 的可行性依据：`tsconfig` 无 `rootDir` 限制且 `resolveJsonModule: true`；
Vite 未自定义 `server.fs.allow`（默认工作区根 = 仓根）；`scripts/check-architecture.mjs`
对解析到 `src/` 之外的 import 不检查。历史档案（`specs/M3-W3.md`、`implementations/*`）
里的旧路径不改（当时的真实路径）。

## 验证

* `pnpm typecheck` ✅
* `pnpm lint:arch` ✅
* `pnpm test` ✅ 1071 通过 0 失败（含对着新路径断言的 `curve.test.ts` / `model.test.ts`）
* `cargo test -p raybend --lib` ✅ 1173 通过 0 失败（含 `develop::params` / `develop::curve`
  对契约 JSON 的两条对齐断言）
* `cargo publish -p raybend --dry-run` ✅ 验证编译通过（1m15s），走到
  `Uploading raybend v0.1.0` 后按 dry-run 语义止步；打包清单 159 个文件，
  两份 JSON 确认在包内，`include_str!` 所需资源（wgsl / sql / webp / json）齐备，无警告

## 崔总执行发布时的步骤（尚未执行）

1. 用 GitHub 账号登录 https://crates.io （crates.io 无独立注册，OAuth 即用；GitHub 邮箱需已验证）
2. 在 https://accounts.crates.io 创建 API Token（publish-new / publish-update）
3. 本机 `cargo login <token>`（凭据自管）
4. `cargo publish -p raybend`

注意：crates.io 版本**发出去即不可覆盖/不可删除**（只能 yank）；0.1.0 发布后如需迭代就升版本号。

## 遗留 / 已知事项

* docs.rs 在线文档构建可能因 `image` 的 `avif-native`（dav1d 系统库）失败 —— 不影响发布本体，
  只是 crates.io 页面上可能没有自动生成的文档。
* 发布包会带上 `raybend-raw-worker` bin（`src/bin/`，crates.io 打包 bin 目标是默认行为，无害）。
* LICENSE 文本未打进包（`license` 字段已声明 AGPL-3.0-only，repository 指向源码仓）；
  根目录 LICENSE 是唯一文本，避免第二份拷贝漂移。
