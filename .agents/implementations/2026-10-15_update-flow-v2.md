2026-10-15 03:50:48

# 更新流程 v2：静默、可续传、永不阻塞（S1–S4）

崔总 2026-10-15 口述方案 → 报告与拍板（`specs/update-flow-v2.md`）→ 本次落地。
判据是崔总给的第一原则：**对用户的打扰降到最低、不影响正常使用**；用户只有两个决策点、
各只问一次（下载 / 跳过、安装 / 取消）。

## 改动范围

### Rust（`src-tauri`，S1）

- `updates.rs` → **`updates/` 目录模块**：
  - `updates/state.rs`：状态落盘（`<app_local_data>/updates/state.json`）+ 版本记账
    （`skipped_version` / `install_prompted_version` / `retry_attempted_version`）、
    `.part` / `.bin` 文件命名与白名单、清理策略；写盘走「临时文件 + 改名」。
  - `updates/download.rs`:Range 断点续传下载（`reqwest` stream + 60 s 静默超时）、
    续传判定纯函数 `resume_plan`（206 续写 / 200 与 416 重来 / 其它报错）、
    minisign 流式验签（`verify_stream`，老式签名回退整读）+ 签名内版本号比对。
  - `updates/mod.rs`：命令 `updates_status` / `updates_check(manual)` / `updates_begin(automatic)` /
    `updates_skip` / `updates_mark_install_prompted` / `updates_install`；后台下载任务；
    `updates://state` 事件广播（进度 200 ms 节流、落盘 2 s 节流）。
- `lib.rs`：命令注册替换 `updates_download`；`SOURCES` 自检表改指 `updates/mod.rs`。
- `Cargo.toml`：新增直接依赖 `reqwest`(?stream/rustls-no-provider/system-proxy)、`rustls`(ring)、
  `minisign-verify`、`base64`、`semver`、`futures-util`、`tokio`(time)——**全部已在锁文件里**
  （插件与 tauri core 本来就带），只是补成直接依赖；特性集与 `tauri-plugin-updater` 完全一致。

### 前端（S2 / S3）

- `src/shell/update-center.tsx`（新）：唯一状态源订阅 + 启动四分支
  （续传 → 静默重试一次 → 问安装 → 静默检查）+ 两个「只问一次」的对话框。
- `src/shell/UpdateDialog.tsx`（重写）：源只留**稳定 / 预览（灰着留口）**，删掉关闭档、
  自定义地址与公钥输入；状态区按 phase 说真话（进度条 / 就绪 / 失败原因）。
- `src/shell/TitleBar.tsx`：右上角持久指示器（下载中报百分比、就绪报版本），点击开对话框；平时不渲染。
- `src/api/updates.ts`：命令面重写 + `onUpdateState` 订阅。
- `src/lib/update-source.ts`（重写）+ 单测：偏好 v1 → v2 一次性迁移（`off` ⇒ 关掉自动检查、
  `custom` 归并到稳定）、`autoCheckAllowed` 通道准入（release→稳定、beta→任意、test/dev→不自动查）。
- `src/App.tsx`：启动检查从 App 搬进 `UpdateCenter`；TitleBar 接 `onOpenUpdates`。
- i18n 双语：新增决策/就绪/进度/指示器文案，删掉自定义源与旧状态文案。

### 文档与设计

- `design/main.md` §3.7：更新三态规格（对话框 / 两个问句 / titlebar 指示器）。
- `docs/privacy.md|.en.md`：联网描述改为实际行为（清单请求、下载后下次启动再确认一次、
  公钥校验后才落盘、失败不打扰且下次启动重试一次、界面不提供换源换钥）。
- `docs/user-guide.md|.en.md`「安装与更新」：按新流程重写。
- `docs/release.md` §4：客户端行为与发布源口径更新（预览通道 UI 未开、文件链路保留）。

## 关键决策与理由

1. **断点续传必须自研下载**：`tauri-plugin-updater` 2.12 的 `Update::download()` 一次性返回
   `Vec<u8>`（回调只给字节数、不给数据），签名校验是私有函数 —— 两条都堵死了「复用 + 落盘」。
   于是自己发 `Range` 请求 + 自己验签；**验签语义照抄插件**（base64 → `PublicKey::decode` /
   `Signature::decode` → 验签 → 签名内版本号比对），保持同一套信任模型。
2. **状态单一事实源在 Rust**：`state.json` 落盘 + `updates://state` 广播，前端不自己推导
   「该不该提示」。跨启动的三件事（续传、只问一次、只重试一次）都靠它成立。
3. **只问一次的记账**：跳过记版本号（只压自动提示）；安装提示**先记账再弹**（中途崩了也不重复问）；
   失败重试记版本号（每个版本只静默重试一次）。
4. **续传的安全性**：下载 URL 不落盘，每次续传前重新 `check()`（清单本身走签名校验），
   版本对不上就把旧进度作废 —— 不给「换掉的 URL 指向旧内容」留缝。
5. **界面不暴露密钥与自定义源**：面向普通用户只留稳定 / 预览；第三方构建按 AGPL-3.0 在源码层
   换源换钥（Rust 侧 `mode = "custom"` 仍接受，只是没有 UI 入口）—— 与项目原有的
   「不强制只接受官方签名」约束不冲突。
6. **删掉「关闭」档**：用户唯一的「别联网」出口是「启动时检查更新」开关（默认开）。
7. **UI 改动前的设计纪律**：本轮的界面改动先写规格（`design/main.md` §3.7），**`.pen` 画稿未补** ——
   Pencil 应用当时不在运行（MCP 报 `transport not connected to app: visual_studio_code`），
   画布上现存的 `Dialog / 更新设置与检查` 还是旧四档版本。**遗留项**：下次 Pencil 会话按 §3.7 重画。

## 验证方式（跑过的命令与结果）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | ✅ 全绿：主 crate 1573 passed / 0 failed / 11 ignored；另有 1 + 7 + 4 + 151 + 2 + 2 passed（含 `raybend-crashpad` 与两个 bin 目标） |
| `cargo test -p raybend-desktop --lib updates` | ✅ 23 passed（新增：续传判定 6 例、真实公钥/签名解码、**真实签名 + 别的内容必须拒**、签名内版本号比对、状态机（跳过后不再提示、失败只重试一次、重启后仍待安装、跳过后清掉半截文件）） |
| `pnpm typecheck` | ✅ 0 错误（另用临时探针确认语言包键合法，验证后已删） |
| `pnpm test` | ✅ 1316 passed |
| `pnpm lint:i18n` / `lint:arch` / `lint:colors` | ✅ 三条全过 |
| `pnpm build` | ✅ 构建成功（仅既有的 chunk > 500 kB 提示） |

未验证（归人类，见下）：真机升级链路、跨启动续传的断电场景、安装器接管后的重启。

## 遗留问题

**基础组件清单（AGENTS.md §12）无变化**：本轮界面全部用现有共享件搭（`Dialog` / `Button` / `Switch` /
`CompactChoice` / `Tooltip` / `IconButton`），进度条是对话框内一段带宽度百分比的 `div`（单处使用，
不构成可复用原语）。

1. **`.pen` 画稿待补**（见「关键决策 7」）：需要 Pencil 应用开着才能改 `design/main.pen`。
2. **真机 E2E 需要连续两个版本**：新版流程只在带 v2 代码的构建里存在，所以要
   先发 **0.1.3**（装上）→ 再发 **0.1.4**，才能在 0.1.3 上看到完整流程
   （启动问下载 → 下载中杀进程验证续传 → 下载完的下一次启动问安装）。
   测试包（`debug:win`）不带公钥、通道是 test，按设计**不参与**更新链路。
3. 未做（有意为之）：下载的「取消/暂停」按钮（崔总口述里没有这个决策点）、
   下载中限速、安装后自动重启（交给安装器）。
