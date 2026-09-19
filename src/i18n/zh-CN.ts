/**
 * 中文语言包（主语言，key 的基准）。
 *
 * 约定（DESIGN.md §11）：
 *   - key 用英文小写点分层级，与组件层级对齐
 *   - 界面文案零硬编码，一律走 key
 *   - 中文不是英文的机械翻译，按中文习惯写
 *
 * `zhCN` 的形状决定了 `MessageKey` 联合类型；
 * `enUS` 声明为 `Record<MessageKey, string>` —— **漏译会在编译期报错**，
 * 所以不需要额外的运行时完整性测试。
 */

/*
 * 键必须与 `en-US.ts` **一一对应**：`MessageKey = keyof typeof zhCN`，
 * 少了哪边都会在切语言时才暴露 —— `locale-parity.test.ts` 会在 `pnpm test` 里当场炸。
 */
export const zhCN = {
 // ── 应用 ───────────────────────────────────────────────
 // ── 数据库升级（M2-W2）────────────────────────────────
 "migration.title": "正在升级数据库",
 "migration.body": "{label}的结构要从 v{from} 升到 v{to}，正在改写数据…",
 "migration.hint": "升级期间界面暂时不可操作，完成后自动恢复。请不要关闭程序。",

 // ── 应用 ───────────────────────────────────────────────
 "import.title": "导入中",
 "import.done_title": "导入完成",
 "import.preparing": "正在准备…",
 "import.summary": "{dirs} 个目录 · 共 {total} 张 —— 已导入的会留在库里",
 "import.stage.scan": "扫描",
 "import.stage.plan": "规划",
 "import.stage.import": "导入",
 "import.stage.thumbs": "缩略图",
 "import.stage.done": "完成",
 "import.imported": "已导入",
 "import.skipped": "跳过",
 "import.duplicates": "其中重复",
 "import.failed": "失败",
 "import.pause": "暂停",
 "import.resume": "继续",
 "import.cancel_button": "取消导入",
 "import.cancel_confirm": "取消这次导入？",
 "import.keep_running": "继续导入",
 "import.keep_partial": "已经导入的照片会保留在库里，取消不会回滚",
 "import.export_errors": "导出错误清单",
 "import.no_backend": "当前环境没有导入后端（开发预览里只有界面）",
 "import.exported_to": "已导出到 {path}",
 "import.errors_truncated": "只显示了最后 {shown} 条（共 {total} 条）",
 "import.reveal": "在库中查看这些照片",
 "import.space_warning": "目标卷空间可能不够：需要约 {needed}，可用 {free}",
 "import.space_continue": "仍然继续",
 "common.save": "保存",
 "repo.counts_photos": "相片数量（不含 _RAW）",
 "repo.counts_images": "图片数量（含 _RAW）",
 "repo.counts_hint": "相片 = 库内可见的图；图片 = 连 _RAW 里的原片一起算。数字随导入与进目录自动更新。",
 "repo.rebuild_title": "重建数据",
 "repo.rebuild_hint": "重扫整个库：对齐磁盘上的文件、重读缺失的元数据、重算计数",
 "repo.rebuild_button": "重建",
 "repo.rebuild_confirm_title": "重建数据",
 "repo.rebuild_confirm": "这会重扫整个库，可能要花一些时间；期间界面不要关。库里的评分、标签、色标等编辑不会丢（重建只补事实：文件、元数据、计数）。",
 "repo.rebuild_confirm_ok": "开始重建",
 "repo.rebuild_done": "重建完成：扫到 {scanned} 个文件 · 新登记 {registered} · 标记缺失 {missing} · 补上元数据 {filled} 张 · 现在 相片 {photos} / 图片 {images}",
 "repo.settings_title": "库设置",
 "repo.settings_hint": "M1 里只有一项：导入模版（决定照片在库里的落法）",
 "repo.settings_offline": "这个库现在不在线 —— 模版改不了，先把它挂上（插好盘，或点列表上的离线图标）",
 "repo.settings_hint_named": "「{name}」的导入模版 —— 决定以后导入的照片落在库里的哪条路径",
 "repo.template_label": "导入模版",
 /**
  * 语言自己的名字（**endonym**）—— 只给帮助菜单里的那条语言切换用。
  *
  * ⚠️ 它**不能**走 `t()` 取当前语言包：那条菜单项要显示的是**目标语言**的名字
  * （中文界面显示 `English`、英文界面显示 `中文`），这才是「切过去会变成什么」的意义。
  * 所以调用方直接用**目标语言的包**取这一条：`LOCALES[next]["locale.name"]`。
  */
 "locale.name": "中文",
 "app.name": "光伴",

 // ── titlebar（应用标题行，design/main.md §2.1）──────────
 "titlebar.menu.help": "帮助",
 "titlebar.menu.help.about": "关于",
 "titlebar.theme.toggle": "切换主题",
 "titlebar.density.compact": "紧凑",
 "titlebar.density.loose": "宽松",
 "titlebar.window.minimize": "最小化",
 "titlebar.window.maximize": "最大化",
 "titlebar.window.restore": "还原",
 "titlebar.window.close": "关闭",

 // ── flowbar：工作流（design/main.md §2.2）──────────────
 "flow.label": "工作流",
 "flow.import": "导入",
 "flow.browse": "浏览",
 "flow.edit": "编辑",
 "flow.export": "导出",
 "flow.tool.snap": "吸附",

 // ── flowbar：图片信息（分组即内容结构，见 §4.8）────────
 "exif.camera": "机型",
 "exif.lens": "镜头",
 "exif.focal": "焦距",
 "exif.aperture": "光圈",
 "exif.shutter": "快门",
 "exif.iso": "感光度",
 "exif.dimensions": "尺寸",
 "exif.megapixels": "像素",
 "exif.format": "格式",
 "exif.empty": "未选择照片",

 // ── 通用 ───────────────────────────────────────────────
 "common.easy_copy.hint": "点击复制",
 "common.easy_copy.done": "已复制",
 "common.easy_destroy.confirm": "确定要移除吗？",
 "common.easy_destroy.shift_hint": "按住 Shift 可跳过确认",
 "common.remove": "移除",
 "common.close": "关闭",
 "common.cancel": "取消",
 "common.confirm": "确定",
 "common.loading": "加载中",
 /**
  * 「超时」那句话 —— `{what}` 是卡住的那个动作（它自己也是一条文案）。
  * 拼装入口是 `i18n/index.ts` 的 `timeoutMessage()`：`lib/timeout.ts` 在纯逻辑层，
  * 不允许 import i18n，所以句子由应用层拼好再递进去。
  */
 "common.timeout": "{what}没有在 {seconds} 秒内回应（后端可能已经挂了）",
 "common.empty": "暂无内容",
 "common.resize_left": "调整左列宽度",
 "common.resize_right": "调整右列宽度",

 "common.retry": "重试",
 /**
  * 浏览器预览（非 Tauri 运行时）里碰到只有桌面端才有的能力时的统一说法。
  * 一律用这一句，不按命令分别写 —— 它能真正出现的地方只有开发预览。
  */
 "common.desktop_only": "这个功能要在桌面应用里运行（当前是浏览器预览）",
 "common.selected_summary": "已选择 {dirs} 个目录 · {photos} 张照片",

 // ── 左列：来源（design/main.md §3.1）──────────────────
 "source.recent": "最近",
 "source.tree": "来源",
 "source.selected": "已选目录",
 "source.recent_unmounted": "这个目录现在找不到（盘没插，或被改名）",
 "source.include_subdirs": "包含子目录",

 // ── 左列：状态与提示（M1-5）──────────────────────
 "source.recent.empty": "还没有导入过的目录",
 "source.tree.empty": "没有可用的来源",
 "source.selected.empty": "勾选目录后会出现在这里",
 "source.load_error": "读不了这个位置：{message}",
 "source.remove_from_recent": "从最近中移除",
 "source.remove_dir": "移除 {path}",
 "source.check": "勾选",
 "source.expand": "展开",
 "source.collapse": "折叠",
 "source.photo_count": "{n} 张照片",
 "source.counting": "统计中…",
 "source.volume.local": "本地磁盘",
 "source.volume.removable": "可移动磁盘",
 "source.volume.optical": "光盘",
 "source.volume.network": "网络位置",
 "source.volume.cloud": "云盘",
 "source.volume.unknown": "未知来源",

 // ── 中列：照片区（design/main.md §3.2、DESIGN.md §12.7）─
 "grid.count": "{n} 张",
 "grid.exclude": "排除",
 "grid.fit": "适合屏幕",
 "grid.actual_size": "实际大小",
 "grid.back": "返回",
 "grid.zoom": "缩放",
  "grid.rating": "{n} 星",
 "grid.color_label": "颜色标记",
 "grid.locked": "已加锁",
 "grid.by_time": "按时间",
  "grid.info": "信息",
 "grid.select_all_day": "全选当天",
 "grid.select_all_range": "全选此段",
  "viewer.back": "返回",
 "viewer.fit_label": "适配",
 "viewer.zoom_in": "放大",
 "viewer.zoom_out": "缩小",
 "viewer.fit": "适配窗口 / 100%",
 "grid.unknown_time": "未知时间",
 "grid.excluded": "已排除",
 "grid.pick_dir": "从左侧选一个目录",
 "grid.loading_dir": "正在读取目录…",
 "grid.timeout.meta": "读取照片信息",
 "grid.empty_dir": "这个目录里没有照片",
 "grid.load_error": "读不了这个目录：{message}",

 // ── toolsbar（内容居中，随工作流装配）──────────────────
 "tools.batch_exclude": "批量排除",

 // ── 右列：库（design/main.md §3.3）────────────────────
 "import.dest_library": "导入目标库",
  "repo.title": "库",
 "repo.create": "新建库",
 /**
  * 「已排除 N 张」—— 与「已选择 M 张照片」同一行、靠右显示。
  * 为什么要单独说一句：导入时用户心里算的是「我这次要带进去几张」，
  * 而排除是**跨源**的（切目录不丢），不显式说出来就会显得总数对不上。
  */
 "import.excluded_count": "已排除 {n} 张",
 /** 超时句里的「卡住的那个动作」（见 `common.timeout`） */
 "import.timeout.command": "导入命令",
 "import.timeout.start": "启动导入",
 "import.timeout.subscribe": "订阅导入进度",
 "import.timeout.status": "读导入状态",
 "import.timeout.export": "导出错误清单",
 "import.timeout.precheck": "空间预检",
 "repo.import": "导入",
 "repo.avoid_duplicates": "避免重复导入",
 "repo.empty": "还没有库 —— 先新建一个作为导入目标",
 "repo.online": "在线",
 "repo.offline": "离线",
 "repo.remount": "重新查找",
 "repo.remount_failed": "没找到这个库（已试过 {tried} 处）",
 "repo.path_count": "{n} 条路径",
 "repo.count_unknown": "—",
 "repo.create.name": "名称",
 "repo.create.path": "库根目录",
 "repo.create.path_hint": "D:\\Photos\\Library",
 "repo.create.browse": "浏览…",
 "repo.create.browse_unavailable": "这个环境没有目录选择器，手动填写路径即可",
 "repo.create.intro": "库会在这个目录里建立 catalog.db 与 photos/，导入的照片按模版落在 photos/ 之下。",
 "repo.create.hint_existing": "该目录里已有库「{name}」—— 将登记为已有库（不会覆盖）",
 "repo.create.hint_existing_registered": "该目录是已登记的库「{name}」—— 将登记一条新路径",
 "repo.create.hint_broken": "该目录下有 catalog.db，但读不出来：{message}",
 "repo.create.hint_not_directory": "这个路径不是一个目录",
 "repo.create.submit": "新建库",
 "repo.create.failed": "建库失败：{message}",
 "repo.import_hint": "先在左边勾选目录，再选一个导入目标库",
 "repo.import_running": "导入中…（M1-6 实现）",

 // ── 关于（帮助 → 关于）─────────────────────────────
 "about.title": "关于",
 "about.version": "版本",
 "about.license": "许可",
 "about.repository": "仓库",
 "about.privacy": "你的照片与目录永远不会被改动",
 "about.debug": "调试信息",
 "about.build_time": "构建时间",
 "about.runtime": "运行环境",
 "about.runtime_browser": "浏览器（开发预览）",

 // ── 浏览 / Browse（M2-W1）─────────────────────
 "browse.count": "共 {n} 张",
 "browse.selected": "已选 {n} 张",
 "browse.filter": "筛选",
 "browse.filterAny": "任一",
 "browse.filterAll": "全部",
 "browse.filterCombinator": "条件之间的关系",
 "browse.filterRemove": "去掉条件：{name}",
 "browse.filterEmpty": "还没有筛选条件 —— 点上面的标记来筛",
 "browse.sort": "排序",
 "browse.sortTakenAt": "拍摄时间",
 "browse.sortImportedAt": "导入时间",
 "browse.sortFileName": "文件名",
 "browse.sortRating": "评分",
 "browse.sortCamera": "机型",
 "browse.sortAsc": "升序",
 "browse.sortDesc": "降序",

 "browse.filterHint": "开启后，后面的标记都变成筛选条件",
 "browse.flag": "旗标",
 "browse.flagClear": "清空旗标",
 "browse.flagCleared": "已清空所有旗标",

 "browse.flagClearConfirm": "清空所有旗标？跨库、跨目录都生效（效果类似关掉软件重开）。",
 "browse.markSkippedLocked": "{n} 张有锁，没有改动",
 "browse.markNothingChanged": "没有需要改动的照片",
 "browse.pick": "留下",
 "browse.reject": "弃掉",
 "browse.like": "喜欢",
 "browse.undo": "撤销",
 "browse.redo": "重做",
 "browse.undoWith": "撤销：{label}",
 "browse.redoWith": "重做：{label}",
 "browse.markedCount": "已改 {n} 张",
 "toast.close": "关闭提示",

 "browse.colorNone": "无色",
 /* 五个色标名 + 「不喜欢 / 未标记喜欢 / 未上锁」：工具条 aria-label、筛选 chips、
    看图状态栏三处共用（`features/browse/labels.ts` 是唯一的取用口） */
 "browse.filterRatingAtLeast": "≥{n} 星",
 "browse.flagWith": "有旗标",
 "browse.flagWithout": "无旗标",
 "browse.flagRejected": "已弃",
 "browse.flagToggle": "旗标",
 "browse.colorRed": "红色",
 "browse.colorYellow": "黄色",
 "browse.colorGreen": "绿色",
 "browse.colorCyan": "青色",
 "browse.colorBlue": "蓝色",
 "browse.colorPurple": "紫色",
 "browse.dislike": "不喜欢",
 "browse.likeNone": "未标记喜欢",
 "browse.lockNone": "未上锁",
 "browse.tagsSection": "标签",
 "browse.tag": "标签",
 "browse.tagsTitle": "标签",
 "browse.tagsTitleBatch": "给 {n} 张照片加标签",
 "browse.tagsDescSingle": "{name} · 已有 {n} 个标签",
 "browse.tagsDescBatch": "批量只会添加：已有标签不会被移除，重复的自动去重",
 "browse.tagsPlaceholder": "输入即搜，回车创建新标签",
 "browse.tagsNoMatch": "没有匹配的标签",
 "browse.tagsCreate": "创建「{name}」",
 "browse.tagsAlready": "已有",
 "browse.tagsRemove": "移除标签 {name}",
 "browse.tagsAdd": "添加标签",
 "browse.tagsSaved": "已给 {n} 张改标签",

 "browse.tagsBatchHint": "标签右边没有叉 —— 想移除请切回单张编辑",
 "browse.lock": "锁",
 "browse.lockNoDelete": "一级锁：不能删",
 "browse.lockNoEdit": "二级锁：不能编辑",
 "browse.clearRating": "清除评分",
 "browse.searchPlaceholder": "搜索库或目录",
 "browse.noRepository": "还没有库。先在导入工作流里建一个库。",
 // 「还没读到」与「真的没有库」在界面上长得一样，必须分开说：
 // 取库列表要对每个库开一次连接数照片，慢盘上可能好几秒
 "browse.reposLoading": "正在读库…",
 "browse.reposError": "读库失败，左侧暂时是空的（鼠标悬停看原因）",
"browse.allRepositories": "查看所有库",
 "browse.emptyTree": "这个库里还没有目录",
 "browse.expand": "展开",
 "browse.collapse": "收起",
 "browse.offline": "离线",
 "browse.loading": "正在加载库…",
 "browse.empty_lib": "这个库里还没有照片",
"browse.pickDirectory": "在左边挑一个目录，这里就显示它的照片",
 "browse.load_error": "读不了这个库：{message}",
 "browse.noSelection": "选中一张照片后，这里显示它的信息",
 "browse.exif": "拍摄信息",
 "browse.preview": "预览",
 "browse.histogram": "直方图",
 "browse.histogramHint": "整张照片的明暗与色彩分布（三通道合成）",
 "browse.histogramEmpty": "这张照片暂时没有直方图",
 "browse.compareNoSize": "还没读到这张的尺寸",
 "browse.compareLimit": "已选 {n} 张，只对比最近选中的 4 张",
 "browse.fileInfo": "文件信息",
 "browse.fieldCamera": "机身",
 "browse.fieldLens": "镜头",
 "browse.fieldFocal": "焦段",
 "browse.fieldAperture": "光圈",
 "browse.fieldExposure": "快门",
 "browse.fieldIso": "感光度",
 "browse.fieldSize": "尺寸",
 "browse.fieldFileName": "文件名",
 "browse.fieldTakenAt": "拍摄时间",
   "browse.dirMenu": "目录操作",
  "browse.deleteEmptyDir": "删除空目录",
  "browse.createSubdir": "创建子目录",
  "browse.deleteEmptyTitle": "删除这个空目录？",
  "browse.deleteEmptyBody": "它下面还有 {n} 个空目录，会一起删掉。目录里没有文件，不影响照片。",
  "browse.deleteEmptyOnly": "只删空目录",
  "browse.deleteDirFailed": "删除失败：{message}",
  "browse.createSubdirTitle": "创建子目录",
  "browse.subdirName": "子目录名",
  "browse.subdirNamePlaceholder": "例如 2026-09-17",
  "browse.createFailed": "创建失败：{message}",
  "browse.dirUpToDate": "目录已是空的",
"browse.fieldPath": "所在路径",
 "browse.fieldFileType": "格式",
 "browse.fieldFileSize": "大小",
 "browse.raw": "RAW",
 "browse.missingFile": "这个文件已经不在磁盘上了",
 "browse.deleteTitle": "删除照片",
 "browse.deleteConfirm": "删除 {n} 张照片？会移到系统回收站（还能再捞回来）。",
 "browse.deleteDone": "已移到回收站 {n} 张",
 "browse.deleteBlocked": "{n} 张有锁，没有删",
 "browse.deleteGone": "{n} 张的文件已经不在了（记录已清理）",
 "browse.deleteFailedTitle": "有 {n} 张没删掉",
 "browse.deleteFailedHint": "它们还在库里，下面是每一条的原因。",

} as const;
