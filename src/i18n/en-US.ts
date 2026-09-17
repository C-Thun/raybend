/**
 * English language pack.
 *
 * Typed as `Record<MessageKey, string>` so a missing key is a **compile error** —
 * that is the completeness guarantee (no runtime test needed).
 *
 * Wording follows UI conventions for photo software rather than word-for-word
 * translation of the Chinese. See DESIGN.md §11.
 */

import type { MessageKey } from "./index.ts";

/*
 * 键必须与 `zh-CN.ts` 一一对应（`MessageKey = keyof typeof zhCN`）：
 * 这里多一个键会类型报错，少一个键会让英文界面显示成 key。
 * `locale-parity.test.ts` 双向断言，`pnpm test` 时就会炸。
 */
export const enUS: Record<MessageKey, string> = {
 "import.title": "Importing",
 "import.done_title": "Import finished",
 "import.preparing": "Preparing…",
 "import.summary": "{dirs} folders · {total} photos — what's imported stays in the library",
 "import.stage.scan": "Scan",
 "import.stage.plan": "Plan",
 "import.stage.import": "Import",
 "import.stage.thumbs": "Thumbnails",
 "import.stage.done": "Done",
 "import.imported": "Imported",
 "import.skipped": "Skipped",
 "import.duplicates": "duplicates",
 "import.failed": "Failed",
 "import.pause": "Pause",
 "import.resume": "Resume",
 "import.cancel_button": "Cancel import",
 "import.cancel_confirm": "Cancel this import?",
 "import.keep_running": "Keep importing",
 "import.keep_partial": "Photos already imported stay in the library — cancelling does not roll back",
 "import.export_errors": "Export error list",
 "import.no_backend": "No import backend in this environment (the dev preview is UI-only)",
 "import.exported_to": "Exported to {path}",
 "import.errors_truncated": "Showing the last {shown} of {total}",
 "import.reveal": "Show these photos in the library",
 "import.space_warning": "Target volume may be too small: need about {needed}, {free} free",
 "import.space_continue": "Continue anyway",
 "common.save": "Save",
 "repo.settings_title": "Library settings",
 "repo.settings_hint": "Only one setting in M1: the import template (where photos land in the library)",
 "repo.settings_offline": "This library is offline — the template cannot be changed until it is mounted (plug the drive in, or click the offline icon in the list)",
 "repo.settings_hint_named": "Import template for \"{name}\" — decides where future imports land",
 "repo.template_label": "Import template",
 /**
  * 语言自己的名字（**endonym**）—— 只给帮助菜单里的那条语言切换用。
  *
  * ⚠️ 它**不能**走 `t()` 取当前语言包：那条菜单项要显示的是**目标语言**的名字
  * （中文界面显示 `English`、英文界面显示 `中文`），这才是「切过去会变成什么」的意义。
  * 所以调用方直接用**目标语言的包**取这一条：`LOCALES[next]["locale.name"]`。
  */
 "locale.name": "English",
 "app.name": "RayBend",

 "titlebar.menu.help": "Help",
 "titlebar.menu.help.about": "About",
 "titlebar.theme.toggle": "Toggle theme",
 "titlebar.density.compact": "Compact",
 "titlebar.density.loose": "Loose",
 "titlebar.window.minimize": "Minimize",
 "titlebar.window.maximize": "Maximize",
 "titlebar.window.restore": "Restore",
 "titlebar.window.close": "Close",

 "flow.label": "Workflow",
 "flow.import": "Import",
 "flow.browse": "Browse",
 "flow.edit": "Edit",
 "flow.export": "Export",
 "flow.tool.snap": "Snap",

 "exif.camera": "Camera",
 "exif.lens": "Lens",
 "exif.focal": "Focal length",
 "exif.aperture": "Aperture",
 "exif.shutter": "Shutter speed",
 "exif.iso": "ISO",
 "exif.dimensions": "Dimensions",
 "exif.megapixels": "Megapixels",
 "exif.format": "Format",
 "exif.empty": "No photo selected",

 "common.easy_copy.hint": "Click to copy",
 "common.easy_copy.done": "Copied",
 "common.easy_destroy.confirm": "Remove this?",
 "common.easy_destroy.shift_hint": "Hold Shift to skip confirmation",
 "common.remove": "Remove",
 "common.close": "Close",
 "common.cancel": "Cancel",
 "common.confirm": "OK",
 "common.loading": "Loading",
 /**
  * The timeout sentence — `{what}` is the action that hung (itself a message key).
  * Composed by `timeoutMessage()` in `i18n/index.ts`: `lib/timeout.ts` lives in the
  * pure-logic layer and may not import i18n.
  */
 "common.timeout": "{what} did not respond within {seconds}s (the backend may have crashed)",
 "common.empty": "Nothing here",
 "common.resize_left": "Resize left column",
 "common.retry": "Retry",
 /**
  * The one message for "this only exists in the desktop app" (browser preview).
  * Deliberately generic: it can only really show up in the dev preview.
  */
 "common.desktop_only": "This needs the desktop app (browser preview here)",
 "common.selected_summary": "{dirs} folders · {photos} photos selected",

 "source.recent": "Recent",
 "source.tree": "Source",
 "source.selected": "Selected",
 "source.recent_unmounted": "This folder cannot be found right now (drive unplugged or renamed)",
 "source.include_subdirs": "Include subdirectories",

 // ── Left column: states and hints (M1-5) ─────────
 "source.recent.empty": "No recent folders yet",
 "source.tree.empty": "No sources available",
 "source.selected.empty": "Checked folders show up here",
 "source.load_error": "Can't read this location: {message}",
 "source.remove_from_recent": "Remove from recent",
 "source.remove_dir": "Remove {path}",
 "source.check": "Check",
 "source.expand": "Expand",
 "source.collapse": "Collapse",
 "source.photo_count": "{n} photos",
 "source.counting": "Counting…",
 "source.volume.local": "Local disk",
 "source.volume.removable": "Removable disk",
 "source.volume.optical": "Optical disc",
 "source.volume.network": "Network location",
 "source.volume.cloud": "Cloud drive",
 "source.volume.unknown": "Unknown source",

 "grid.count": "{n} items",
 "grid.exclude": "Exclude",
 "grid.fit": "Fit to screen",
 "grid.actual_size": "Actual size",
 "grid.back": "Back",
 "grid.zoom": "Zoom",
  "grid.rating": "{n} stars",
 "grid.color_label": "Color label",
 "grid.locked": "Locked",
 "grid.by_time": "By time",
 "grid.select_all_day": "Select whole day",
 "grid.select_all_range": "Select this range",
  "viewer.back": "Back",
 "viewer.fit_label": "Fit",
 "viewer.zoom_in": "Zoom in",
 "viewer.zoom_out": "Zoom out",
 "viewer.fit": "Fit / 100%",
 "grid.unknown_time": "Unknown time",
 "grid.excluded": "Excluded",
 "grid.pick_dir": "Pick a folder on the left",
 "grid.loading_dir": "Reading folder…",
 "grid.timeout.meta": "Reading photo info",
 "grid.empty_dir": "No photos in this folder",
 "grid.load_error": "Can't read this folder: {message}",

 "tools.batch_exclude": "Exclude selected",

 "import.dest_library": "Destination library",
  "repo.title": "Libraries",
 "repo.create": "New library",
 /**
  * 「已排除 N 张」—— 与「已选择 M 张照片」同一行、靠右显示。
  * 为什么要单独说一句：导入时用户心里算的是「我这次要带进去几张」，
  * 而排除是**跨源**的（切目录不丢），不显式说出来就会显得总数对不上。
  */
 "import.excluded_count": "{n} excluded",
 /** The hanging action in the timeout sentence (see `common.timeout`) */
 "import.timeout.command": "Import command",
 "import.timeout.start": "Start import",
 "import.timeout.subscribe": "Progress subscription",
 "import.timeout.status": "Reading import state",
 "import.timeout.export": "Exporting the error list",
 "import.timeout.precheck": "Space check",
 "repo.import": "Import",
 "repo.avoid_duplicates": "Skip duplicates",
 "repo.empty": "No libraries yet — create one to import into",
 "repo.online": "Online",
 "repo.offline": "Offline",
 "repo.remount": "Find again",
 "repo.remount_failed": "Library not found (tried {tried} locations)",
 "repo.path_count": "{n} paths",
 "repo.count_unknown": "—",
 "repo.create.name": "Name",
 "repo.create.path": "Library root",
 "repo.create.path_hint": "D:\\Photos\\Library",
 "repo.create.browse": "Browse…",
 "repo.create.browse_unavailable": "No folder picker in this environment — type the path instead",
 "repo.create.intro": "The library creates catalog.db and photos/ here; imported photos land under photos/ following the template.",
 "repo.create.hint_existing": "This folder already holds the library \"{name}\" — it will be registered, not overwritten",
 "repo.create.hint_existing_registered": "This folder is the registered library \"{name}\" — a new path will be added",
 "repo.create.hint_broken": "There is a catalog.db here but it can't be read: {message}",
 "repo.create.hint_not_directory": "This path is not a folder",
 "repo.create.submit": "Create library",
 "repo.create.failed": "Couldn't create the library: {message}",
 "repo.import_hint": "Check the folders on the left, then pick a destination library",
 "repo.import_running": "Importing… (lands in M1-6)",

 // ── About (Help → About) ───────────────────────────────
 // Wording note: "Repository" is what users look for; keep the URL text itself
 // (the About dialog renders the host without the scheme).
 "about.title": "About",
 "about.version": "Version",
 "about.license": "License",
 "about.repository": "Repository",
 "about.privacy": "Your photos and folders are never modified",
 "about.debug": "Debug info",
 "about.build_time": "Built",
 "about.runtime": "Runtime",
 "about.runtime_browser": "Browser (dev preview)",

 // ── 浏览 / Browse（M2-W1）─────────────────────
 "browse.count": "{n} photos",
 "browse.selected": "{n} selected",
 "browse.filter": "Filter",
 "browse.filterHint": "When on, the marks to the right become filter conditions",
 "browse.flag": "Flag",
 "browse.flagClear": "Clear all flags",
 "browse.pick": "Pick",
 "browse.reject": "Reject",
 "browse.like": "Like",
 "browse.colorNone": "No color",
 "browse.tag": "Tags",
 "browse.lock": "Lock",
 "browse.lockNone": "Unlocked",
 "browse.lockNoDelete": "Lock 1: cannot delete",
 "browse.lockNoEdit": "Lock 2: cannot edit",
 "browse.clearRating": "Clear rating",
 "browse.searchPlaceholder": "Search libraries or folders",
 "browse.noRepository": "No library yet. Create one in the Import workflow first.",
  "browse.reposLoading": "Reading libraries…",
  "browse.reposError": "Could not read libraries; the list is empty for now (hover for the reason)",
 "browse.allRepositories": "Show all libraries",
 "browse.collapseRepositories": "Collapse library list",
 "browse.wholeRepository": "Whole library",
 "browse.emptyTree": "This library has no folders yet",
 "browse.expand": "Expand",
 "browse.collapse": "Collapse",
 "browse.offline": "Offline",
 "browse.loading": "Loading library…",
 "browse.empty_lib": "No photos in this library yet",
 "browse.load_error": "Can't read this library: {message}",
 "browse.noSelection": "Select a photo to see its details here",
 "browse.exif": "Capture",
 "browse.fileInfo": "File",
 "browse.fieldCamera": "Camera",
 "browse.fieldLens": "Lens",
 "browse.fieldFocal": "Focal",
 "browse.fieldAperture": "Aperture",
 "browse.fieldExposure": "Shutter",
 "browse.fieldIso": "ISO",
 "browse.fieldSize": "Size",
 "browse.fieldFileName": "Name",
 "browse.fieldTakenAt": "Taken",
   "browse.dirMenu": "Folder actions",
  "browse.deleteEmptyDir": "Delete empty folder",
  "browse.createSubdir": "Create subfolder",
  "browse.deleteEmptyTitle": "Delete this empty folder?",
  "browse.deleteEmptyBody": "Its {n} empty subfolder(s) will go with it. There are no files inside — nothing else is touched.",
  "browse.deleteEmptyOnly": "Empty folders only",
  "browse.deleteDirFailed": "Delete failed: {message}",
  "browse.createSubdirTitle": "Create subfolder",
  "browse.subdirName": "Folder name",
  "browse.subdirNamePlaceholder": "e.g. 2026-09-17",
  "browse.createFailed": "Create failed: {message}",
  "browse.dirUpToDate": "Folder is already empty",
"browse.fieldPath": "Path",
 "browse.fieldFileType": "Type",
 "browse.fieldFileSize": "Size",
 "browse.raw": "RAW",
 "browse.missingFile": "This file is no longer on disk",
};
