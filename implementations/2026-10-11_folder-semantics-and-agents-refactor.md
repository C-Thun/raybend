# 目录范围改文件夹语义 + AGENTS.md 全局信息梳理

2026-10-11 完成（构建中）。

崔总这一轮提的后续：① 目录范围语义要跟文件夹一致；② AGENTS.md 做一次重新梳理（压缩 + 关键点上移）。

---

## 一、目录范围：子树语义 → **文件夹语义**

**现象**（崔总发现）：点一个目录会把**它子目录里的照片一起铺出来**。他的判断是对的，也确实容易乱套。

**改前**：`BrowseQueryDto::to_query` 用 `Scope::subtree(path)` —— 范围是「该目录的整棵子树」。

**改后**：用 `Scope::directory(path)` —— **只看该目录直属的照片**（`_RAW/` 视为透明的相机原片目录，与位图同层）。
与文件夹概念一致：点什么目录就看那个目录里的东西；要看子目录就展开它。

**唯一改动点**：`src-tauri/src/browse.rs::to_query`（`browse_page` / `browse_timeline` / `browse_facets` 三个命令共用它）。
`Scope::subtree` 保留（`query.rs` 层仍需要，测试也在用）。

**顺带发现的既有 bug（已修）**：`Scope::Directory` 与 `Subtree` 的 SQL 参数**没做大小写折叠** ——
`rel_path_folded` 是小写列，而参数直接用原路径 → **目录名带大写（`Photos/A`）时查不出照片**。
两处都改成 `normalize_rel_prefix(...).to_lowercase()`。

**新增测试**：`directory_scope_sees_only_its_own_photos` —— 一个目录里放直属位图 + `_RAW/` 原片 +
两个子目录 + 相邻同名前缀目录（`photos/Ab`），断言只返回直属的两个。这条测试就是上面那个折叠 bug 的发现者。

---

## 二、AGENTS.md 全局信息梳理

按崔总的领会执行：**AGENTS 写「设定 / 架构方向 / 纪律」的关键点，精简但一看就懂；模块原文档基本保留全部，作为留档；两边不用指针。**

### 压缩

- **§2 硬约束**（23 条）逐条压缩：**保留规则、判据与关键锚点**（文件/函数名），去掉冗长举例与重复铺垫。
  例：第 12 条（复用）从 8 行压到 5 行、第 21 条（编辑器快照）从 5 条子项压到一段、
  第 22 条（渐隐）从 4 行压到 3 行。**压缩的是措辞，不是信息** —— 判据、反查法、抓手全部保留。

### 上移（原本散在模块文档、但属于全局的东西）

- **§11.5 新增词条**：**资产（asset）与文件组**（位图 + `_RAW/` + `.xmp` 一起说话）、
  **`_RAW/` 保留目录语义**、**缩略图三档**（thumb 384 / film 192 / preview 1920）、
  **`latest`**（当前编辑栈输出，与 `issue` 并列）。
- 这些原在 `memory/FUNCTION-REPOSITORY.md` / `FUNCTION-IMAGING.md` 里 —— **原处保留**（留档），
  AGENTS 里给「一看就懂」的定调版本。

### 未做

- **模块文档本体未改**：崔总说「基本保留全部，作为留档」，这一轮只动 AGENTS.md。
  后续若要精简模块文档，单独一轮做（避免把留档改薄）。

---

## 三、验证（Agent 侧）

| 项 | 结果 |
| --- | --- |
| `cargo test --workspace` | **1492 + 134 + 7 + 2 + 2 全过** |
| `pnpm typecheck && pnpm test` | **1279 全过** |
| `pnpm lint:colors / lint:arch / lint:i18n` | 全过 |
| `pnpm debug:win` | 已启动（重新出产物） |

**未做**：真机 E2E（归崔总）。

---

## 四、并发情况说明（重要）

这一轮中途发现 **`src/api/types.ts` 被另一个会话改动**（新增 `rawWidth/rawHeight/latestSourceBase` 等
显示尺寸字段，与我的 `recycledFrom` 并存），并一度让 `store.test.ts` / `from-asset.test.ts` 的 fixture
类型不匹配（typecheck 红）。**我没有去动它**（避免并发冲突），只在报告里说明；稍后它自行收敛，
最终 typecheck 恢复全绿。最终产物包含两个会话的全部改动。
