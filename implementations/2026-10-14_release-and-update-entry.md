2026-10-14 22:17:15

# 发版与更新链路收口：指定版本号、`--publish` 一键、启动静默检查、AI 同级探测

崔总 2026-10-14 提出四件事：① 评估 CI 打包；② 一键发版（含指定版本号）；③ 启动自动检查更新；
④ AI 外挂编译流程摸清并写进 readme。本记录是这轮的实现与结论；调研阶段核实了既有链路
（两条指令发布、官网 `release:published` 自动更新、Tauri updater 接线、`--ai` 外挂都已存在）。

## 结论先说：CI 打包

**技术可行，但对现状不划算，暂缓。** 真正的卡点不是构建能力（Rust 工具链锁定、runner 自带 MSVC/WiX、
dav1d 可用 meson+ninja+nasm 重建并缓存、onnxruntime DLL 已有固定 URL + SHA-256 自动下载），而是
**152 MB 的 ONNX 模型在独立的 `model-registry` 仓库（git LFS）**：CI 要出 AI 版得先定模型的公开分发方式，
否则只能出不带 AI 的版本 —— 与本地主力包不同质、两套口径。其次是现有 WSL→Windows 镜像/wslpath/锁逻辑
在 CI 里要重写、并加 Secrets 与发布纪律边界重划。评估正文写进 `docs/release.md` §11，
`memory/FUTURE.md` G24 补了结论并把触发条件改为「模型公开分发方式定下来」或「有了云签名」。

## 改动范围

### 1. 指定版本号（`src/lib/release-plan.ts` / `.test.ts`）

- `parseReleaseArgs` 接受版本号作位置参数（`pnpm release 0.2.0`）或 `--version v0.2.0`；与 bump 词互斥。
- 新增 `normalizeTargetVersion`（接受前导 `v`）与 `compareVersions`（完整语义化比较，含预发布规则）。
- `createReleasePlan`：指定版本必须**严格大于**当前版本（降级直接拒绝）；beta 通道要求目标版本自带
  预发布标签；test 通道不接受指定版本号。
- 于是「自增（major/minor/patch）」「指定版本号」两种模式都在，且都不绕过安全校验。

### 2. `--publish` 一键发版（`scripts/release.mjs` / `release-run.test.mjs` / `release-plan.ts`）

- `pnpm release patch --win-msi --unsigned --publish`：准备（版本同步→前端→Windows 构建→finalize）
  完成后，调用与两步发布**同一个** `publishRelease({execute:true,…})`，不是第二条通路。
- 解析层硬规则：`--publish` 必须带 `--win-msi`/`--win-nsis`；不能与 `--dry-run`、`--skip-build`、
  `test` 通道同用；`--no-crates` 只在 `--publish` 时有意义。
- 两步形态保留（需要人工验收窗口时用）。**「代码整理提交」按崔总拍板保持现状**：脚本只提交版本文件与
  许可清单，其余改动一律阻断；不新增自动提交全部改动的开关。

### 3. 启动静默检查更新（`src/lib/update-source.ts` / `src/App.tsx` / `UpdateDialog.tsx`）

- 新增 `AUTO_UPDATE_KEY`（默认开，只有显式 `"off"` 才关）、`AUTO_UPDATE_DELAY_MS = 8000`、
  `readAutoCheck`、`autoCheckSource`（用户源偏好优先；`off`/无公钥返回 `null`）。
- App 根层 `onMount` 延迟 8 秒检查一次：发现新版本用 Toast 提示 + 「查看更新」动作打开更新对话框；
  下载与安装仍需用户确认（签名校验不变）；失败静默，不打扰启动。
- `defaultUpdateSource` 改为 **test/dev 通道默认 `off`**（避免调试包被拉到正式版），beta/release 照旧。
- UpdateDialog 加「启动时检查更新」开关（同一偏好键）。
- 隐私合规同批完成：`updates.privacy` 文案、`docs/privacy.md` / `privacy.en.md`、
  `docs/user-guide.md` / `user-guide.en.md` 全部从「仅主动检查时联网」改为「启动后自动检查 + 可关闭」。

### 4. AI 同级探测（`scripts/lib/ai-build.mjs` / 测试 fixture）

- `--ai=auto` 的回退链变为：显式登记 `ai-model-source.local.json` → **同级 `../model-registry` 自动探测**
  （不写登记文件，仍过全部摘要/契约校验）→ 构建无 AI 版并打印原因。
- 探测到的源与登记源走同一个 `readAiSource`，安全边界不变（registryId、artifact/profile 摘要、
  编码器契约、文件 SHA-256 全部照验）。日志会标明来源是「同级自动探测」。
- 顺带按 pi-lens 要求把两处 JSON 解析包了 try/catch（错误信息更明确）。

### 5. 更新公钥接入（`scripts/lib/release-config.mjs` / `release.mjs` / `vite.config.ts`）

- 公钥来源优先级：`RAYBEND_UPDATER_PUBLIC_KEY` 环境变量 → 仓内 `release.config.json`
  （`schema: 1` + `updaterPublicKey`；公开信息，可进仓库）→ 空（= 不带更新构建）。
  私钥/密码仍只走环境变量。新增单测覆盖坏配置必须显式报错。
- `vite.config.ts` 不再从 TS 导入 `scripts/lib/*.mjs`（`__RAYBEND_PHOTO_AI__` 与
  `__RAYBEND_UPDATER_PUBLIC_KEY__` 改为只读构建注入的 env）。原因有两个：
  ① 所有构建入口（`release` / `debug:win` / `pnpm tauri`）本来就会注入这两个值，现场探测是重复逻辑；
  ② TS 侧导入 `.mjs` 会触发「缺声明文件」的检查噪音（pi-lens 单文件模式不采纳 `tsconfig.node.json`），
  去掉后两边都干净。`tsconfig.node.json` 的 `allowJs` 实验也随之回滚。
- test/dev 通道默认不自动检查的改动见第 3 条 —— 这条同时避免「带公钥的调试包误查正式版」。

### 6. 文档

- `docs/release.md`：日常发行改为「一条命令或两条指令」；补指定版本号、`--publish` 硬规则、
  密钥与 `release.config.json`、启动静默检查；新增 §10「AI 外挂编译」与 §11「为什么暂不做 CI 打包」。
- `README.md` / `README.zh-CN.md`：新增「发布构建 / Release builds」小节（可直接复制的三条命令）+ 指向
  `docs/release.md`。
- `docs/development.md`：AI 段补「自动探测同级 `../model-registry`」。

## 验证

- `pnpm test`：1316 通过（新增 update-source 的自动检查用例）——`src/**/*.test.ts`
- `pnpm test:release`：99 通过（新增指定版本号、`--publish` 解析与一键流程用例）
- `pnpm typecheck`：通过；`pnpm lint:i18n` / `pnpm lint:arch` / `pnpm lint:colors`：全绿
- `pnpm build`：通过（vite 注入改为 env-only 后构建正常）
- `node --test scripts/lib/release-config.test.mjs`：2 通过

## 遗留 / 需要崔总

1. **更新密钥**：崔总执行 `pnpm tauri signer generate -w ~/.config/raybend/updater.key`，
   把打印的公钥给我 → 写进 `release.config.json`，再用 `--with-updater` 出带 `latest.json` 的版本。
2. **真机验证**（归人类）：`vN → vN+1` 的启动检查提示、下载、安装、失败恢复；`--publish` 一次到底的完整发布。
3. CI 打包按 §11 结论暂缓，触发条件已更新到 `memory/FUTURE.md` G24。
4. 本次改动未提交（按纪律由崔总决定提交时机）。
