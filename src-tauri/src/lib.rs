//! raybend（光伴）桌面外壳。
//!
//! 这一层**刻意保持很薄**：只做「窗口 + WebView + 命令转发」。
//! 业务逻辑全部在 `raybend` crate 内，且该 crate **不依赖 Tauri**
//! —— 这是为 Tauri 3.0 的运行时重构预留的迁移空间（见 `AGENTS.md` §6.2）。
//!
//! M0-2 的渲染可行性验证会在这里另开一个 `spike-viewport` 调试窗口，
//! 主窗口保持不透明、不受影响。

pub mod browse;
pub mod db;
mod import;
mod migration;
pub mod repo;
pub mod source;
pub mod tags;
pub mod thumbs;

/// IPC 契约测试（拉把 Rust 序列化出的键名与前端 ts 镜像对齐）。
#[cfg(test)]
mod contract;
pub mod dirs;
/// 渲染可行性 spike 的调试窗口（`PLAN.md` A.2）。**按需建窗**，不影响主窗口。
pub mod spike_viewport;

/// 主窗口标签（与 `tauri.conf.json` 的窗口配置、`capabilities/default.json` 对应）。
pub const MAIN_WINDOW_LABEL: &str = "main";

/// 启动闪屏的窗口标签（同上）。
pub const SPLASH_WINDOW_LABEL: &str = "splash";

/// 闪屏的**最短停留时长**：就算主窗口已经就绪，也要等它露满这么久（人类 2026-09-17 定）。
///
/// 这条不是技术限制，是观感：闪屏一闪而过反而像「卡了一下」或「闪屏坏了」，
/// 等满 3 秒才像一次有意的开场。所以 `ui_ready` 只是「最快也要等到这时候」，不自成一条路径。
const SPLASH_MIN_VISIBLE: std::time::Duration = std::time::Duration::from_secs(3);

/// 等「界面就绪」的上限（硬兜底）。
///
/// 超时就直接把主窗口显出来 —— **宁可少一个闪屏，也不能把用户卡在闪屏上**。
/// 前端挂了（白屏、抛错、dev server 没起）时，这条路径就是「和以前一样，直接看到界面」。
///
/// **比最短停留多 1 秒是刻意的**：这样「是前端说好了、还是兜底放的行」从计时上就能分辨，
/// 也让前两者不会在同一瞬间互相盖过去。
/// （不写成 `SPLASH_MIN_VISIBLE + 1s`：常量里的 `Duration` 相加还不是稳定特性；
/// 两者的大小关系由 `splash_fallback_is_later_than_min_visible` 这个单测盯着。）
const SPLASH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

/// 闪屏是什么时候露出来的（`setup()` 里 `show()` 之后记一次；dev 不显示闪屏 ⇒ 永远为空）。
///
/// 它是「至少显示 3 秒」这条规则的锚点：为空 ⇒ 剩余时间算 0 ⇒ dev 下主窗口立刻显示。
static SPLASH_SHOWN_AT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 闪屏还要再待多久（已经待够、或压根没显示过闪屏 ⇒ `ZERO`）。
fn splash_remaining() -> std::time::Duration {
    remaining_until_min_visible(SPLASH_SHOWN_AT.get().copied(), std::time::Instant::now())
}

/// 「还要再等多久」的**纯计算**：把时钟当入参，单测才能不靠 `sleep` 也覆盖边界。
///
/// `shown_at` 为 `None` 表示这次启动压根没露过闪屏（dev）⇒ 不等。
/// 用 `saturating_sub`：闪屏已经露过头（慢机器、兜底已经放行）时返 `ZERO` 而不是 panic。
fn remaining_until_min_visible(
    shown_at: Option<std::time::Instant>,
    now: std::time::Instant,
) -> std::time::Duration {
    match shown_at {
        Some(shown_at) => SPLASH_MIN_VISIBLE.saturating_sub(now.saturating_duration_since(shown_at)),
        None => std::time::Duration::ZERO,
    }
}

/// 命令行标记：带上它启动就顺手把 spike 窗口开出来。
///
/// 为什么需要它：spike 窗口原先只能在**开发页**（`src/dev/SpikeViewport.tsx` 的按钮）里打开，
/// 而打包版根本够不着开发页 —— 可「人类在真机上照 `plans/M2-W1-windows-gpu.md` 逐项验证 GPU」
/// 这条路必须能一键起窗口。`scripts/spike-win.mjs` 就靠它。
const SPIKE_ARG: &str = "--spike=1";

/// 同上，**环境变量**形态。两个都认：脚本是经 WSL→Windows 起进程的，
/// 环境变量能不能透传取决于互操作层，命令行参数则是硬的 —— 两个都给，哪个到了都行。
const SPIKE_ENV: &str = "RAYBEND_SPIKE";

/// 判定「这次启动要不要开 spike 窗口」的**纯函数**：`args` / `env` 当入参，
/// 单测才不用去改进程环境（改环境变量的测试会互相打架）。
///
/// 环境变量按「非空且不是 `0`」算真：脚本传 `RAYBEND_SPIKE=1`，
/// 但人手工写了空值或 `0` 时不该意外弹出一个调试窗口。
fn spike_requested_from(args: &[String], env: Option<&str>) -> bool {
    if args.iter().any(|arg| arg == SPIKE_ARG) {
        return true;
    }
    matches!(env, Some(value) if !value.is_empty() && value != "0")
}

/// [`spike_requested_from`] 的真实入参版本（读进程的 args 与 env）。
fn spike_requested() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let env = std::env::var(SPIKE_ENV).ok();
    spike_requested_from(&args, env.as_deref())
}

/// 前端是否已经报过「界面就绪」（`ui_ready`）。
///
/// 启动兜底线程用它决定**日志摸辞**：报过就绪之后，兜底只是保险，
/// 不该再印「前端仍未报就绪」—— 那句话会把排障的人引向错方向
/// （2026-09-17 我自己就被它误导过一轮：日志里同时出现「主窗口已就绪」与「仍未报就绪」）。
static UI_READY_REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 收尾：显示主窗口（并把焦点交给它）+ 关掉闪屏。
///
/// **幂等**：`ui_ready`（前端首屏就绪）、兜底线程、以及「等满最短停留」的那条路径都会调它，先后不定，
/// 所以每一步都只关心「窗口还在不在」，调两次也不会出错。
/// 窗口不存在时静默跳过：Linux 开发配置会整体替掉 `app.windows`（见 `tauri.linux.conf.json`），
/// 那里本来就没有闪屏窗口。
fn reveal_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = splash.close();
    }
}

/// 「界面好了」—— 由前端在首屏（含字体）就绪后调一次，见 `src/App.tsx`。
///
/// 为什么不让 Rust 自己判断：只有前端知道自己的第一帧什么时候画完
/// （`on_page_load` 在脚本跑完前就发了，那时界面还是空的）。
///
/// **注意它不等于「立刻显示主窗口」**：闪屏至少要露满 `SPLASH_MIN_VISIBLE`
/// （人类 2026-09-17 要求「准备好了也等 3 秒」），这条命令只是把「最早可以显示的时刻」
/// 报上来，真正的收尾由 [`reveal_main_after_splash_min`] 对齐。
#[tauri::command]
fn ui_ready(app: tauri::AppHandle) {
    UI_READY_REPORTED.store(true, std::sync::atomic::Ordering::Relaxed);
    reveal_main_after_splash_min(&app);
}

/// 「界面就绪」的收尾，但**不早于闪屏露满 `SPLASH_MIN_VISIBLE`**。
///
/// 已经等够了（慢机器、或 dev 下压根没闪屏）就直接收尾；否则睡剩下的那点时间再收尾。
/// 多花一个线程是划算的：它把「等够 3 秒」这件事从命令处理里摘出来，前端不必为此阻塞。
fn reveal_main_after_splash_min(app: &tauri::AppHandle) {
    let remaining = splash_remaining();
    if remaining.is_zero() {
        reveal_main(app);
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(remaining);
        eprintln!(
            "[raybend] 主窗口已就绪，等闪屏露满 {} 秒后显示",
            SPLASH_MIN_VISIBLE.as_secs()
        );
        reveal_main(&handle);
    });
}

/// 启动应用。
///
/// # Panics
///
/// Tauri 运行时初始化失败时 panic —— 此时进程已无法提供任何功能。
pub fn run() {
    tauri::Builder::default()
        // 目录选择器（建库弹窗的「浏览…」）。官方插件：Windows 走原生对话框。
        .plugin(tauri_plugin_dialog::init())
        .manage(db::DbState::default())
        .manage(thumbs::SourcesThumbs::default())
        // 浏览过的目录的元信息缓存（会话级内存，不落盘 —— 见 plans/photo-meta-and-tile-display.md）
        .manage(source::SourcesMetaCache::default())
        // ⚠️ **导入批次表必须注册**：漏了它，`app.state::<ImportBatches>()` 一调用就 panic
        // （`state() called before manage()`），而且**编译期不报**。真机踩过：
        // 导入点下去没反应、弹窗空着、连「取消导入」都卡住（命令 panic 后 JS 的 promise
        // 永远不 settle）。下面 `every_state_type_is_managed` 那个测试就是用来防复发的。
        .manage(import::ImportBatches::default())
        // 浏览会话态：撤销栈（每库一份）+ 旗标（跨库，内存）+ 当前打开的库缓存
        .manage(browse::BrowseState::default())
        .invoke_handler(tauri::generate_handler![
            // ── 数据底座的诊断与设置（M1-2）──
            db::app_paths,
            db::db_status,
            db::setting_get,
            db::setting_set,
            // ── 来源：最近目录 / 驱动器 / 目录树 / 照片清单 ──
            source::recent_dirs_list,
            source::recent_dir_remember,
            source::recent_dir_forget,
            source::volumes_list,
            source::dir_list,
            source::source_scan,
            source::source_count,
            source::source_times,
            source::source_paths_status,
            source::dir_meta_ensure,
            source::file_exif,
            // ── 缩略图（未入库的源文件也要能出图）──
            thumbs::thumb_get,
            thumbs::thumb_sources_stats,
            // ── 统一取图口（view 与缩略图共用的总入口，M2-W2）──
            thumbs::view_image,
            thumbs::image_histogram,
            tags::tag_list,
            tags::tag_ensure,
            // ── 库（相片仓）──
            repo::repositories_list,
            repo::repository_probe,
            repo::repository_create,
            repo::repository_remount,
            repo::repository_counts,
            repo::repository_sync_dir,
            repo::repository_rebuild,
            repo::repository_settings,
            repo::repository_set_template,
            repo::repository_template_preview,
            import::import_precheck,
            import::import_start,
            import::import_pause,
            import::import_resume,
            import::import_cancel,
            import::import_status,
            import::import_errors_export,
            import::import_interrupted,
            // ── 浏览（M2-W1：查询 / 标记 / 撤销 / 旗标 / 删除）──
            browse::browse_page,
            browse::browse_timeline,
            browse::browse_facets,
            browse::browse_markings,
            browse::browse_mark,
            browse::browse_undo,
            browse::browse_redo,
            browse::browse_delete,
            browse::flags_get,
            browse::flags_set,
            browse::flags_clear,
            // ── 库内目录树的菜单（新建子目录 / 删除空目录，BROWSE.md §4.3）──
            dirs::dir_empty_check,
            dirs::dir_remove_empty,
            dirs::dir_create,
            // ── 渲染 spike（M2-W1：透明挖洞 + wgpu 直绘的可行性验证）──
            spike_viewport::spike_open,
            spike_viewport::spike_close,
            spike_viewport::spike_command,
            spike_viewport::spike_snapshot,
            spike_viewport::spike_hit_test,
            spike_viewport::spike_write_report,
            // ── 启动流程 ──
            ui_ready,
        ])
        .manage(spike_viewport::SpikeState::default())
        .setup(|app| {
            use tauri::Manager;
            /*
             * 迁移通知的钩子要**赶在第一次开库之前**装上：
             * 下面那行 `db::warm_up` 就可能触发 `app.db` 的升级，
             * 而升级开始时界面得能弹阻塞遮罩（人类 2026-09-19 的要求）。
             */
            migration::install(app.handle());
            // 先把数据底座打开（命令也可以懒打开，这里做一次是为了启动日志能立刻反映问题）。
            // **失败不阻止启动**：窗口该出来还是要出来，错误让前端在需要时再报。
            let state = app.state::<db::DbState>();
            db::warm_up(app.handle(), &state);

            /*
             * 启动闪屏（见 `tauri.conf.json` 的 `splash` 窗口与 `public/splash.html`）。
             *
             * 两个窗口都是 `visible: false` 声明的：主窗口藏到「界面就绪」是刻意的
             * —— 否则闪屏期间能看到它在下面一行一行地渲染。
             *
             * 时序：闪屏至少露 `SPLASH_MIN_VISIBLE`；前端更早就绪也等满它；
             * 前端一直不就绪则走 `SPLASH_TIMEOUT` 兜底（人类 2026-09-17 要求最短 3 秒）。
             *
             * 开发模式下（`pnpm tauri dev`，定义就是 `!cfg!(feature = "custom-protocol")`）：
             * **不弹闪屏，并且立刻显示主窗口** —— 否则每次起 dev 都要白等 3 秒兜底，
             * 而且 `pnpm smoke:ui` 会在 CDP 目标列表里多看到一个窗口。
             */
            if tauri::is_dev() {
                if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
                    let _ = splash.close();
                }
                if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    let _ = main.show();
                }
            } else {
                if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
                    let _ = splash.show();
                    // 「至少露满 3 秒」从**这一刻**算起（不是从 setup 进来说起）。
                    let _ = SPLASH_SHOWN_AT.set(std::time::Instant::now());
                }
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(SPLASH_TIMEOUT);
                    // 只在**真的**没等到前端就绪时才这样印：报过就绪之后这里只是保险
                    // （那时主窗口早该由 `reveal_main_after_splash_min` 显示过了）。
                    if !UI_READY_REPORTED.load(std::sync::atomic::Ordering::Relaxed) {
                        eprintln!(
                            "[raybend] 闪屏已露满 {} 秒但前端仍未报就绪，按兜底显示主窗口",
                            SPLASH_TIMEOUT.as_secs()
                        );
                    }
                    reveal_main(&handle);
                });
            }
            // 渲染 spike 的调试窗口（`plans/M2-W1-windows-gpu.md` 那张清单要用它）。
            // 位置放在闪屏逻辑**之后**：它是调试设施，正常启动路径不该受它影响。
            //
            // **成功也记一行**：这张日志是「窗口到底开没开」的**唯一外部证据** ——
            // `tasklist /v` 只显示进程的**主窗口**标题，看不见第二个窗口（我最初就是靠它
            // 误判成「没开」的）。开不起来也只记日志，不能让主程序起不来。
            if spike_requested() {
                match spike_viewport::open_window(app.handle()) {
                    Ok(_) => eprintln!("[raybend] spike 调试窗口已打开"),
                    Err(error) => eprintln!("[raybend] spike 窗口没能开起来：{error}"),
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}

#[cfg(test)]
mod tests {
    use crate::spike_requested_from;
    use std::collections::HashSet;

    /// 启动标记的判定：命令行参数与环境变量两路都要认，
    /// 而且**不能**把空值 / `0` / 形近参数误判成「要开 spike 窗口」。
    ///
    /// 漏接这根线的后果实测过（2026-09-17）：人类跑了 `pnpm spike:win`，
    /// 起起来的却是**正常主界面**、中间没有镂空 GPU 区，那张 GPU 清单一步都走不下去 ——
    /// 而脚本还在那儿印「已经开了」。
    #[test]
    fn spike_window_opens_only_on_an_explicit_marker() {
        fn args(list: &[&str]) -> Vec<String> {
            list.iter().map(|item| item.to_string()).collect()
        }

        // 两路各自都能单独触发
        assert!(spike_requested_from(&args(&["raybend-desktop.exe", "--spike=1"]), None));
        assert!(spike_requested_from(&args(&["raybend-desktop.exe"]), Some("1")));
        assert!(spike_requested_from(&args(&["raybend-desktop.exe"]), Some("true")));

        // 不给标记就不开
        assert!(!spike_requested_from(&args(&["raybend-desktop.exe"]), None));
        assert!(!spike_requested_from(&args(&["raybend-desktop.exe"]), Some("")));
        assert!(!spike_requested_from(&args(&["raybend-desktop.exe"]), Some("0")));

        // 形近参数不能误判（与 worker 标记那条测试同一个教训）
        assert!(!spike_requested_from(&args(&["raybend-desktop.exe", "--spike"]), None));
        assert!(!spike_requested_from(&args(&["raybend-desktop.exe", "--spike=0"]), None));
        assert!(!spike_requested_from(
            &args(&["raybend-desktop.exe", "--raybend-raw-worker"]),
            None
        ));
    }

    /// 本 crate 里所有可能提到状态类型的源文件（新增文件时补进来）。
    const SOURCES: &[(&str, &str)] = &[
        ("lib.rs", include_str!("lib.rs")),
        ("db.rs", include_str!("db.rs")),
        ("import.rs", include_str!("import.rs")),
        ("repo.rs", include_str!("repo.rs")),
        ("source.rs", include_str!("source.rs")),
        ("thumbs.rs", include_str!("thumbs.rs")),
    ];

    /// 把 `...::Foo::default()` / `...::Foo` 收敛成**类型名** `Foo`。
    ///
    /// 关键是要把方法调用那一段（`default()`）丢掉 —— 它长得像类型但其实是调用。
    fn type_name(path: &str) -> String {
        let parts: Vec<&str> = path
            .split("::")
            .map(str::trim)
            .filter(|part| !part.is_empty() && !part.contains('('))
            .collect();
        parts.last().copied().unwrap_or_default().to_string()
    }

    /// 取出 `prefix` 之后**括号配平**的那一段（够用即可，不做完整解析）。
    ///
    /// 必须配平：`.manage(db::DbState::default())` 里第一个 `)` 是 `default()` 的，
    /// 按「找第一个右括号」切会把类型名切成 `db::DbState::default(`（第一版就是这么错的）。
    fn balanced_after<'a>(text: &'a str, prefix: &str, open: char, close: char) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(index) = rest.find(prefix) {
            let tail = &rest[index + prefix.len()..];
            let mut depth = 1usize; // prefix 自带一个 open
            let mut end = None;
            for (offset, ch) in tail.char_indices() {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(offset);
                        break;
                    }
                }
            }
            match end {
                Some(offset) => {
                    out.push(&tail[..offset]);
                    rest = &tail[offset..];
                }
                None => break,
            }
        }
        out
    }

    /// 闪屏「至少露满 3 秒」的边界：没露过 / 刚露 / 露了一半 / 正好够 / 超了很久。
    ///
    /// 纯计算（时钟当入参），所以不靠 `sleep` 也不会有 3 秒的测试耗时。
    #[test]
    fn splash_min_visible_boundaries() {
        use std::time::{Duration, Instant};

        let shown = Instant::now();
        let remaining = super::remaining_until_min_visible;

        // 没显示过闪屏（dev）⇒ 一点都不等，主窗口立刻显示
        assert_eq!(remaining(None, shown), Duration::ZERO);
        // 刚露出来 ⇒ 还得等满最短停留
        assert_eq!(remaining(Some(shown), shown), super::SPLASH_MIN_VISIBLE);
        // 露了 1 秒 ⇒ 还差 2 秒
        assert_eq!(
            remaining(Some(shown), shown + Duration::from_secs(1)),
            Duration::from_secs(2)
        );
        // 正好够 ⇒ 0（不是负数，也不 panic）
        assert_eq!(
            remaining(Some(shown), shown + super::SPLASH_MIN_VISIBLE),
            Duration::ZERO
        );
        // 超了很久（慢机器 / 兜底已经放行）⇒ 0
        assert_eq!(
            remaining(Some(shown), shown + Duration::from_secs(86_400)),
            Duration::ZERO
        );
    }

    /// 兜底必须**晚于**最短停留：否则它会抢在「等满 3 秒」前面把闪屏关掉，
    /// 那条人类要求就成了摆设。
    #[test]
    fn splash_fallback_is_later_than_min_visible() {
        assert!(super::SPLASH_TIMEOUT > super::SPLASH_MIN_VISIBLE);
    }

    /// 每条命令用 `app.state::<T>()` 取的状态，**都必须在 builder 里 manage 过**。
    ///
    /// 这一条看着像废话，但漏注册的后果是「运行期 panic + 前端卡死」而不是编译错误，
    /// 且只在真机点那条命令时才炸（2026-09-16：`ImportBatches` 漏了，整个导入挂掉）。
    #[test]
    fn every_state_type_is_managed() {
        let mut managed: HashSet<String> = HashSet::new();
        let mut used: Vec<(String, String)> = Vec::new();

        for (file, text) in SOURCES {
            for path in balanced_after(text, ".manage(", '(', ')') {
                managed.insert(type_name(path));
            }
            for path in balanced_after(text, "state::<", '<', '>') {
                used.push(((*file).to_string(), type_name(path)));
            }
        }

        // `state::<T>()` 里的 T 也可能是泛型参数（`app.state::<Self>()` 之类）——
        // 这里只要求「要么 manage 过，要么是明显的非状态类型（全大写或含泛型）」
        let missing: Vec<String> = used
            .iter()
            // 单字母是泛型占位符（`state::<T>`），不是状态类型
            .filter(|(_, name)| name.len() > 1 && !managed.contains(name))
            .map(|(file, name)| format!("{file}: {name}"))
            .collect();

        assert!(
            missing.is_empty(),
            "这些类型被 app.state::<T>() 取用，但没在 builder 里 .manage() 注册：\n{}",
            missing.join("\n")
        );
    }
}
