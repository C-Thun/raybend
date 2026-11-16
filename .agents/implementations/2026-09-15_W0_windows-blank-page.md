# W0：修复 Windows 版页面打不开（P0）

完成时间：2026-09-15 16:22:29 CST

## 本次改动的范围

修复一个**从 M0-1 就存在、直到现在才暴露**的构建配置缺陷。**已由用户目视确认修复成功**。

## 现象与根因

| 平台 | 构建方式 | 修前 |
| --- | --- | --- |
| WSL/Linux | `pnpm tauri dev` | ✅ 正常 |
| Windows | 裸 `cargo build` + 直接跑 exe | ❌ 窗口出现了，但页面打不开 |

### 根因（已定位到源码级）

```rust
// tauri-2.11.5/build.rs:256
let custom_protocol = has_feature("custom-protocol");
let dev = !custom_protocol;        // ← dev 由 feature 决定，不是 debug/release！
```

```rust
// tauri-codegen-2.6.3/src/context.rs:176
let assets = if let Some(assets) = assets {
    quote!(#assets)
} else if dev && config.build.dev_url.is_some() {
    let assets = EmbeddedAssets::default();   // ← dev=true 时嵌入资源是空的
    quote!(#assets)
} else {
    // 从 frontend_dist 真正嵌入资源
};
```

调用链：`cargo build`（无 Tauri CLI）→ `custom-protocol` feature 未开 → `dev = true`
→ **嵌入资源为空** → webview 去连 `devUrl`（`http://localhost:1420`）→ 本机无服务 → 页面打不开。

### 为什么 `custom-protocol` 会缺失

`create-tauri-app` 模板自带：

```toml
[features]
custom-protocol = ["tauri/custom-protocol"]
```

**M0-1 的「删净模板演示」把它当成模板残留一并删掉了。** 模板里其实有注释警告不要删，但被忽略了。

## 改动

| 文件 | 改动 |
| --- | --- |
| `src-tauri/Cargo.toml` | 补回 `[features]` 段与 `custom-protocol = ["tauri/custom-protocol"]`，并写明为什么不能删 |
| `AGENTS.md` §5.3 | 构建命令改为 `cargo build -p raybend-desktop --features custom-protocol`；硬规矩由 3 条扩为 5 条 |
| `AGENTS.md` §5.3.1 | 新增：源码级根因说明 + 四条路径的表现对照表 |
| `AGENTS.md` §5.3.2 | 新增：Windows 侧验证纪律（必须确认「内容看得见」） |
| `AGENTS.md` §5.3.3 | 新增：本次「吃一堑」汇总（4 条） |

## 验证方式

### 1. 决定性检查：嵌入资源是否存在

```bash
EXE=/mnt/c/rb-target/raybend/debug/raybend-desktop.exe
strings -n 6 "$EXE" | grep -c "index-Ch4j6gKZ.js"   # → 1 ✓
```

**为什么这个检查是决定性的**：`index-Ch4j6gKZ.js` 是 `16:16` 刚由 `pnpm build` 生成的新文件名，
而 exe 是 `16:19` 构建的。**若嵌入资源为空（`EmbeddedAssets::default()`），这些文件名根本不会出现在二进制里。**
命中即证明资源确实被扫描并嵌入了。

> **注意**：搜 JS 的**原始字节**搜不到是正常的 —— Tauri 嵌入时会做 brotli 压缩。
> 我第一次就是这么误判的，差点以为没修好。

### 2. 时间戳检查

产物 `16:19:37` > `dist/` `16:16:44` ✓（修前是 exe `02:50` 早于 dist `10:28`）

### 3. 人工目视（**E2E，归用户**）

启动 exe → **用户确认「窗口内容是有的了」** ✓

## 本次「吃一堑」汇总

| # | 坑 | 后果 | 教训 |
| --- | --- | --- | --- |
| 1 | 清理模板时删掉了 `[features] custom-protocol` | Windows 版一直白屏，而 WSL 侧因走 `pnpm tauri dev` 完全正常，问题被掩盖很久 | **「模板自带」不等于「模板残留」**。删配置性代码前先搞清用途；CLI 才注入的东西，走裸 cargo 就必须自己写明 |
| 2 | 验证只看 `MainWindowHandle` / `MainWindowTitle` | 曾声称「Windows 运行验证通过」，而页面根本打不开 | **「窗口出现了」不等于「功能对了」**。验证清单必须含「内容看得见」 |
| 3 | 用 `cmd.exe /c "start \"标题\" 路径"` 启动 exe | 弹「Windows 找不到文件 `\RayBend\`」，干扰到用户 | WSL→Windows 的**引号嵌套不可靠**；直接用 `/mnt/c/...` 跑 |
| 4 | exe 比 `dist/` 旧 | 即使逻辑正确，跑的也是旧前端 | **产物时间戳必须晚于 `dist/`** |

前两条已写进 `AGENTS.md` §5.3.1 / §5.3.2，后两条写进 §5.3.3。

## 遗留问题

- **计划中的其余步骤尚未执行**（Step 6–19：设计系统落地）。当前进度见 `plans/M1.md` 的勾选状态。
- 本次改动**尚未提交**（处于 plannotator 执行流程中，按全局纪律不自动 commit）。
