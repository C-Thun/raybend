//! XMP 巡检 CLI（`specs/xmp-audit-tool.md`）：`pnpm check:xmp "<库根>"`。
//!
//! **只读工具**：不写、不删、不改盘上任何文件。
//! 库必须**已经是当前 schema 版本**（旧库拒绝打开，免得巡检顺手把库迁了 ——
//! 先用应用打开一次完成迁移，再巡检）。
//!
//! 退出码：`0` = 没有 FAIL；`1` = 有 FAIL；`2` = 用法 / 打开失败。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::migration::{DbKind, schema_version, supported_version};
use raybend::store::time;
use raybend::xmp::audit::{Severity, audit};

const USAGE: &str = "用法：pnpm check:xmp \"<库根>\"（catalog.db 所在目录；库须为当前版本）";

fn main() -> ExitCode {
    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if !root.is_dir() {
        eprintln!("不是目录：{}", root.display());
        return ExitCode::from(2);
    }
    if !root.join("catalog.db").is_file() {
        eprintln!("目录下没有 catalog.db：{}", root.display());
        return ExitCode::from(2);
    }

    // 版本闸门：只巡检与当前程序同版本的库（`CatalogDb::open` 会迁移旧库）。
    match catalog_version(&root.join("catalog.db")) {
        Ok(version) if version == supported_version(DbKind::Catalog) => {}
        Ok(version) => {
            eprintln!(
                "库 schema 版本 {version}，本工具支持 {}；请先用应用打开这个库（完成迁移）再巡检。",
                supported_version(DbKind::Catalog)
            );
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("读不到库版本：{error}");
            return ExitCode::from(2);
        }
    }

    let catalog = match CatalogDb::open(&root, OpenOpts::unbacked_up(time::now_millis())) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("打不开库 {}：{error}", root.display());
            return ExitCode::from(2);
        }
    };
    let report = match catalog.read(|conn| audit(conn, catalog.root())) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("巡检失败：{error}");
            let _ = catalog.retire();
            return ExitCode::from(2);
        }
    };
    print!("{}", report.render(catalog.root()));
    let _ = catalog.retire();

    let failures = report.count(Severity::Fail);
    if failures > 0 {
        eprintln!("发现 {failures} 处盘上与库不一致（见上面的 FAIL 行）。");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// 只读地读 `user_version` —— 打不开或不是 SQLite 都返回错误字符串。
fn catalog_version(path: &Path) -> Result<i64, String> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
    schema_version(&conn).map_err(|error| error.to_string())
}
