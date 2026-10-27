# 修复退出编辑后取图请求的异常重复渲染与队列堆积

完成时间：2026-10-27 15:27:07

## 背景与结论

崔总真机现象：编辑后退出 editor，browse 双击看大图空白并一直显示「正在载入原图…」，compare 与 import 也受影响，GPU 长时间高占用，严重时只能关程序。

这次按两条线处理并都已落地：

1. **源头**：修掉前端会连续发出相同刷新/取图请求的地方；
2. **消费端**：给真实渲染建立带内容身份的同 key 单飞，重复请求等待并共享结果，且留下可查日志。

**已修复的是「相同内容的异常重复渲染与队列堆积」**，不是把 RAW 改成多管线，也不是把正常冷渲染变成零等待。真机还量到一个独立事实：1920 AVIF 编码本身可以很贵，见「遗留」。

## 根因

缓存只能避免「已经完成以后」的重复工作，挡不住「第一个还没渲染完，后面又连续进来」的重复工作。

`view_image` / `thumb_get` 没有执行中请求的身份合并；同时存在三处前端放大器：

- `ThumbQueue.refresh()`：旧请求在飞时也立刻再发同路径请求，连续 confirm 可叠出一串；
- `createPreviewQueue`：只保留等待中的最后一个，不识别在飞任务与内容身份，退出编辑还会再补一次 preview 刷新；
- `viewer.store::loadFullFor`：预载在占位返回后才出现时，仍可能再发一个 full。

不同入口（网格、viewer 两段加载、退出编辑 preview、issue/导出）又会撞同一幅输出，于是它们一起挤单例 RAW 解码与后台队列，界面观感就是越来越卡。

## 改动

### 1. 消费端：同 key 单飞 + 重复日志

新增 `crates/raybend/src/thumbnail/flight.rs`：

- `FlightGate::begin(key)`：首个请求为 leader，后续同 key 请求等待；
- 等待者拿到同一个结果，不是简单丢弃（否则它的界面仍然空图）；
- leader 成功、失败、提前返回或 panic 都会结算并唤醒等待者；
- 结算后释放 key，**它不是另一层缓存**；后续请求仍先查真正的缓存；
- 每次合并记 DEBUG，等待者达到 3 个（总共 4 次压入）记 WARN，提示反查上游。

接入位置：

| 路径 | 身份 key | 文件 |
| --- | --- | --- |
| 384 小图 | 文件身份（含源签名）+ 档位 + 编辑/编码/管线签名 | `crates/raybend/src/thumbnail/worker.rs` |
| 1920 占位 / full 真身 | 库 + 资产 + 变体指纹 + 源文件签名 + base + 档位 + 管线版本 | `src-tauri/src/thumbs.rs` |
| import 库外图 | 路径 + 文件大小/mtime + 档位 + 编码签名（复用 `SourceKey`） | `crates/raybend/src/display/mod.rs` |
| 既有 `WorkQueue` 后台任务 | 原任务 key；补重复 DEBUG/WARN，不改调度 | `src-tauri/src/work.rs` |

这样 `develop_preview_refresh` 与 viewer 的 screen 请求会汇到同一次 1920 渲染；preview 刷新与网格 thumb 请求会汇到同一次 384 渲染。

**没有只按路径合并**：编辑栈变化后是另一个 key，不会把旧栈结果误发给新请求。库外也没有新增磁盘缓存、预热或后台扫描。

### 2. 源头：前端刷新合并

- `src/components/ui/thumb-queue.ts`
  - 新增真实在飞集合与尾随 refresh 集合；
  - 在飞期间的 N 次 `refresh(path)` 作废旧结果，但只在结束后补一次最新请求；
  - `clear()` / `cancel()` 同时取消尾随刷新，旧目录不会复活。
- `src/lib/preview-queue.ts`
  - `push(path, after?, identity?)` 增加内容身份；
  - 同路径 + 同 identity 合并到在飞/待跑任务；不同 identity 只保留最新尾随；
  - 不传 identity 保持旧行为，绝不按路径猜同内容；
  - `onDuplicate` 暴露合并事件。
- `src/workspaces/editor/EditorWorkspace.tsx`
  - 用 `developRev` 作为 preview 内容身份；
  - confirm、撤销后刷新、退出/换照片补刷都带 rev；
  - 重复合并写 `console.debug`。
- `src/components/ui/viewer/store.ts`
  - `loadFullFor` 也等待已经存在的同 key 预载，不再在占位返回后漏发第二个 full；
  - 预载/真身重复合并有控制台诊断。

### 3. 日志可观测性

- `[image] 相同渲染请求已在执行…`：哪一份渲染被合并；
- `[image] 相同渲染请求连续压入 N 次…`：达到 WARN 阈值，要求反查源头；
- `[work] 相同后台任务…`：更早一层的 `develop_preview_refresh` 合并也不再无声；
- key 的路径用显示形式交给既有脱敏层，不打印可能含路径的原始缓存字节键。

上一轮加的分段日志（`[view]` / `[render]` / `[raw] 等解码许可`）继续用于区分排队、解码、显影、编码。

## 验证

### 单元测试 / 质量门

- `cargo test --workspace`：**全绿**（主库 1591、桌面库 155，其余目标也全绿；11 + 2 ignored 为既有）。
- `pnpm typecheck`：通过。
- `pnpm test`：**1334 / 1334**。
- `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`：全绿。

新增/补强测试：

- 单飞：同 key 一个 leader、等待者共享结果、不同 key 不互堵、失败唤醒、Drop/panic 不吊死；
- 真实 384 并发：6 个相同请求只进入一次渲染，6 个调用者拿到相同字节；
- ThumbQueue：连续 refresh 只补一次、旧结果不发布、clear 不复活；
- PreviewQueue：同 rev 合并、不同 rev 只留最新、无 identity 保持旧行为；
- 全量测试曾抓到单飞测试自身的放行竞态，已改成「确认 waiter 已认领再放行」并重跑全绿，未把偶发失败算作通过。

### Windows 真机 CDP 自动复测

`pnpm debug:win` 成功，`check:win` 确认 exe、worker、前端资源一致。

1. **编辑 → 立刻退出 → 双击**：
   - 两个相同 1920 请求只出现一次 `[render] Screen`，另一个等待共享；
   - 三个相同 384 请求只出现一次 `[render] Grid`，其余两个合并；
   - full 真身渲染约 2.2s，最终正常显示原图，无悬挂 IPC。
2. **compare**：两格都 `data-compare-url=ok`，两张分别为 5184 / 3712 原图并完整解码。
3. **import**：JPG 与 RAW 都能看原图，载入提示正常消失。
4. **库外直接并发**：同一未缓存 ORF 同时发 4 个 full 请求，约 2.3s 四个都返回相同的 2,463,001 字节；日志明确记录 3 个等待者与「4 次压入已合并」WARN。
5. **空闲复核**：所有 IPC 已结算；主进程 5 秒 CPU 增量约 0.031s（约 0.006 核），未发现该进程 GPU 引擎持续占用（>0.5% 的采样项为空）。

以上是**真机自动化验证**，崔总的目视操作确认仍可再做一次。

## 评审修订（2026-10-27 第三方 review 后）

1. **`thumb-queue.ts` 的在飞记录从布尔集合改成计数**（真实缺陷）：`clear()` 后同一路径
   会立刻重取，而旧 generation 的请求仍在飞 —— 旧的 `finally` 会把新请求的「在飞」
   抹掉，于是下一次 `refresh()` 又并发一份同路径渲染（正是本轮要消掉的问题）。
   现在按「路径 → 在飞条数」计数，且只有**最后一个**在飞请求结束才放出尾随刷新。
   补回归测试 `clear 之后同路径重取…`；已用旧实现验证过该测试会红（不是摆设）。
2. **`viewer/store.ts`：作废代的等待分支不再清 `fullPending`**（本轮新加的那分支拷了
   旧代码的写法）：旧结果迟到时它描述的是新那张照片，不能替它把「正在载入原图」
   的角标落下。（当时同函数里另两处同类旧代码属范围外；已于第 2 轮一并收敛，见下。）
3. **`diagnostics/logger.rs`：噪音闸门去掉每行 `format!` 分配**（每条日志热路径上调 5 次），
   改成 `strip_prefix` + `::` 判断，语义与单测不变。

修订后重跑：`cargo test --workspace` 全绿（主库 1591）、`pnpm typecheck`、
`pnpm test` 1334 / 1334、三条 lint 全绿；`pnpm debug:win` + `check:win` 重建通过，
按上一节同一路径复测：`[render]` 中 Grid / Screen / Full **各只 1 次**，
Grid 那一条合并了 2 个等待者、Screen 合并 1 个；看图约 6s 后出原图（3888px）。

## 评审第 2 轮补充（崔总 2026-10-27 点名三项）

1. **等待超时**（`thumbnail/flight.rs`）：`FlightWait::wait_timeout(limit)`，
   `wait()` 默认上限 `DEFAULT_WAIT = 120s`（长于 RAW 解码 45s 超时与最重的整帧管线）。
   超时**只放弃等待**、记 WARN（`[image] 等待同一份渲染超过 …ms，放弃等待（不重复渲染）`），
   不重新渲染（否则又把重复渲染引回来）；limit 夹到 24h 并走 `checked_add`，无溢出 panic 面。
   新单测：限额附近返回、leader 仍可结算、结算后 key 释放。
2. **共享结果不再重复复制**（`flight.rs` 的 `publish` / `into_owned`）：
   leader 成功时调用 `publish(value)` 把值**移进** `Arc`（不再 `value.clone()`），
   失败时 `publish_error(&e)` 只共享错误文本、leader 保留原错误类型；
   取回自有值时用 `into_owned(arc)`（`Arc::try_unwrap`，谁最后一个放手就零拷贝拿走）。
   384 / 1920 / full / 库外四条路径全部改用这一套。
3. **作废代不碰全局提示状态**（`viewer/store.ts`）：`loadFullFor` 迟到的旧结果
   不再 `setFullPending(false)`（那描述的是新那一代）。新回归测试：
   a 真身在飞 → 切到 b → a 迟到，b 的「正在载入原图…」必须还在；
   **已用旧实现验证该测试会红**。

第 2 轮修订后：`cargo test --workspace` 全绿（主库 1592）、`pnpm test` 1335 / 1335、
`pnpm typecheck` 与三条 lint 全绿；`pnpm debug:win` + `check:win` 重建通过，
真机复测同一路径：`[render]` Grid / Screen / Full **各 1 次**、合并日志正常，
**无一条**「等待同一份渲染超过」告警，看图正常出原图。

## 遗留（与异常重复队列分开）

- 一次正常的 1920 冷渲染实测约 6.75s，其中**编码约 4.65s**；解码约 2.0s、显影约 71ms。它是正常路径的编码/调度性能问题，不是这次「同一请求被重复执行」。需要另案评估 AVIF 编码与后台并发，不用降低看图档或引入新档位来掩盖。
- `editor_unbind_renderer` 在本次两轮分别约 3.3s / 1.1s，仍受在跑显影帧影响；本次只加耗时证据，没有改编辑器停止语义。
- 单飞只合并**执行中的同一身份**；不同照片、不同编辑 rev、不同档位的正常重活仍会按既有优先级执行。
