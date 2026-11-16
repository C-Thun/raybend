# M4 导出预设与队列界面精简
完成时间：2026-09-26 23:57:34 CST

## 本轮口径与结果

崔总确认极简预设流程：上方只有可点选的列表，点选整份载入下面；空列表显示“当前无预设，请先新建”。下方一直是参数表单，名称 trim/NFC 后唯一：不存在为“新建预设 / 新增”，存在为“修改预设 / 变更”。选旧预设后改新名字创建另一预设，改成已有名字则覆盖那个同名预设；不需要额外新建模式。

名称输入停止 1.5 秒后再判断新增/变更，连续输入取消旧计时。列表点选、加载已选预设、保存成功立即匹配；销毁取消计时。等待期间保存按钮和保存命令禁用，防止旧标题下保存至另一目标。最终保存仍以提交时名称匹配，同一匹配函数供检测与保存使用。

- 移除列表区导入/导出按钮；预设文件交换业务标记“暂不启用”，保留底层序列化与文件能力，但无界面/菜单/命令面板入口。
- 移除目标名/尺寸“导出预览”、开始按钮下说明、独立失败/失效清单及弹窗。删除对应运行时动作注册和命令，清理废弃文案。
- 底部保存按钮收紧为“新增 / 变更”；同一行 8px 间隔，开始/停止按钮撑满剩余宽度。保存仍走同一表单提交路径。
- 百分比/最大边长数字框由 64px 加宽 50% 至 96px，保留原生数字步进器和 %/px 单位。
- 失败条目在队列中直接显示状态、原因与单项重试，完整原因可悬停查看；方形内容区保持，网格下方留错误信息区，列表模式在旁显示。使用 PhotoGrid 既有 cellExtra 与行高适配，未复制网格组件。
- 定稿哈希失效时执行器跳过并直接移除条目，释放执行槽，继续后续项；不产生 skipped 状态、历史或去重影子。前端队列计数和选择随权威状态更新。失败仍保留且可选/移除/重试，完成记录仍保留。

## 涉及文件与命令

工作区 ExportWorkspace/actions/store/source、共享导出 model、App 与 commands/catalog、两语言包；Rust export/jobs 与外壳 export 转发；API/核心 preset 文件能力标注暂不启用。PLAN、design/export.md 与旧 W4/W5 规格补最新定案。未改并行的 lib/selection.ts。

保存继续使用 export.save，明确无默认键（避免误存），enabled 与按钮共用 canSave；开启/停止仍 Mod+Enter，队列移除仍 Delete、取消仍 Esc。单条失败的重试绑定具体条目，是列表内部操作，无全局默认键；没有另造失败清单命令。

## 验证

- pnpm test：1045 通过，含名称 trim/NFC 匹配、重命名新增/覆盖已有名、1.5 秒防抖/1499ms 边界、连续输入、列表点选和销毁取消。假时钟保证秒级执行。日志 /tmp/m4-cleanup-tests-final.log。
- pnpm typecheck、lint:colors、lint:arch、lint:i18n 全通过。
- cargo test --workspace --lib：核心 1158 通过/4 ignored，外壳 93 通过/1 ignored；新增失效条目移除后继续执行、可再次入队、最后一个失效时空开启不占线程测试。日志 /tmp/m4-cleanup-rust.log。
- cargo check --workspace、pnpm build 通过。日志 /tmp/m4-cleanup-check.log、/tmp/m4-cleanup-build.log。
- pnpm check:export 通过：空预设提示、列表无多余按钮、名称检测等待与标题/按钮切换、96px 数字框、紧凑保存/伸展运行同排、队列失败原因与重试；之前的回焦点/返回工作流和 300 张跨页稳定性仍通过。日志 /tmp/m4-cleanup-export-final.log。
- pnpm smoke:ui http://localhost:1420/ 通过，problems=[]。日志 /tmp/m4-cleanup-ui.log。
- pnpm debug:win 首次被活着的旧应用占用阻止，崔总关闭后重建成功。最终 EXE 2026-09-26 23:56:06.210 CST，dist 23:53:26.470 CST，worker 23:54:53.076 CST、raybend-worker-proto-v3；四项嵌入资源/时间/协议核对通过。日志 /tmp/m4-cleanup-win-final.log。
- git diff --check 通过。

## 遗留与验收边界

Pencil MCP 的 get_app_state/read_skill 均报告无法连接 visual_studio_code（transport not connected）；按项目绕过纪律先完成代码、说明与构建。design/export.md 已明确废弃旧预览/摘要/清单设计，但 .pen 仍需连接恢复后同步。没有直接解析或编辑 .pen 文件。

真实 Windows 交互、DPI、真实照片与色彩仍由崔总验收；上述为单元、编译及可重复合成后端冒烟。未发布、push、打 tag，也未实施此前暂缓的外部编辑器入口。
