//! 诊断：对着**真实的** `app.db` 跑一遍「库列表」的后端部分（`repositories_list` 干的活）。
//!
//! 存在的理由：人类报「进浏览看不到库」时，前端只能显示一句错误 ——
//! 到底是后端读不出来、还是读得很慢、还是前端渲染炸了，在 GUI 里分不清。
//! 这个例子把后端那一半单独跑出来，顺带把每一步的耗时打出来。
//!
//! 用法（参数是**数据目录**，不是 app.db 文件本身 —— 与 `AppDb::open` 的口径一致）：
//! ```text
//! cargo run -p raybend --example repo-views -- <数据目录>
//! ```
//! 想验「库在线」那条分支（会真去数照片），得让 `repository_paths.path` 在本机解析得动 ——
//! 直接对着 Windows 的 `C:\...` 路径跑会走离线分支。做法：拷一份 app.db 到本机，
//! 把路径改成本机挂载点（例如 `/mnt/c/src/tmp/testrespos`），再跑这个例子。

use std::path::{Path, PathBuf};
use std::time::Instant;

use raybend::store::db::AppDb;
use raybend::store::repository::{self, RepositoryView};
use raybend::store::{assets, pool, time};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("用法: repo-views <数据目录>（里面应当有 app.db）")?;

    let opened = Instant::now();
    let db = AppDb::open(Path::new(&path), time::now_millis())?;
    println!("打开数据目录：{:?}", opened.elapsed());

    let built = Instant::now();
    let views = db.read(|conn| repository::build_views(conn, count_photos_in))?;
    println!(
        "build_views：{} 个库，耗时 {:?}",
        views.len(),
        built.elapsed()
    );

    for v in &views {
        print_view(v);
    }

    // 单独再数一遍，明确圈出「数照片」这一步的成本（它是唯一会碰别的文件的步骤）。
    for v in &views {
        let Some(root) = v.root.as_deref() else {
            continue;
        };
        let t = Instant::now();
        let counted = count_photos_in(Path::new(root));
        println!(
            "  `{}`：单独数一次 = {:?}，结果 {:?}",
            v.name,
            t.elapsed(),
            counted
        );
    }
    Ok(())
}

fn print_view(v: &RepositoryView) {
    println!(
        "  · {name} | id={id} | online={online} | root={root:?} | photos={count:?} | tried={tried} | paths={paths:?}",
        name = v.name,
        id = v.id,
        online = v.online,
        root = v.root,
        count = v.photo_count,
        tried = v.tried_paths,
        paths = v.paths.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
    );
}

/// 与 `src-tauri/src/repo.rs` 里的同名函数保持一致（那里是唯一事实来源，改动要同步）。
fn count_photos_in(root: &Path) -> Option<i64> {
    let catalog: PathBuf = root.join(repository::CATALOG_FILE_NAME);
    let pool = pool::ReadPool::open(catalog).ok()?;
    let (assets, _files) = pool.with(assets::counts).ok()?;
    Some(assets)
}
