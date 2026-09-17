# 待你做的事（ASSISTANCE.md）

> 这里**只放「现在就必须由你做、不做会卡住我」的事**，一条一句话。
> 做完告诉我，我验证后**直接从本文件删掉** —— 不写总结、不留归档（历史在 `implementations/` 与 git log）。
> **越短越好，空着就是最好的状态。** 未来才做的（验收、拍板、授权）不属于这里。

## 一、现在需要你做（阻塞中）

W1 的编码部分已经做到**只剩设计**了，剩下这两件都要你出手：

### A. 渲染 spike 的 Windows 逐项验证（W1 的唯一人类关卡）

**要做的事**：跑一行命令，然后照着一张表点几下、填两格。

```bash
pnpm spike:win
```

它会自动：重建前端产物 → 构建 Windows 产物 → `check:win` 核对 → 带 `RAYBEND_SPIKE=1` 启动应用
（**启动时自动开出 spike 窗口**）。

然后照 **`plans/M2-W1-windows-gpu.md`** 逐项走（那张表写得像操作手册：每项都是
「怎么做 → 看什么 → 填什么」，预计 15 分钟；含三个后端重测约 25 分钟），
最后在 spike 窗口右栏点「写报告」，把 `spike-report.md` 发回来。

**为什么只能你做**：真实 GPU 的透明合成、DPI 缩放、多显示器、最小化恢复、体感帧率
（`AGENTS.md` §2.8）—— 这些在 WSL 的软件渲染里量不出任何有意义的数。
Agent 侧已经把**能程序算的全算完了**（适配器/后端/alpha 模式/dpr/帧分位数/坐标往返偏差/
设备丢失恢复），表里只剩「只有眼睛能判」的事。

**做完我会**：把数字写进 `PLAN.md` 的 M0 表与实施记录；任何一项不过就按 `AGENTS.md` §2.11
判「能绕过」还是「绕不过去」，需要退路时评估 `FUTURE.md` A3 的 Tauri CEF 运行时。

### B. Pencil 连接（挡住 W1 最后的设计部分 0.1–0.6）

现象：Pencil 的 MCP 桥**能应答**，但四种调用（`get_app_state` / `execute` 带与不带 filePath）
一律返回 **「A file needs to be open in the editor to perform this action」** —— 它看不到编辑器里
打开的任何 `.pen`。

**请你**：重启 VSCode（Pencil 面板里确认 `design/browse.pen` 是打开/焦点状态），然后叫我一声。
我接着做 0.1–0.6（tiles 按落地规格重画 + 补三帧 + 库内外差异标注 + AppIcon 换真 logo + `browse.md` 收敛 + 截图给你过目）。

## 二、绕过去了的问题（攒着等外援）

> **这一节不阻塞任何工作**：每条都写清了「怎么绕的」与「风险是什么」，
> 所以可以攒着 —— 直到某一条**再也绕不过去**（继续走会踩空），它才升到上面「一、现在需要你做」。
> 修好之后同样**直接删掉**（历史在 `implementations/` 与 git log）。
>
> 规则见 `AGENTS.md` §2.12。

1. **中文在小控件里看着偏高（0.5–0.8px）** —— 已做度量修正（`vite.config.ts` 的
   `cjkMetricsOverride` 把 CJK 字体的 ascent/descent 改成方块字 em 盒，实测已生效），
   但**像素偏移一点没变**，判断是光栅化/基线吸格（本环境测不出来源）。
   → 你在真机上看一眼：**如果还是偏高就说一声**，那条改动一条 revert 就能撤
   （涉及三处：`vite.config.ts` 的插件、`src/index.tsx` 的字体 import、`scripts/ui-smoke.mjs` 的两条断言）。
   量法与数据见 `implementations/2026-09-17_exclude-chain_radius_language_font-metrics.md` §二。

2. **RAW 缩略图 134 ms/张（目标曾是 <10ms）** —— 瓶颈在 rawler 的
   `preview_image()` 实际走的是完整图像解码路径，拿不到「只抠 JPEG 段」的便宜。
   绕法：接受现状（比 JPEG 的 53ms/张慢，但比完整解码 764ms 快得多），
   缩略图队列本来就是后台跑、有缓存。
   风险：RAW 占比高的库里，首次导入后的缩略图阶段会明显慢于 JPEG。
   真要压到 10ms 级，得绕开 rawler 自己解析 TIFF/BMFF 里的预览偏移 —— 那是另一个工作量，
   记在 `FUTURE.md` 待办里（尚未登记）。

3. **契约/类型检查器偶发「陈旧快照」误报** —— 表现为报某个刚加的字段/键「不存在」，
   而 `npx tsc --noEmit` 全项目 0 错。已用决定性实验证伪一次（把键从源文件删掉才复现），
   并已用 `lens_diagnostic_mark` 标为误报。
   绕法：**先跑权威编译器**，不一致就以编译器为准；必要时提交一次让快照刷新。

4. **`cargo test` 偶发链接失败**（`rust-lld: undefined hidden symbol`）——
   今天第三次之后已按早先的建议**关掉测试档的增量编译**（`[profile.test] incremental = false`），
   根因是增量编译的陈旧目标文件。代价：改完源码后测试二进制整份重编。
   若你嫌慢，可以改回增量 + 遇到时 `cargo clean -p raybend`。

---

不属于这里的：设计取舍 → 各文档的「待决」小节；目视/真机验收 → 下面第三节；
工具约束 → `design/main.md` §6.1。

---

## 三、人类验证清单（**不阻塞开发**，到点我会提醒）

> 与 `PLAN.md` 的「人类验收清单」是一回事，但这里只放**当前这一波新产生的**。

| # | 事项 | 怎么做 | 卡住什么 |
| --- | --- | --- | --- |
| 1 | **M1 验收**（`PLAN.md` 第 1–4 项，含 3b–3f） | 跑 `C:\rb-target\raybend\debug\raybend-desktop.exe`（**需先重建**，见下）逐项过 | M1 签字 |
| 2 | **浏览工作区目视**（M2-W1 新交付） | 切到「浏览」工作流：左列能看到库与目录树、中列网格出真图（含 RAW！）、点照片右栏出信息；点目录切换范围；点「按时间」出分组标题 | W2 开工前确认地基可用 |
| 3 | **Windows GPU 验证**（M0-2 全部项 + M0-6 帧率） | **已可做**：`pnpm spike:win` → 照 `plans/M2-W1-windows-gpu.md` 逐项走 → 点「写报告」发回 | W2 的原生视口（**见「一、现在需要你做」A 项**） |
| 3b | **设计稿目视**（M2 阶段 0.1–0.3 的新画框） | 打开 `design/browse.pen` 看三个新画框：`Components / Tile 两态对照`（库内 vs 库外差什么）、`States / 浏览三态（命令面板 / 筛选 / Focus）`、以及两个网格画框里重画过的 tiles（正方外框 + 覆盖式信息条 + RAW 角标）。**尺寸以实际应用为准**，这里只看结构与状态是否说清；`main.pen` 的 `AppIcon` 是**占位符**（定案见 `design/main.md` §9） | 不卡开发；有问题我改画布 |
| 4 | **网格水印的观感 + 「等缓存」的等待体感**（2026-09-17 支线） | 真机跑一次：① 打开一个**外部目录** —— 那段「印在底纹上的大图标 + 文字」的**呼吸/扫光浓度与速度**是否「不抢眼也不显得卡」；② 双主题 × 两档密度各看一遍；③ 打开一个大目录（几百上千张），确认「等头部缓存再铺照片」的等待可接受（这是那个人类定下的取舍）；④ 空态：找一个空目录 / 空库，看那块水印；⑤ 切到英文（帮助菜单 → English）确认这几处文案真的跟着变 | 不卡任何开发；只影响要不要调那两个数字（`motion.css` 的 `rb-watermark-*`、`StateWatermark.tsx` 的不透明度） |

**重建 Windows 产物的命令**（照抄；细节见 `AGENTS.md` §5.3）。

> 现在有更省事的一条：**`pnpm spike:win`** 等价于「`pnpm build` → Windows 构建 → `pnpm check:win` → 开窗」
> 这几步的合体（外加 `--dry-run` 只构建不开窗）。下面这段是它的展开形态，出错时照它手跑：

```bash
pnpm build                                  # 必须先做：dist 是编译期嵌进 exe 的
export CARGO_TARGET_DIR='C:\rb-target\raybend'
export WSLENV='CARGO_TARGET_DIR'
cd /home/andares/repos/c-thun/raybend
cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build -p raybend-desktop -p raybend --features custom-protocol'
pnpm check:win                              # 验产物比 dist 新、资源名对得上
```

> ⚠️ 这次构建要**同时构建 `raybend` crate 的 bin**（`-p raybend`）——
> RAW 解码跑在独立的 `raybend-raw-worker` 进程里（`AGENTS.md` §6.3 的进程隔离）。
> 只建 `raybend-desktop` 的话，主程序会回退成「用标记参数重启自己」那条路径（也能工作，
> 但那是发布形态，开发期走独立 bin 更清楚）。
