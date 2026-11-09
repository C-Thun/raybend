# 地基升级第一轮：TypeScript 7 / Tauri 2.12 / Rust 1.99.1

> 完成时间：2026-11-10 03:21:40

按 `implementations/2026-11-10_dependency-baseline-survey.md` 的 P0 清单执行。**只动地基，不动业务代码**。

## 1. 改了什么

### 前端（`package.json` / `pnpm-lock.yaml`）

| 包 | 旧 | 新 |
| --- | --- | --- |
| typescript | 6.0.3 | **7.0.2** |
| solid-js | 1.9.15 | 1.9.18 |
| @solidjs/router | 1.0.0 | 1.0.1 |
| @ark-ui/solid | 5.39.2 | 5.40.0 |
| @tabler/icons-solidjs | 3.46.0 | 3.51.0 |
| @tauri-apps/api | 2.11.1 | 2.12.4 |
| @tauri-apps/cli | 2.11.4 | 2.12.4 |
| @tauri-apps/plugin-dialog | 2.7.3 | 2.8.4 |
| vite | 8.3.0 | 8.3.7 |
| vite-plugin-solid | 2.11.14 | 2.11.15 |
| tailwindcss / @tailwindcss/vite | 4.3.3 | 4.3.5 |
| @fontsource-variable/noto-sans-sc | 5.3.0 | 5.3.1 |
| @types/node | 26.5.1 | 26.6.6 |

`pnpm outdated`（根）现在为空。`packageManager` 保持 `pnpm@12.9.1` **未动**（崔总指定：pnpm 本身由他管）。

### Rust

- `rust-toolchain.toml`：`1.98.1` → **`1.99.1`**；`Cargo.toml` 的 `[workspace.package] rust-version`：`1.98` → **`1.99`**（两处必须同改，否则依赖解析会回退）。
- `cargo update`（161 个包），关键项：
  - **tauri 2.11.5 → 2.12.5**、tauri-build 2.6.3 → 2.7.4、tauri-runtime(-wry) 2.12.x
  - wry 0.55.1 → 0.57.1、tao 0.35.3 → 0.37.2
  - tauri-plugin-updater 2.12.0 → 2.14.1、dialog 2.7.3 → 2.8.4、single-instance 2.4.5 → 2.5.5
  - trash 5.2.9 → 5.3.0、kamadak-exif 0.6.1 → 0.6.2、notify 8.2.0 → 8.2.2、flate2 1.1.11、serde_json 1.0.154、tokio 1.54.1、rustls 0.23.46
- `src-tauri/Cargo.toml` 的三处**联动 pin**（这是本轮唯一需要判断的地方）：
  - `webview2-com = "=0.38.2"` → **`"=0.39.1"`**
  - `windows-core = "=0.61.2"` → **`"=0.62.2"`**
  - `minisign-verify = "0.2"` → **`"0.3"`**
  - 理由：tauri 2.12 起用 `webview2-com 0.39 + windows 0.62`；tauri-plugin-updater 2.14 内部用 `minisign-verify 0.3`。
    不同步就会出现**同一 crate 的两份副本**，`desktop_behavior.rs` 里 `ICoreWebView2Settings3` 的 COM 接口会因类型不同而编译失败。
    已在 `src-tauri/Cargo.toml` 的注释里写好这条纪律，避免下次再踩。

## 2. 关键决策

1. **TS 7 直接上**：升之前已用 `pnpm dlx` 的临时副本对现仓跑过 typecheck（0 error / 0.74 s），
   并确认仓内没有任何 `import "typescript"` 的工具（TS 7.0 不带 compiler API）。升级后 `pnpm typecheck` 0 error。
2. **Rust 1.99.1**：先把工具链装好（`rustup toolchain install 1.99.1`）再改两个声明，避免 cargo 因缺工具链报错。
3. **Tauri 2.12**：主因是 2.12.0 的安全修复 GHSA-w28w-mhc8-qvjv（我们没用 `ipc::Channel`，属于白送的修复）；
   顺带拿到 `noRedirectionBitmap` 等 Windows 配置项，后续透明窗 + wgpu 直绘可评估。
4. **不升**：Solid 2（rc.21）、Ark 6（next）、Tauri 3（alpha.6）、rusqlite 0.41、sha2 0.11、roxmltree 0.21、
   zip 9——理由见调研报告 §2 / §3.6，留给下一轮。
5. **pnpm 不动**：崔总定，pnpm 是全局工具，由 pnpm 自己提示升级。

## 3. 涉及文件

- `package.json`、`pnpm-lock.yaml`
- `Cargo.toml`（rust-version）、`Cargo.lock`、`rust-toolchain.toml`
- `src-tauri/Cargo.toml`（tauri 插件版本 + 三个联动 pin）
- `AGENTS.md` §3 版本基线表、`memory/ARCHITECTURE.md` §4 版本基线表（同步新版本，并加了一行 TypeScript）
- **新增 `pnpm-workspace.yaml`**（见 §5 第 1 条）

## 4. 验证

| 门 | 命令 | 结果 |
| --- | --- | --- |
| 类型 | `pnpm typecheck`（TS 7.0.2） | ✅ 0 error，~4 s（含 pnpm 开销） |
| 前端单测 | `pnpm test` | ✅ **1432 passed / 0 failed**（4.9 s） |
| 架构/文案/配色 | `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n` | ✅ 三条全绿 |
| 前端构建 | `pnpm build` | ✅ 4.55 s（rolldown） |
| Rust 编译 | `cargo check --workspace`（1.99.1，全量重建） | ✅ 5 min 06 s，只有既有的 dead_code 警告 |
| Rust 测试 | `pnpm test:rust`（= `cargo test --workspace`） | ✅ 200 + 2 + 2 doctest 全绿（11 min 44 s，含全量编译） |

## 5. 遗留问题

1. **`pnpm-workspace.yaml` 是 pnpm 自己生成的**，内容只有 `minimumReleaseAgeExclude`（vite@8.3.7 / rolldown@1.2.18
   那两个当天发布的包）。pnpm 12 的供应链策略默认不接受太新的包，遇到显式 pin 的新版本就记进这个豁免表。
   我按「pnpm 管自己的产物」保留了它（与 `packageManager` 同性质）。**如果崔总不想放宽这条策略，
   把 vite 退回 8.3.6 并删掉该文件即可**——两行就能回退。
2. **Windows 侧要人工做一步**：`scripts/release.mjs` 会校验 Windows 上 `cargo-tauri` 与前端 CLI **版本严格相等**，
   CLI 已到 2.12.4，需执行 `cargo install tauri-cli --version 2.12.4 --locked`（仅崔总能做）。
3. **zip 现在有两份**：我们诊断包用 `zip 4.6.1`，`tauri-plugin-updater 2.14` 在 Windows 目标下要 `zip 9.5.0`
   （`cargo tree -i zip@9.5.0 --target x86_64-pc-windows-msvc` 已确认）。不是错误，只是多编一份；
   P1 把 `diagnostics/bundle.rs` 一起挪到 zip 9 就能去掉。
4. **未做 Windows 真机构建**：本轮只跑了 Linux 侧 check/test/build。透明窗、wgpu 直绘、更新器这些
   与 Tauri 版本强相关的部分，要 `pnpm debug:win` + `pnpm check:win` 才算验过（归崔总）。
5. P1 待办（rusqlite 0.41 / sha2 0.11 / roxmltree 0.21 / base64 0.23 / pollster 1.0 / dav1d 1.5.4）见调研报告 §4。
