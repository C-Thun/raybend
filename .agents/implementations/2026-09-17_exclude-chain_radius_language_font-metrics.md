# 排除链路做通 + 语言切换 + tile 倒角加大 + 字体度量修正（本轮四条）

完成时间：2026-09-17 00:20:09 CST

本轮来自人类的一批反馈，四条并行推进（一张实施记录，因为它们是同一次交互里提出、且互相有牵连）。

## 一、easy copy：已复制**原地替换**点击复制

**病根**（两条叠在一起）：`EasyCopy` 的气泡内容写成了 `flex-col` 两行，同时渲染
`点击复制` 与 `已复制`（所以看到的是「一张写着两句话的卡片」）；而气泡因此长高一倍，
在贴着窗口顶部的 `flowbar` 上方**放不下**，Ark 的定位器把它 flip 到**下方** ——
这就是「在底部弹」的来源。

**改法**（`src/components/ui/EasyCopy.tsx`）：**一个气泡、一句文案**。
点击后重放一次「向上弹出」并把文案翻转成 `已复制`。一个改动同时解决两件事，
也把「不会因变高而翻面」变成结构性保证。

文档同步：`DESIGN.md` §12.1 与 `design/main.md` §4.1 原来写的也是「再弹一个」（与人类要求不符），
现已改成「翻转 + 为什么不是并排两句」。

## 二、字体上飘：查到根因、修了度量，但**像素偏移没变**（按人类指示跳过）

人类给了三个排查方向，逐条查完（**全部用像素实测**，方法见下）：

| 方向 | 结论 |
| --- | --- |
| CSS 排版（line-height / flex 居中 / 不对称 padding） | **不是**。`line-height` 从 1.45 换成 1 / 1.2 / 写死 px，墨水位置几乎不动 —— 半行距是上下对称分的，**墨水在行盒里的位置与 `line-height` 无关** |
| 外部全局定义（body 的 line-height / font-face metrics） | **是它**。Noto Sans SC 声明 ascent/descent ≈ **1.16em / 0.29em**，而方块字墨水只占 **0.85em / 0.09em** —— 行盒把墨水天然分得偏上；再叠上字体栈里 Inter 排前面（拉丁走 Inter、汉字走 Noto，同一条行盒两套 metrics 互相拉扯），偏移随字号/组合漂 |
| 「多段字体」冒充加粗 | **没有**。加粗就是 `font-weight`；可变字体真有 100–900 轴（实测 Inter 400→600 字符宽度会变、CJK 笔画会变粗，不是合成粗体） |

**量法**（本轮新建，值得复用）：`Page.captureScreenshot` 截图后，
按元素矩形算**墨水质心**相对盒子中心的偏移（dpr=2，亚像素，精度约 ±0.1px），
并用「上空/下空」两个数交叉验证。第一次用「Range 行盒 + 字体 metrics 推算」的模型法
算出来有 0.5px 级系统误差，**实测证明模型法不可靠** —— 教训：位置问题一律上像素。

**修法**：`vite.config.ts` 新增 `cjkMetricsOverride` 编译期插件，给 CJK 字体的
每个 `@font-face` 补 `ascent-override: 88% / descent-override: 12%`（= 方块字 em 盒，
Noto 自己的 OS/2 typo 度量）。**已确认生效**（实测：行盒 `line-height: normal` 下 145 → 100、
canvas 度量 116/29 → 88/12）。

**但**：真机界面上的像素偏移**一点没变**（−0.79 / −0.62 / −0.68 前后完全一致）。
也就是说这 0.5–0.8px 在本环境里是**光栅化/基线吸格**造成的，不是布局算错 —— 度量修正治不了它。
人类拍板：**跳过，以后解决**（见 `ASSISTANCE.md` §二）。

**踩坑两枚**（都记在代码注释里）：① `transform(id)` 上的 id 带 `?direct` 查询串，
`id.endsWith(".css")` 会静默漏掉；② CSS 的 `@import` 由 postcss **直接读盘**内联、
**不走插件管线** —— 所以 CJK 字体包改成从 `src/index.tsx` 用 JS `import`，
插件才见得着它。

守卫：`pnpm smoke:ui` 新增两条断言 —— ① CJK 度量必须仍是 88/12；
② `line-height: normal` 下 CJK 行盒必须 ≈100px。覆盖一旦失效立刻红。

## 三、导入排除：从「一点效果都没有」做到链路通

调研发现：**排除原本对导入零效果** —— `import_start` 只传 sources，后端根本不知道排除；
排除集合还存在照片网格的 store 里，**换目录就被 `resetDirState` 清空**。
按人类要求四件事一起做：

| # | 事 | 落点 |
| --- | --- | --- |
| 1 | **视觉**：照片变透明 + **照片正中**一个禁行图标 | `Tile` 新增 `excluded`：照片 `opacity-35`（不是整块调淡 —— 外框的选中/指向底色要留着），图标用中性的 `fg-2`（人类：「图标本身冲击已经很大，颜色要克制」） |
| 2 | **跨目录/跨源的一份内存列表** | 从照片网格 store **搬到**导入工作区 store（`workspaces/import/store.ts`）：键是**绝对路径**（= 后端给的 `SourceItem.path`，前后端一套口径）。切目录、重扫都不再清空 |
| 3 | **计数**：多源集合 − 排除，右边显示「已排除 N 张」 | 新增纯函数 `lib/excluded.ts`（`isUnderDir` / `countExcludedInDirs` / `importPhotoCount` / `invertExcluded`）；`RepositoryFooter` 同一行右侧显示排除数（为 0 时那句不出现） |
| 4 | **真的不导入** | 契约加 `excluded: string[]`：`api/import.ts` → `imports/store.ts` → `import_start` → `RunRequest.excluded`（`Arc<HashSet>`，整批共享）；runner 在**扫描之后、读元数据之前**剔除，**不计入 total/skipped**（「不导入这张」≠「试过了但跳过」） |

「落在已勾选目录里」的判据**必须与后端一致**：`includeSubdirs = true` 任意层级、
`false` 只算直属文件（对应 `ScanOptions.max_depth`）。判据走 `pathKey`
（NFC + 分隔符统一 + 折叠大小写 + 去尾斜杠），所以 `D:\Photos` 与 `d:/photos/` 认得出同一处。

边界都写了单测：**前缀相近但不是子目录**（`/src/2026-08` vs `/src/2026`）、Windows 反斜杠与大写、
中文与超长路径、空路径、排除比总数还多（不出现负数）、空集合、`invert` 的可逆性与不可变性。

## 四、tile 倒角加大

人类：「倒角基本要看不出了」。`--tile-radius` **3px → 6px**（外框与照片共用；
照片比外框内缩 `--tile-pad`，同一个值下内角看起来略圆一点，正好不显得「套了两层框」）。
放大截图核对过：两处圆角都清晰可辨。

## 五、帮助菜单里的语言切换（在「关于」上面）

- 新增 i18n 键 `locale.name`（**自语名 / endonym**）：`zh-CN = 中文`、`en-US = English`。
- 菜单项文字**必须从目标语言的包取**（`localeLabel(nextLocale(locale()))`）：
  中文界面显示 `English`、英文界面显示 `中文` —— 也就是「点下去会变成什么」。
  用 `t()` 会让两种界面都写自己的名字，那条菜单就失去意义（**代码里已注明这是特例**）。
- 顺带补上**持久化**：`raybend.locale` 存 `localStorage`（与主题/密度同一类设备级偏好），
  `src/index.tsx` 在**首次渲染前** `hydrateLocale()` —— 否则会先闪一帧中文再跳成英文。

## 六、验证

| 项 | 结果 |
| --- | --- |
| `pnpm test` | **444 通过**（新增 `lib/excluded` 7 条、工作区 store 排除 2 条） |
| `pnpm typecheck` / `lint:colors` / `lint:arch` / `build` | 全绿（生产 CSS 里能查到 `ascent-override`） |
| `cargo test -p raybend` | **567 通过**（新增「被排除的文件在规划前就被丢掉且不计入计数」1 条） |
| `cargo clippy --workspace --all-targets` | **0 警告**（顺手清掉 `meta_cache.rs` 测试里的 4 处历史告警：无用帮手、needless borrow、`map`→`inspect`、无效果的 struct update） |
| `pnpm smoke:ui` | **`problems: []`**；新增实测：排除态照片 `opacity 0.35`、禁行图标在照片正中（±2px）、图标色 = `--fg-2`（中性）；CJK 度量 88/12、行盒 100px |
| 倒角 | 截图放大核对：外框与照片左上角圆角均清晰可见（实测 `border-radius: 6px`） |

## 七、遗留 / 归人类

- **字体上飘**：度量已按方块字 em 盒修正（真机上是否改善**未见分晓**）——
  量级 0.5–0.8px，本环境判为光栅化/吸格。目视一眼即可判断，见 `ASSISTANCE.md` §二。
- **排除的真机手感**：勾目录 → 排除几张 → 切目录回来 → 按导入，看计数与实际导入张数是否对得上。
- **语言切换**：切到英文后重启，应当仍是英文（持久化）；`关于` 上方那条显示的是目标语言名。
- **本地工具链的坑**：`cargo test` 偶发 `rust-lld: undefined hidden symbol ... .llvm.*`
  （增量编译的陈旧目标文件），`cargo clean -p raybend` 即可恢复 —— 见 `ASSISTANCE.md` §二。
