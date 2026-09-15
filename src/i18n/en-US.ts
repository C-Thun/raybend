/**
 * English language pack.
 *
 * Typed as `Record<MessageKey, string>` so a missing key is a **compile error** —
 * that is the completeness guarantee (no runtime test needed).
 *
 * Wording follows UI conventions for photo software rather than word-for-word
 * translation of the Chinese. See DESIGN.md §11.
 */

import type { MessageKey } from "./index";

export const enUS: Record<MessageKey, string> = {
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
 "common.empty": "Nothing here",
 "common.retry": "Retry",
 "common.selected_summary": "{dirs} folders · {photos} photos selected",

 "source.recent": "Recent",
 "source.tree": "Source",
 "source.selected": "Selected",
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
 "grid.by_time": "By time",
 "grid.select_all_day": "Select whole day",
 "grid.select_all_range": "Select this range",
 "grid.unknown_time": "Unknown time",
 "grid.excluded": "Excluded",
 "grid.pick_dir": "Pick a folder on the left",
 "grid.empty_dir": "No photos in this folder",
 "grid.load_error": "Can't read this folder: {message}",

 "tools.batch_exclude": "Exclude selected",

 "repo.title": "Libraries",
 "repo.create": "New library",
 "repo.import": "Import",
 "repo.avoid_duplicates": "Skip duplicates",
 "repo.empty": "No libraries yet — create one first",
 "repo.online": "Online",
 "repo.offline": "Offline",
 "repo.remount": "Find again",
 "repo.remount_failed": "Library not found (tried {tried} locations)",
 "repo.path_count": "{n} paths",
 "repo.card.selected": "Selected",
 "repo.count_unknown": "—",
 "repo.create.name": "Name",
 "repo.create.path": "Library root",
 "repo.create.browse": "Browse…",
 "repo.create.browse_unavailable": "No folder picker in this environment — type the path instead",
 "repo.create.intro": "The library creates catalog.db and photos/ here; imported photos land under photos/ following the template.",
 "repo.create.hint_existing": "This folder already holds the library \"{name}\" — it will be registered, not overwritten",
 "repo.create.hint_existing_registered": "This folder is the registered library \"{name}\" — a new path will be added",
 "repo.create.hint_broken": "There is a catalog.db here but it can't be read: {message}",
 "repo.create.hint_not_directory": "This path is not a folder",
 "repo.create.submit": "Create library",
 "repo.create.failed": "Couldn't create the library: {message}",
 "repo.import_hint": "Check folders on the left, then pick a library",
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
};
