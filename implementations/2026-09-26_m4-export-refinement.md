# M4 导出界面调整与 Tiles 刷新稳定性
完成时间：2026-09-26 21:56:23 CST

## 本轮范围

按崔总最新指示调整普通导出，并重点处理上部 Tiles 的回焦点闪烁/定稿消失。外部编辑器及 browse 定稿选择只记录定案，本轮未实现。并行 Agent 的 Shift 选择和 M5 改动保留，未改 lib/selection.ts。

## 已记录的后续行为

PLAN M4-W6、FUTURE、design/browse.md：外部编辑唯一入口在 browse toolsbar right；拍摄信息顶上放定稿下拉，切换仅改变显示、不写 latest；仅组件内存保留，切目录刷新组件或重启后默认 latest。外部编辑打开该显示定稿对应的 RGB16 TIFF。editor 无此入口。这些入口及弹窗/应用发现留待后续实施。

## 已实施

- 普通导出只提供 WebP、AVIF、JPG、PNG，按此顺序，默认新预设 WebP；PNG RGB16 无损且隐藏质量。TIFF 编码能力保留给外部编辑，普通预设验证拒绝 TIFF。
- 移除右上「新建」；直接改表单，按规范化后的名称判断新增/更新。改成新名字创建另一预设，原预设及其队列保留；已有名字更新对应预设。默认模板改为 `:CYEAR-:CMONTH-:CDAY/:FILENAME`。
- 导出尺寸改为三行单选：保持不变、百分比缩小、最大边长。后两项数字为 64px 短框，同行右侧固定 %/px，仅当前模式启用输入；百分比 1–100 整数，最大边 1–65535，均不放大。模式切换修复无效暂存值，不显示“填 0 原尺寸”。
- Rust 的 render_for_preset 为普通导出和尺寸预览共用入口，在完整显影/几何之后复用 display/output::resize_output。图片仍保留 RGB16 至编码，前端只保存参数；百分比与长边不串行叠加。
- 预设设备键 export.presets.v1→v2 显式一次性迁移，文件格式支持读取 v1、写出 v2；新增 sizeMode/percent。保留旧 ID/名称/目录/模板，旧 TIFF 改为同样无损 RGB16 的 PNG，旧 maxEdge=0 转“保持不变”。队列和开关继续不持久化。
- 共享 Form 新增 RadioChoices（Ark RadioGroup），用于真正互斥选择；既有 RadioCircle 是勾选框，不能当单选组复用。数字框继续用共用 Input，命令注册没有新增：格式/尺寸/名称均为表单内部交互，保存/导出继续使用已有命令与热键。
- Pencil MCP 同步四格式、三种尺寸及短输入框，更新主稿与相关状态稿；右栏截图检查通过。PLAN、IMAGING、export.md 的当前格式和尺寸规范同步。

## 闪烁与定稿消失的原因及修复

1. Windows 回焦点触发实时读盘，后端即使 changed_assets 为空也会通知 catalog://changed；原导出订阅对每条通知都清空全部定稿列表、全部缩略图。现在空变化仅刷新库信息，资产有变化才定向重读。
2. 原 invalidate 先删摘要，主图保留但小图突然为零、行高收缩；新摘要到来再展开。现在保留完整旧摘要，响应到齐后原子替换；每资产请求版本隔离晚响应，不再让一次刷新取消其他资产的在途批次。内容相同复用数组/定稿对象，避免无意义更新；小图筛选和排序结果按数据/选中入队状态复用。
3. 共享 PhotoGrid 的行模型每次计算生成新对象，VirtualGrid 的 For 因引用变化而重建整行；cellExtra/cellOverlay 也在渲染 effect 里重新创建子组件。现在复用未变化的行对象，扩展槽按资产 ID 保持实例，在 JSX 插入处 untrack，保持 Provider owner 链正确。
4. 首次载入先准备首批 128 个定稿摘要（筛选模式按批准备所需摘要），显示共用加载水印，随后挂载完整 Tile；不先展示错误的大主图布局。context 初始化在数据适配器之前，后续只订阅库/目录变化；返回工作流重读时不清空已显示内容。
5. 源文件变化仅刷新受影响定稿图片；缩略图队列原有机制会保留旧 URL 并先解码新图，但 Tile 的 loading 判定曾仍把旧图隐藏。现在有旧 src 就继续显示，到新图就绪再切换。共享可见单元还会在缓存被清空回到 idle 时请求图片，不对失败状态形成自动重试循环。
6. 浏览共享数据源刷新会清掉第一页以后的页，而虚拟范围不变时不一定再次请求，可能留下空格/缺定稿。现在按新时间线中的资产 ID 对齐保留画面、标记页待重读；可见 Tile 显式请求需要更新的页。未访问页不自动重读，成本仍是一趟首屏加当前可见页；删除/重排不把旧下标指向错照片。
7. 主色/辅色选择框移到 Tile 内最上层的绝对定位边框，信息黑条和图片不能覆盖它；所有状态继续复用同一 Tile。

## 验证

- 先添加 DOM 回归复现原问题：内容未变的 catalog 通知使定稿 Tile 节点身份变化；原代码断言失败（/tmp/m4-refine-before.log）。
- `pnpm test`：1043 通过；覆盖并发刷新/旧响应隔离、错误保留旧列表、一次性迁移、按名新增/更新、尺寸非法值、布局对象身份、旧页按资产 ID 重排及强制重读。日志 /tmp/m4-refine-tests-final.log。
- `pnpm typecheck`、颜色/分层/i18n 三 lint 均通过；日志 /tmp/m4-refine-typecheck-final.log、/tmp/m4-refine-{colors,arch,i18n}-final.log。
- `cargo test -p raybend --lib`：1156 通过，4 ignored；`cargo test -p raybend-desktop --lib`：93 通过，1 ignored；`cargo check --workspace` 通过。日志 /tmp/m4-refine-{core,shell,check}.log。尺寸测试覆盖保持精度、百分比 1/50/100、最大边、不放大及非法值；预设文件验证覆盖旧 TIFF 迁移和 v2 拒绝 TIFF。
- `pnpm check:export` 通过：连续回焦点无摘要重取/节点重建/URL 改变，慢速更新期间完整小图与几何保持，切工作流返回仍有定稿；四格式顺序、质量显示、尺寸控件启用/短宽、选择框层级、原选择/入队/删除/弹窗均通过。300 张合成记录实际跨过 256 张分页边界后回焦点，定稿与图片节点始终在场。日志 /tmp/m4-refine-export-paged.log。
- `pnpm check:browse` 最终通过，日志 /tmp/m4-refine-browse-verified.log；`pnpm smoke:ui http://localhost:1420/` 完整通过，problems=[]，日志 /tmp/m4-refine-ui-final.log。开发服务器中断及并行构建期间的中间冒烟未算通过，恢复服务、源码稳定后重跑。
- `pnpm debug:win` 最终通过，包含重新 build、主程序/worker 构建和 check:win；主程序 2026-09-26 21:51:26.307 CST，dist 21:50:28.244 CST，worker 21:47:54.705 CST、协议 raybend-worker-proto-v3。资源四项和时间/协议核对通过，日志 /tmp/m4-refine-win-final.log。产物 /mnt/c/rb-target/raybend/debug/raybend-desktop.exe。
- `git diff --check` 通过。Rust 仅对本轮涉及文件执行 rustfmt；全仓 fmt 检查有既有未格式化文件，没有批量改动无关代码。

## 验收边界

上述为类型、单元、编译及可重复合成后端冒烟，未声称真实 Windows GUI、DPI、真实照片库、颜色和性能已验收。崔总可用新调试版重点检查实际切 Windows 窗口与导出工作流的稳定性。未发布、push、打 tag 或创建发布安装包。
