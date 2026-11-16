# M2-W1「地基 + 浏览网格」实施记录（阶段 1–6 已完成）

完成时间：2026-09-17 03:44:41 CST

> 本次会话是「一夜连续施工」：从 `5304a21`（M1 交付）起，按 `plans/M2.md` 的 W1 推进。
> **W1 的 8 个阶段里 1–6 已完成并提交，7（渲染 spike + Windows 验证套件）与 8 的收尾部分未做**，
> 原因与后续动作见文末「遗留」。

---

## 一、这次做完了什么（按提交顺序）

| # | 提交 | 内容 |
| --- | --- | --- |
| 1 | `60c3adf` | **RAW 真解码入场**：rawler 后端 + worker 进程隔离 + 接进缩略图管线 |
| 2 | `3f9d174` | **catalog 查询引擎**（范围/筛选/排序/分页）+ 全文索引维护 + 10 万条基准 |
| 3 | `6618620` | **标记写入 + 撤销栈 + 旗标（内存）+ 回收站删除** |
| 4 | `c9f57e5` | **IPC 命令层**（11 条命令）+ 契约两侧同步 |
| 5 | `c9fb349` | **前端数据层**（api 封装 + browse store + 三态纯逻辑） |
| 6 | `dafd665`/`e36c78f` | **浏览界面**：网格 + 行模型 + 三列工作区接线 |

### 1.1 RAW 解码（`crates/raybend/src/raw/`）

`AGENTS.md` §6.3 的三条约束全部落地：可插拔 trait（对外零 rawler 类型）、worker 进程隔离、
内嵌预览优先。

**实测数据**（debug 构建，WSL，样本 = Panasonic RW2 5184×3888）：

| 路径 | 耗时 | 说明 |
| --- | --- | --- |
| 预检（大小 + 魔数） | 0.03 ms | 进 worker 前的廉价闸门 |
| 内嵌预览 → 768px | **~134 ms/张**（稳态） | 首张多 ~130ms：rawler 相机库初始化，已缓存到进程级 |
| 完整解码 | **764 ms/张** | 5184×3888，走 `RawDevelop`（去马赛克 → 白平衡 → 色彩校准 → sRGB） |
| 缩略图管线 | RAW 3 张全部渲染，占位 0 | 之前 RAW 一律占位图 |

计划里写的「内嵌预览 <10ms」**没有达成** —— 真实构成是 `rawler 相机库初始化`（一次性，~130ms）

+ `IFD 解析`（~2ms）+ `内嵌 JPEG 解码`（~30–55ms）+ `区域平均缩放`（~50–90ms）。
要做到 10ms 级别得绕过 rawler 直接抠 JPEG 段，属于优化项，记在下面的遗留里。

**过程中抓到的两个真 bug（都是演练/冒烟当场抓的，不是事后想到的）**：

1. **阻塞读期间持锁 → 看门狗失效**：`sleep 60` 真把父进程卡了 60 秒。
   修法：读之前把 `stdout` 取出去、读完再还；超时用**独立标志**区分「看门狗开的刀」。
2. **回退成「自己 + 标记」时会把非 worker 二进制当 worker 起**：示例程序打印的文字被当成
   协议头（报「响应头过长 4GB」）。修法：加**启动握手** + 禁止自我递归；
   错误从「协议错误」变成「XX 不会说我们的协议」。

### 1.2 查询引擎（`crates/raybend/src/store/query.rs`）

10 万条合成库实测（`cargo run --release -p raybend --example query-bench`）：

| 场景 | P95 | 备注 |
| --- | --- | --- |
| 整库计数 / 红标 / 近 30 天 | 0.03 / 0.49 / 0.26 ms | 用上了局部索引 |
| 3 星及以上 / 3星且红标 | 5.9 / 3.8 ms | |
| 中文检索（trigram） | 3.8 ms | |
| 分页第一页 / 深翻到第 5 万张 | **0.25 / 25.6 ms** | 修排序写法后 |
| 时间线（全量 id+时间） | 8.3 ms | |
| 分面统计（4 个维度） | 55.6 ms | 合并成一趟扫描后 |
| 目录子树计数 | 27.4 ms | EXISTS 逐行探测，仍是最慢的一条 |

**结论：不需要新增索引**（计划里留的 `catalog_0004_query_indexes.sql` 因此没写）。
慢的地方全在**写法**而不是缺索引，对照表写进了模块文档：

+ `ORDER BY a.taken_at IS NULL, ...` 让索引失效 → 分页 61ms → **0.23ms**
+ `LIKE '前缀/%'` 做子树范围（LIKE 默认大小写不敏感 ⇒ 用不上索引）→ 改范围比较
+ 四条 `GROUP BY` 分别扫全表算分面 → 合并成一趟 → 109ms → 53ms

### 1.3 顺手补的一个缺口：全文索引从来没人写过

`catalog_0001_init.sql` 建了 `assets_fts`，但 **M1 从头到尾没有一行代码往里写** ——
「搜什么都是空」。现在按「派生索引」维护（`store/fts.rs`）：
签名（条数 + 最大 id）不符就在查询前重建；单行编辑走 `refresh_asset`。

### 1.4 标记 / 撤销 / 旗标 / 删除（`store/{marking,flags,delete}.rs`）

+ 撤销记的是**带 before/after 的原子操作**（批量打星每张旧值可能都不同，只记 id 还原不了）
+ 锁真的挡写：二级锁挡标记与标签、一级锁只挡删除；**锁自己不受锁限制**（否则解不开）
+ 批量里夹着锁住的照片不整批失败：能改的改掉，`skipped_locked` 如实报回
+ 旗标键是 `(repository_id, asset_id)` —— 两个库的第 5 号是两张不同的照片
+ 删除**只到系统回收站**（`AGENTS.md` §11.3 的「不做真删」就此解除，但不提供永久删除）：
  文件先走、记录后走，避免「记录没了、文件还在」的残留

### 1.5 前端与界面

+ `src/api/browse.ts`（命令封装）+ `src/features/browse/`（store / 行模型 / 网格 / 左右两列）
+ 浏览工作区三列接线完成：**切到「浏览」即可看到库里的照片**
+ 分组口径与导入网格**交叉校验**（`rows.test.ts` 用同一份输入比两边的划分，口径漂了就红）
+ 26+2 条 i18n 键中英各一份

---

## 二、验证方式（跑过的命令与结果）

| 层 | 命令 | 结果 |
| --- | --- | --- |
| Rust 单测 | `cargo test -p raybend` | **679 通过 / 1 ignored**（ignored = 手动跑的回收站真实路径） |
| Rust 集成 | `cargo test -p raybend --test raw_worker` | 6 通过（含超时看门狗的回归用例） |
| Rust lint | `cargo clippy --workspace --all-targets` | 0 警告 |
| 真实 RAW | `cargo run -p raybend --example raw-smoke -- /mnt/c/src/tmp/pic --limit 40` | 见 §1.1 |
| 销毁演练 | `cargo run -p raybend --example raw-smoke -- --crash --timeout` | ✅ 全部符合预期 |
| 回收站 | `cargo test -p raybend -- --ignored store::delete` | ✅ 本机 `trash` 可用 |
| 查询基准 | `cargo run -q --release -p raybend --example query-bench` | 见 §1.2 |
| 前端 | `pnpm typecheck` / `test` / `lint:colors` / `lint:arch` / `build` | 全绿（476 测试通过） |
| 契约 | `cargo test -p raybend-desktop` | 28 通过（11 个新 DTO 的键名断言） |

**没跑的**：`pnpm smoke:ui`（浏览器端冒烟）与 **Windows 侧任何验证** —— 见遗留。
按 `AGENTS.md` §2.8：以上全是**冒烟级**证据，**E2E 与目视归人类**。

---

## 三、过程中的判断与取舍（供复盘）

1. **`[profile.test] incremental = false`**：`rust-lld: undefined hidden symbol` 今天第三次出现，
   按 `ASSISTANCE.md` §二 早先记下的建议关掉了测试档的增量编译。代价是测试二进制每次整份重编。
2. **不新增索引**：基准显示瓶颈全在写法（见 §1.2），按 `PLAN.md` §0 原则 4 不加。
3. **RW2 的魔数是 `IIU\0`（0x55）不是标准 TIFF 的 0x2A**：只认 0x2A 会把全部 RW2 挡在门外，
   而这是**冒烟当场**报出来的（预检写对了才知道）。
4. **时间分组留在前端**：计划里写「Rust 侧为准」，实际做法是 Rust 给**有序时间线**、
   前端按 `lib/time-group.ts` 的规则分组 —— 因为分组是纯展示派生量，且已有成熟实现与测试。
   口径一致性靠 `rows.test.ts` 的交叉校验守住。**这是对计划的偏离，特此记录**。
5. **契约检查器报「陈旧快照」误报**：`dto-contract.test.ts` / `en-US.ts` 的 i18n 键被反复报错，
   但 `npx tsc --noEmit` 全项目 0 错，且做了**决定性实验**（把键从 zh-CN.ts 删掉 → 精确复现那三条报错；
   恢复 → 0 错）。结论：检查器读的是**已提交快照**。每次提交后报错内容随之前移，可印证。

---

## 四、遗留（明确的下一步）

### 4.1 W1 未完成的部分

1. **阶段 7：渲染 spike + Windows 验证套件（整块未做）** —— 这是 W1 里最高风险、也是
   最需要人类参与的一项：
   + `crates/raybend/src/render/`（wgpu 上下文 / 视口变换 / WGSL）目前**仍是注释**；
   + `src-tauri/src/spike_viewport.rs`（透明挖洞调试窗口）不存在；
   + `plans/M2-W1-windows-gpu.md`（逐项表格 + 一键脚本）没写。
2. **阶段 8 的收尾**：`PLAN.md` 的 M2 段重写与 M0 表格状态、`ARCHITECTURE.md` /
   `FUTURE.md` / `THIRD-PARTY-NOTICES.md`（rawler、trash、auto-animate 三个新依赖）的同步。
3. **阶段 0：画布**（补三帧 + tiles 同步）—— 用户批注说「完成第一波次后再补」，本轮未做。

### 4.2 功能上的已知缺口（都不影响「看到照片」）

+ **看图 / 对比 / 胶片带**（W2）；筛选与排序 UI、标记动作接线（W2）；
+ 左右列宽度拖拽与 `toolsbar` 的浏览装配（W2）；
+ RAW 缩略图 134ms/张：若要压到 10ms 级，需要绕开 rawler 的完整解码路径直接抠 JPEG 段
  （`preview_image` 走的是 full-image 解码）；
+ `FUTURE.md` 该登记的新依赖（rawler / trash / `@formkit/auto-animate`）还没写进去。

### 4.3 需要人类做的事（已同步进 `ASSISTANCE.md`）

1. **M1 验收清单**（`PLAN.md` 第 1–4 项）—— M1 已交付，验收一直没做（不阻塞开发）；
2. **Windows 侧确认浏览工作区**：切到「浏览」看网格出图、左列目录树、右栏信息；
3. **Windows GPU 验证**（M0-2 + M0-6）—— 需要先有 §4.1 第 1 项的 spike 代码。
