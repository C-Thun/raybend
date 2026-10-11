完成时间：2026-10-10 01:01:30 CST

# XMP 写出开关 + 验收清单 + 只读巡检工具

## 1. 需求（崔总 2026-10-09 四问拍板）

1. 设置里加 XMP 写出开关 → **加，默认开**（opt-out）；
2. 开关存哪一级 → **设备级**（`app.db` settings KV）；
3. 打开开关时对历史照片 → **不补写**（从下一次编辑/标记起写）；
4. 验收材料 → **清单落 `specs/xmp-sidecar.md` §10** + Agent 写**只读巡检脚本**。

## 2. 改动范围与文件

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 规格 | `specs/xmp-write-toggle.md`（新） | 开关语义（唯一事实源）、UI、测试、验收新增项 |
| 规格 | `specs/xmp-audit-tool.md`（新） | 巡检口径/输出/退出码/入口/限制 |
| 规格 | `specs/xmp-sidecar.md` | 状态行加 2026-10-09 增补；§1.2、§9、§10（A–E 组清单）、§11.6、§12 同步 |
| 核心 | `crates/raybend/src/xmp/collect.rs`（新） | 从 DB 组装「落点 + 内容」的唯一一份实现（写出口与巡检共用）；含真实临时目录的写/删测试 |
| 核心 | `crates/raybend/src/xmp/audit.rs`（新） | `audit(conn, root) -> AuditReport`：路径/孤儿/`_RAW` 违例/解析/哈希/元数据/`rb:tagState`/`crs:` 规则/大小写折叠/文本漂移；7 个单测 |
| 核心 | `crates/raybend/src/xmp/packet.rs` | 抽 `pub crs_fragments`（compose 与巡检共用）；`latest_is_recorded` 公开（去掉私有重复规则） |
| 核心 | `crates/raybend/src/xmp/mod.rs` | 注册并导出 collect / audit / crs_fragments / latest_is_recorded |
| 外壳 | `src-tauri/src/sidecar.rs` | 开关（`xmp.write_sidecar`，默认开）+ `sync_now` 入口闸门；`sync_one` 改为调 `xmp::collect`（删掉重复组装）；开关值判定单测 |
| 前端 | `src/api/db.ts` | `SETTING_KEYS.xmpWrite` |
| 前端 | `src/lib/xmp-write.ts` + `.test.ts`（新） | 与 Rust 同口径的判定函数与值表测试 |
| 前端 | `src/shell/GlobalSettingsDialog.tsx` | 新「文件」页 + Switch（说明在 tooltip，§2.20） |
| 前端 | `src/i18n/{zh-CN,en-US}.ts` | `settings.files.*` 三条 |
| 入口 | `crates/raybend/examples/xmp-audit.rs`（新） | 只读 CLI：参数/版本闸门/打开/打印/退出码 |
| 入口 | `package.json`、`docs/development.md` | `pnpm check:xmp "<库根>"` 入口与文档行 |
| 记忆 | `memory/PLAN.md`（§4 #2、§6 决策行）、`memory/REVIEW.md`（R6-06 + §11.6）、`memory/FUTURE.md`（条目 3） | 状态与决策落账 |

## 3. 关键决策与理由

1. **默认开**（记档理由见 `memory/REVIEW.md` §11.6）：sidecar 在库内承担灾难恢复半边；写在库内而非用户源目录；行业两种先例并存。关闭只是出口。
2. **设备级 `app.db` KV，无数据库迁移**：现成 `setting_get/set`；语义是「这台机器愿不愿意在照片旁落文件」。按库存储留将来（需 catalog 迁移）。
3. **闸门单点**：`sync_now` 入口一处短路 → 关闭时**不写也不删**（含「内容全空 → 删 sidecar」这条）；照片进回收站仍带自家 sidecar（清理，不算生成）；导入采纳（读回）不受影响。
4. **判定跨语言两处、同口径**：Rust `write_enabled_value` 与 TS `xmpWriteEnabled` 用同一张值表（缺失/`true`/大小写/空白/垃圾值 → 开；仅明确 `false` → 关），两侧注释互指，测试同表。跨语言无法共享代码，这是最小重复。
5. **巡检复用运行时同一份实现**（§2.12）：抽 `collect`（原先在 `src-tauri` 里组装）与 `crs_fragments`（原先 compose 内联），巡检不另立规则；`profile_hash` / `resolve_sidecar` / `parse` 直接用现成的。
6. **巡检只读 + 版本闸门**：先只读读 `PRAGMA user_version`，与当前 schema 不同就拒绝（exit 2），避免「巡检顺手把旧库迁了」；版本一致时才 `CatalogDb::open`（迁移为空操作）。
7. **工具形态选 example 而非 bin**：与 `crash-drill` / `migration-drill` 同族；不进入 `cargo install` 的产品面。入口 `pnpm check:xmp`（WSL 跑，Windows 盘库用 `/mnt/c/...`）。
8. **前端不新增命令**：开关是设置项，经既有 `settings.open`（`Mod+,`）可达；无默认热键问题（§2.15 记录）。

## 4. 验证（Agent 冒烟）

* `cargo test -p raybend --lib --offline`：**1472 通过、12 忽略、0 失败**（含 `xmp::` 72 个：collect 3 + audit 7 等）。
* `cargo test -p raybend-desktop --lib --offline`：**120 通过、1 忽略、0 失败**（含开关值判定）。
* `cargo check --workspace --offline`：通过。
* `./node_modules/.bin/tsc --noEmit`：通过。
* `node --test "src/**/*.test.ts"`：**1242 通过、0 失败**（含新增 `src/lib/xmp-write.test.ts`）。
* `node scripts/check-hardcoded-colors.mjs`、`check-architecture.mjs`、`check-i18n.mjs`：全部通过。
* `rustfmt --edition 2024 --check`（本次涉及的核心/示例/外壳文件）：通过。
* **CLI 实跑**（合成库 `/tmp/rb-xmp-smoke2/lib`，由 `migration-drill seed` 造 6 张、当前 schema）：
  * 干净库 → `FAIL 0 / exit 0`；
  * 植入孤儿自家 `.xmp` → `FAIL 孤儿`、exit 1；
  * 植入 `_RAW/` 下的自家 `.xmp` → `FAIL 规则禁止`、exit 1；
  * 植入外来 `.xmp` → `INFO 外来文件`；
  * 错误路径（无参数 / 目录不存在 / 无 `catalog.db`）→ exit 2 且各自给出原因；
  * `pnpm check:xmp <库>` 入口实跑通过（FAIL 时退出码 1 正确透传）。

## 5. 顺带说明（非本单元语义）

* `pi-lens` 的 Rust autofix 在编辑过程中**顺带格式化**了同 crate 的 `xmp/adopt.rs`、`xmp/mapping.rs`（纯 rustfmt：import 排序、换行重排；逐行核对无改语义）。记录在案，避免被误认成功能改动。
* `memory/PLAN.md` 决策表里原有一处 `---` 紧跟表格行（历史遗留的 MD003），本次加分节空行顺手修掉。

## 6. 遗留（待人类）

* **Windows 真机验收**：`specs/xmp-sidecar.md` §10 的 A–E 组（含 E 组开关三问）；并跑一次 `pnpm check:xmp "<真实库根>"`。
* **预期信号说明**：关闭开关期间改过的资产，巡检会如实报 FAIL（盘上与库不一致）——这是设计（不补写、不动盘上文件），不是 bug；验收按 E 组说明理解。
* 巡检工具在**旧 schema 的库**上会拒绝（exit 2）：先用应用打开一次完成迁移再巡检。
* 未做（按决定）：不补写历史、不做批量动作、开关状态不进 catalog（按库策略留将来）。

## 7. 纪律自查

* §2.15 命令体系：不新增命令/热键（理由见 §3.8）。
* §2.16 迁移框架：不涉及（app.db KV / 无 schema 变化）。
* §2.19 XMP 联动：本单元即 XMP 本体；巡检工具覆盖新数据的往返核对。
* §2.20 界面文案：开关说明只在 tooltip，不占版面。
* §2.12 复用：collect / crs_fragments / 判定函数均单一来源，无第二套实现。
* §2.8 测试分工：巡检工具就是「可重复执行的验收代码」；GUI/视觉/回收站/真实库仍归人类（本记录只声明 Agent 冒烟结论）。
