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

export const zhCN = {
 // ── 应用 ───────────────────────────────────────────────
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
 "common.empty": "暂无内容",
 "common.retry": "重试",
 "common.selected_summary": "已选择 {dirs} 个目录 · {photos} 张照片",

 // ── 左列：来源（design/main.md §3.1）──────────────────
 "source.recent": "最近",
 "source.tree": "来源",
 "source.selected": "已选目录",
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
 "grid.by_time": "按时间",
 "grid.select_all_day": "全选当天",
 "grid.select_all_range": "全选此段",
 "grid.unknown_time": "未知时间",
 "grid.excluded": "已排除",
 "grid.pick_dir": "从左侧选一个目录",
 "grid.empty_dir": "这个目录里没有照片",
 "grid.load_error": "读不了这个目录：{message}",

 // ── toolsbar（内容居中，随工作流装配）──────────────────
 "tools.batch_exclude": "批量排除",

 // ── 右列：库（design/main.md §3.3）────────────────────
 "repo.title": "库",
 "repo.create": "新建库",
 "repo.import": "导入",
 "repo.avoid_duplicates": "避免重复导入",
 "repo.empty": "还没有库，先建一个",
 "repo.online": "在线",
 "repo.offline": "离线",
 "repo.remount": "重新查找",
 "repo.remount_failed": "没找到这个库（已试过 {tried} 处）",
 "repo.path_count": "{n} 条路径",
 "repo.card.selected": "已选中",
 "repo.count_unknown": "—",
 "repo.create.name": "名称",
 "repo.create.path": "库根目录",
 "repo.create.browse": "浏览…",
 "repo.create.browse_unavailable": "这个环境没有目录选择器，手动填写路径即可",
 "repo.create.intro": "库会在这个目录里建立 catalog.db 与 photos/，导入的照片按模版落在 photos/ 之下。",
 "repo.create.hint_existing": "该目录里已有库「{name}」—— 将登记为已有库（不会覆盖）",
 "repo.create.hint_existing_registered": "该目录是已登记的库「{name}」—— 将登记一条新路径",
 "repo.create.hint_broken": "该目录下有 catalog.db，但读不出来：{message}",
 "repo.create.hint_not_directory": "这个路径不是一个目录",
 "repo.create.submit": "新建库",
 "repo.create.failed": "建库失败：{message}",
 "repo.import_hint": "先在左边勾选目录、再选一个库",
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
} as const;
