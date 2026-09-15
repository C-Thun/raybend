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

 "tools.batch_exclude": "Exclude selected",

 "repo.title": "Libraries",
 "repo.create": "New library",
 "repo.import": "Import",
 "repo.avoid_duplicates": "Skip duplicates",
 "repo.empty": "No libraries yet — create one first",
};
