//! **显示尺寸**（display size）：一张照片「现在显示的到底是多大」的**唯一**解析口。
//!
//! 出处：`specs/display-size-source.md` §4。以前这件事散在三个互不同源的口径里
//! （catalog 的基础尺寸 / 变体的全尺寸渲染 / 渲染出来的字节），本模块把它收敛成一个：
//!
//! | 概念 | 定义 | 存哪 |
//! | --- | --- | --- |
//! | **原始轴尺寸** | 文件头里的 width/height（**未**应用方向） | `assets.width/height`（语义不变） |
//! | **方向** | EXIF 1..8 | `assets.orientation`、`asset_files.orientation` |
//! | **展示尺寸** | `oriented_size(原始轴, 方向)` = 屏上「宽 × 高」 | `asset_files.display_width/height` |
//! | **输出尺寸** | `EditGeometry::output_size(源展示尺寸)`（已含方向与几何） | `develop_stacks.output_*`、`issues.output_*` |
//!
//! 铁律：
//!
//! 1. 方向只在 [`crate::media::meta::oriented_size`] 里算，几何只在
//!    [`EditGeometry::output_size`] 里算 —— 本模块只**选择**与**回退**；
//! 2. 本模块的查询**只读库**：不读文件、不渲染。老库未回填时按下面的矩阵回退
//!    （最坏退到 NULL，消费方走旧行为），从不现场读盘 —— 现场读盘只发生在
//!    [`ensure_file_sizes`] 这一条显式入口上。
//!
//! 回退的两种性质要分清（2026-10-11 崔总真机反馈后补的账）：
//!
//! * **NULL 列**（v19 之前建的栈 / 定稿，列还没回填）→ 按下面的矩阵**现算**：
//!   latest 用当前栈、定稿用它自己的配置，源尺寸取库里的值 —— 零 I/O，
//!   所以「老库还没回填」不会让界面退回基准尺寸（那是**另一个口径**，不是未知）；
//! * **源尺寸也未知**（`asset_files.display_*` 与 `assets.width/height` 都没有）
//!   → 才是 NULL，消费方走旧行为。
//!
//! ## 判定矩阵（与单测一一对应）
//!
//! | 变体 | 用谁 | 缺失回退 |
//! | --- | --- | --- |
//! | [`SizeVariant::Sooc`] | bitmap 行的 `display_*` | raw 行 → `assets` 展示尺寸 → NULL |
//! | [`SizeVariant::Raw`] | raw 行的 `display_*` | bitmap 行 → `assets` 展示尺寸 → NULL |
//! | [`SizeVariant::Latest`] | `develop_stacks.output_*` | 无栈行 → 基准展示尺寸；有行但 NULL → 现算 → NULL |
//! | [`SizeVariant::Issue`] | `issues.output_*` | 现算 `geometry.output_size(源展示尺寸)` → NULL |

use rusqlite::{Connection, OptionalExtension, params};

use crate::develop::geometry::EditGeometry;
use crate::error::{Error, Result};
use crate::media::meta::{PhotoMeta, normalize_orientation, oriented_size, read_photo_meta};
use crate::store::db::CatalogDb;
use crate::store::develop::{DevelopStack, EditBase, load as load_stack};

/// 显示变体（与 `issues::Selection` 同形；这里只关心「尺寸按哪个变体算」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeVariant {
    Sooc,
    Raw,
    Latest,
    Issue(i64),
}

impl SizeVariant {
    /// 解析显示变体键：`sooc` / `raw` / `latest` / `issue:N`（认不出给 `None`）。
    ///
    /// 这是字符串口径的**唯一**解析处（导出快照与 `export_variant_details` 共用）。
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "sooc" => Some(Self::Sooc),
            "raw" => Some(Self::Raw),
            "latest" => Some(Self::Latest),
            _ => key
                .strip_prefix("issue:")
                .and_then(|id| id.parse::<i64>().ok())
                .filter(|id| *id > 0)
                .map(Self::Issue),
        }
    }
}

/// 该变体的**展示尺寸**；库里没有就 `None`（调用方按旧行为回退，不报错、不读盘）。
///
/// # Errors
/// 数据库读失败（损坏的定稿 JSON 也算 —— 那是库坏了，不是「尺寸未知」）。
pub fn display_size(
    conn: &Connection,
    asset_id: i64,
    variant: SizeVariant,
) -> Result<Option<(u32, u32)>> {
    match variant {
        SizeVariant::Sooc => preferred_size(conn, asset_id, "bitmap"),
        SizeVariant::Raw => preferred_size(conn, asset_id, "raw"),
        SizeVariant::Latest => latest_size(conn, asset_id),
        SizeVariant::Issue(issue_id) => issue_size(conn, asset_id, issue_id),
    }
}

/// 某个源的展示尺寸 = 该 role 的文件行 → 基准展示尺寸（= 矩阵里的回退链）。
pub fn source_size(
    conn: &Connection,
    asset_id: i64,
    base: EditBase,
) -> Result<Option<(u32, u32)>> {
    preferred_size(
        conn,
        asset_id,
        match base {
            EditBase::Sooc => "bitmap",
            EditBase::Raw => "raw",
        },
    )
}

/// **编辑栈的输出尺寸**（latest 与定稿共用）：`geometry.output_size(源展示尺寸)`。
///
/// 源展示尺寸未知 → `None`（写侧就写 NULL，读侧走回退链）。**零渲染、零读盘。**
///
/// # Errors
/// 数据库读失败。
pub fn stack_output_size(
    conn: &Connection,
    asset_id: i64,
    stack: &DevelopStack,
) -> Result<Option<(u32, u32)>> {
    let Some(source) = source_size(conn, asset_id, stack.source_base)? else {
        return Ok(None);
    };
    Ok(Some(output_of(stack.geometry, source)))
}

/// 套一层几何：`None` = 原图（输出尺寸就是源展示尺寸）。
#[must_use]
pub fn output_of(geometry: Option<EditGeometry>, source: (u32, u32)) -> (u32, u32) {
    geometry.map_or(source, |geometry| geometry.output_size(source))
}

/// 一个**文件行**的展示尺寸（按库内相对路径查，大小写折叠后匹配）。
///
/// 浏览网格要按每张照片的真实比例摆位：RAW 的比例**只能来自 record**（rawler 出图尺寸，
/// 崔总 2026-10-11：「对 RAW 来说不能用私自解算像素数来替代」）—— 记录还没建好就返回
/// `None`，界面按默认占位比例显示，等懒补落地自然变准。
///
/// # Errors
/// 数据库读失败。
pub fn file_display_size(
    conn: &Connection,
    rel_path: &str,
) -> Result<Option<(u32, u32, u16)>> {
    let folded = crate::store::path_semantics::PathForms::new(rel_path)
        .folded()
        .to_string();
    let row: Option<(Option<i64>, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT display_width, display_height, orientation FROM asset_files
              WHERE rel_path_folded = ?1",
            [folded],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((width, height, orientation)) = row else {
        return Ok(None);
    };
    let Some((width, height)) = stored_edges(width, height) else {
        return Ok(None);
    };
    Ok(Some((
        width,
        height,
        orientation.map_or(1, |value| normalize_orientation(Some(value))),
    )))
}

/// 把这张照片的**所有尺寸记录**补成现成的（缺什么补什么）—— 读侧碰到 NULL 的保底入口。
///
/// 顺序：文件行（`ensure_file_sizes`）→ latest 输出（`develop::backfill_output_sizes`）
/// → 各定稿输出（`issues::backfill_output_sizes`）。三步都幂等、**只写 NULL**、
/// 算不出就留 NULL 等下次（`specs/display-size-lifecycle.md` §2「现算 → 写回」）。
///
/// 返回这次真写了多少行。**别在读连接里裸调** —— 它开写事务（单一写者）。
///
/// # Errors
/// 数据库读写失败。
pub fn ensure_sizes(catalog: &CatalogDb, asset_id: i64) -> Result<usize> {
    let root = catalog.root().to_path_buf();
    catalog.write(move |conn| ensure_sizes_conn(conn, &root, asset_id))
}

/// [`ensure_sizes`] 的连接版本（已经在一笔写事务里时用，例如 `export_snapshots`）。
///
/// # Errors
/// 数据库读写失败。
pub fn ensure_sizes_conn(conn: &Connection, root: &std::path::Path, asset_id: i64) -> Result<usize> {
    let mut written = ensure_file_sizes_conn(conn, root, asset_id)?;
    written += crate::store::develop::backfill_output_sizes(conn, asset_id)?;
    written += crate::store::issues::backfill_output_sizes(conn, asset_id)?;
    Ok(written)
}

/// 老的（v19 之前导入的）文件行补上方向与展示尺寸 —— **唯一**会读文件头的入口。
///
/// 幂等、可重入、失败只当「没读到」：已有值不动，读不出尺寸的行保持 NULL。
/// 调用方拿它当「顺手补一下」，不该等它。
///
/// 返回这次真写了多少行。
///
/// # Errors
/// 数据库读写失败（文件读不了**不算**错误）。
pub fn ensure_file_sizes(catalog: &CatalogDb, asset_id: i64) -> Result<usize> {
    let root = catalog.root().to_path_buf();
    catalog.write(move |conn| ensure_file_sizes_conn(conn, &root, asset_id))
}

/// [`ensure_file_sizes`] 的连接版本（写事务内用；文件 I/O 在事务里，所以只适合
/// 一两张照片的顺手补 —— 批量走 [`crate::store::backfill`]）。`root` = 库根。
///
/// # Errors
/// 数据库读写失败（文件读不了**不算**错误）。
pub fn ensure_file_sizes_conn(
    conn: &Connection,
    root: &std::path::Path,
    asset_id: i64,
) -> Result<usize> {
    let missing: Vec<(i64, String)> = {
        let mut statement = conn.prepare(
            "SELECT id, rel_path FROM asset_files
              WHERE asset_id = ?1 AND missing_since IS NULL
                AND (display_width IS NULL OR display_height IS NULL)
              ORDER BY id",
        )?;
        statement
            .query_map([asset_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if missing.is_empty() {
        return Ok(0);
    }

    let mut updates: Vec<(i64, u32, u32, u16)> = Vec::new();
    for (row_id, rel_path) in missing {
        let absolute = root.join(&rel_path);
        if !absolute.is_file() {
            continue;
        }
        if let Ok(meta) = read_photo_meta(&absolute)
            && meta.has_size()
        {
            let meta = apply_display_size_source(&absolute, meta);
            updates.push((row_id, meta.width, meta.height, meta.orientation));
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }

    let mut written = 0usize;
    for (row_id, width, height, orientation) in updates {
        written += conn.execute(
            "UPDATE asset_files
                SET display_width = ?2, display_height = ?3, orientation = ?4, updated_at = ?5
              WHERE id = ?1 AND display_width IS NULL AND display_height IS NULL",
            params![
                row_id,
                i64::from(width),
                i64::from(height),
                i64::from(orientation),
                crate::store::time::now_millis()
            ],
        )?;
    }
    Ok(written)
}

// ─────────────────────────── 内部：三条回退链 ───────────────────────────

/// 把一个目录里刚读到的**展示尺寸**写回库里对应的文件行（老库懒回填）。
///
/// `dir` 是**绝对路径**的目录，`files` 是 `(目录内相对名, 宽, 高, 方向)`。
/// 只补空值（两列都 NULL 的行），已经有值的不动；`dir` 不在库根里（导入来源目录）
/// 或没有任何一行命中时自然什么都不写。返回真写了多少行。
///
/// # Errors
/// 数据库读写失败。
pub fn backfill_dir_display_sizes(
    catalog: &CatalogDb,
    dir: &std::path::Path,
    files: &[(String, u32, u32, u16)],
) -> Result<usize> {
    let root = catalog.root().to_path_buf();
    let Ok(relative) = dir.strip_prefix(&root) else {
        return Ok(0);
    };
    let dir_rel = relative.to_string_lossy().replace('\\', "/");
    let mut updates: Vec<(String, u32, u32, u16)> = Vec::new();
    for (name, width, height, orientation) in files {
        if *width == 0 || *height == 0 {
            continue;
        }
        /*
         * RAW **不许**用我们私自解的文件头数顶（崔总 2026-10-11）：
         * 「图片尺寸以 rawler 回传为主……对 RAW 来说不能用私自解算像素数来替代正式
         * develop 输出的真实位图像素数」。文件头那份是整帧，比出图大一圈。
         * 所以这里只写位图行；RAW 行由导入登记 / `ensure_file_sizes`（rawler 出图尺寸）
         * 与批量回填负责。
         */
        if crate::media::kind::kind_of_file(name) == crate::media::kind::MediaKind::Raw {
            continue;
        }
        let rel = if dir_rel.is_empty() {
            name.clone()
        } else {
            format!("{dir_rel}/{name}")
        };
        updates.push((
            crate::store::path_semantics::PathForms::new(&rel)
                .folded()
                .to_string(),
            *width,
            *height,
            *orientation,
        ));
    }
    if updates.is_empty() {
        return Ok(0);
    }
    catalog.write(move |conn| {
        let mut written = 0usize;
        for (folded, width, height, orientation) in updates {
            written += conn.execute(
                "UPDATE asset_files
                    SET display_width = ?2, display_height = ?3, orientation = ?4, updated_at = ?5
                  WHERE rel_path_folded = ?1
                    AND display_width IS NULL AND display_height IS NULL",
                params![
                    folded,
                    i64::from(width),
                    i64::from(height),
                    i64::from(orientation),
                    crate::store::time::now_millis()
                ],
            )?;
        }
        Ok(written)
    })
}

/// 「该文件自己的展示尺寸 → 基准展示尺寸」。
/// RAW 的**出图尺寸**（原始像素轴，与文件头尺寸同一坐标系）；非 RAW 或读不到 → `None`。
///
/// 只认 rawler 显影管线的输出（`RawlerBackend::output_size`）—— 崔总 2026-10-11：
/// 「对外展示的分辨率」必须等于出图尺寸，出多少像素就报多少。文件头那份是**整帧**，
/// 会比出图大一圈（RW2 实测 5264×3904 vs 出图 5184×3888）。
#[must_use]
pub fn raw_output_size(path: &std::path::Path) -> Option<(u32, u32)> {
    if crate::media::kind::kind_of_file(&path.to_string_lossy()) != crate::media::kind::MediaKind::Raw
    {
        return None;
    }
    crate::raw::RawlerBackend::output_size(path).ok().flatten()
}

/// **展示尺寸的唯一口径**：RAW 换成出图尺寸（原始轴 → 按方向转成展示轴），其余原样。
///
/// 导入登记、懒回填（`ensure_file_sizes_conn`）都走它；批量回填因为要写**原始轴**
/// 的中间结构，直接调 [`raw_output_size`]。别在别处再写第三份。
#[must_use]
pub fn apply_display_size_source(path: &std::path::Path, meta: PhotoMeta) -> PhotoMeta {
    let Some((width, height)) = raw_output_size(path) else {
        return meta;
    };
    let (width, height) = oriented_size(width, height, meta.orientation);
    PhotoMeta { width, height, ..meta }
}

fn preferred_size(conn: &Connection, asset_id: i64, role: &str) -> Result<Option<(u32, u32)>> {
    if let Some(size) = file_size(conn, asset_id, role)? {
        return Ok(Some(size));
    }
    /*
     * 这个角色自己的文件行没值（老库没回填）时的退路 —— **只能退到「同一个变体」的来源**：
     *
     * | 角色 | 退路 | 为什么 |
     * | --- | --- | --- |
     * | `bitmap` | `assets` 的原始轴 + 方向 | 导入时那张**主文件**就是位图，与位图行同源 |
     * | `raw` | 只有**没有位图行**时才是 `assets`（RAW-only 库）；否则 `None` | 资产那行记的是主文件（JPEG+RAW 时就是位图）—— 跨过来就是真机报的「raw 缩进了 sooc 的尺寸里」 |
     *
     * 2026-10-11 崔总真机（MYP1000037：位图 3712×2784 / RAW 5264×3904）：
     * RAW 行没回填时，两个方向都会拿对方的数字 —— sooc 按 5264 放大、raw 按 3712 缩小，
     * 同一个变体的尺寸还会在「懒回填跑前 / 跑后」之间跳。**宁可 `None`（未知）**：
     * 调用方（`export_snapshots`）会先把文件行补上（读一次文件头）再取尺寸，
     * 前端把 `None` 当「还没拿到」而不是「用别的变体凑」。
     */
    match role {
        "bitmap" => {
            if let Some(size) = asset_size(conn, asset_id)? {
                return Ok(Some(size));
            }
            // 没有位图行（RAW-only 资产上的 sooc 变体 / `Sooc` 基底）：那就只有 RAW 可问
            if has_file_role(conn, asset_id, "bitmap")? {
                return Ok(None);
            }
            file_size(conn, asset_id, "raw")
        }
        // RAW 那一份**只认 rawler 回传的出图尺寸**（崔总 2026-10-11：「不能用私自解算
        // 像素数来替代正式 develop 输出的真实位图像素数」）。行上没值就报**未知**：
        // 不许退到 `assets`（那是文件头/EXIF 的口径，整帧，比出图大一圈），
        // 等导入登记或保底回填把行补上，界面按「还没拿到」处理。
        "raw" => Ok(None),
        _ => Ok(None),
    }
}

/// 这个资产有没有某个角色的文件行（有没有行 ≠ 行上有没有尺寸）。
fn has_file_role(conn: &Connection, asset_id: i64, role: &str) -> Result<bool> {
    let row: Option<i64> = conn
        .query_row(
            "SELECT id FROM asset_files WHERE asset_id = ?1 AND role = ?2 ORDER BY id LIMIT 1",
            params![asset_id, role],
            |row| row.get(0),
        )
        .optional()?;
    Ok(row.is_some())
}

/// 一个 `asset_files` 行的展示尺寸（两列都在才算数）。
fn file_size(conn: &Connection, asset_id: i64, role: &str) -> Result<Option<(u32, u32)>> {
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT display_width, display_height FROM asset_files
              WHERE asset_id = ?1 AND role = ?2
                AND display_width IS NOT NULL AND display_height IS NOT NULL
              ORDER BY id LIMIT 1",
            params![asset_id, role],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(width, height)| edges(width, height)))
}

/// 基准展示尺寸：位图行 → `assets` 的原始轴 + 方向现算 → RAW 行。
///
/// 「资产自己的尺寸」排在 RAW 行**之前**是有意的（理由见 [`preferred_size`]）：
/// 没有位图行时，先问资产自己记的主文件尺寸，别拿另一个变体的数字。
/// 归一化只在这一处与 `browse.rs` 的 DTO 边界做。
fn base_size(conn: &Connection, asset_id: i64) -> Result<Option<(u32, u32)>> {
    if let Some(size) = file_size(conn, asset_id, "bitmap")? {
        return Ok(Some(size));
    }
    if let Some(size) = asset_size(conn, asset_id)? {
        return Ok(Some(size));
    }
    file_size(conn, asset_id, "raw")
}

/// `assets` 行的原始轴 + 方向归一（给**从未回填的老库**用的那一份）。
fn asset_size(conn: &Connection, asset_id: i64) -> Result<Option<(u32, u32)>> {
    let row: Option<(Option<i64>, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT width, height, orientation FROM assets WHERE id = ?1",
            [asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(row.and_then(|(width, height, orientation)| {
        let width = width.and_then(|value| u32::try_from(value).ok())?;
        let height = height.and_then(|value| u32::try_from(value).ok())?;
        if width == 0 || height == 0 {
            return None;
        }
        Some(oriented_size(
            width,
            height,
            normalize_orientation(orientation),
        ))
    }))
}

/// latest 的输出尺寸：**库里存的值优先**，NULL（老库还没回填）就按当前栈现算。
///
/// 这是「读侧的唯一回退」：`AssetItem.latestWidth/Height` 与 `sizes::display_size(Latest)`
/// 都走它 —— 于是老库里编辑过的照片不会再退回**基准尺寸**（那是另一个口径：
/// 未裁切的原始画面，比例可以完全不同；2026-10-11 崔总真机反馈的 4:3 拉伸）。
/// 现算零 I/O：源尺寸取库里的展示尺寸，库里没有才退到 `assets` 原始轴 + 方向。
///
/// # Errors
/// 数据库读失败。
pub fn latest_size(conn: &Connection, asset_id: i64) -> Result<Option<(u32, u32)>> {
    let stored: Option<(Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT output_width, output_height FROM develop_stacks WHERE asset_id = ?1",
            [asset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match stored {
        // 没有编辑栈行 = 没编辑过：基准展示尺寸
        None => base_size(conn, asset_id),
        // 存值优先：latest 是 editor **每次保存随笔写的现成值**（崔总 2026-10-11：
        // 「latest 是 editor 里随时调随时写……browse 时数据库里绝对是现成的」）——读侧不现算。
        Some((width, height)) => match stored_edges(width, height) {
            Some(size) => Ok(Some(size)),
            // 缺值（老库 / 源尺寸当时未知）→ 现算保底；**写回**由 `ensure_sizes` 负责
            None => stack_output_size(conn, asset_id, &load_stack(conn, asset_id)?),
        },
    }
}

/// 一条定稿的输出尺寸：**库里存的值优先**，NULL（老库还没回填）就按该定稿的配置现算。
///
/// # Errors
/// 数据库读失败，或该定稿的配置损坏 / 编辑源无效。
pub fn issue_size(conn: &Connection, asset_id: i64, issue_id: i64) -> Result<Option<(u32, u32)>> {
    let row: Option<(Option<i64>, Option<i64>, String, String)> = conn
        .query_row(
            "SELECT output_width, output_height, profile_json, source_base
               FROM issues WHERE asset_id = ?1 AND id = ?2",
            params![asset_id, issue_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((width, height, json, base)) = row else {
        return Ok(None);
    };
    if let Some(size) = stored_edges(width, height) {
        return Ok(Some(size));
    }
    let stack: DevelopStack = serde_json::from_str(&json)
        .map_err(|error| Error::Unsupported(format!("定稿 {issue_id} 的配置损坏：{error}")))?;
    let base = EditBase::parse(&base)
        .ok_or_else(|| Error::Unsupported(format!("定稿 {issue_id} 的编辑源无效")))?;
    issue_output_size(conn, asset_id, base, &stack)
}

/// 已解析好的定稿行 → 输出尺寸（`issues::list` 手上有栈，不必为尺寸再读一遍行）。
///
/// 「源展示尺寸 → 几何」的算法只有这一处；`issue_size` 与写侧回填都走它。
///
/// # Errors
/// 数据库读失败。
pub fn issue_output_size(
    conn: &Connection,
    asset_id: i64,
    base: EditBase,
    stack: &DevelopStack,
) -> Result<Option<(u32, u32)>> {
    let Some(source) = preferred_size(
        conn,
        asset_id,
        match base {
            EditBase::Sooc => "bitmap",
            EditBase::Raw => "raw",
        },
    )?
    else {
        return Ok(None);
    };
    Ok(Some(output_of(stack.geometry, source)))
}

/// `(宽, 高)` 两列都为正整数且落在 u32 里才算「已知」。
fn edges(width: i64, height: i64) -> Option<(u32, u32)> {
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}

fn stored_edges(width: Option<i64>, height: Option<i64>) -> Option<(u32, u32)> {
    edges(width?, height?)
}

#[cfg(test)]
mod tests {
    use super::{
        SizeVariant, display_size, ensure_file_sizes, read_photo_meta, stack_output_size, PhotoMeta,
    };
    use crate::develop::geometry::{CropRect, EditGeometry};
    use crate::store::db::{CatalogDb, OpenOpts};
    use crate::store::develop::{DevelopStack, EditBase};
    use crate::store::issues;
    use crate::store::migration::{self, Backups, DbKind};
    use rusqlite::Connection;

    const T0: i64 = 1_789_516_800_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, Backups::none(), T0).unwrap();
        conn
    }

    /// 造一个资产 + 若干文件行；`(role, display_width, display_height, orientation)`。
    fn asset(conn: &Connection, id: i64, files: &[(&str, Option<i64>, Option<i64>, Option<i64>)]) {
        conn.execute(
            "INSERT INTO assets(id, taken_at, imported_at, updated_at) VALUES(?1, ?2, ?2, ?2)",
            rusqlite::params![id, T0],
        )
        .unwrap();
        for (index, (role, width, height, orientation)) in files.iter().enumerate() {
            let rel = format!("photos/{id}-{index}-{role}.bin");
            conn.execute(
                "INSERT INTO asset_files
                    (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
                     display_width, display_height, orientation, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'bin', 1, 1, ?4, ?5, ?6, 1, 1)",
                rusqlite::params![id, role, rel, width, height, orientation],
            )
            .unwrap();
        }
    }

    fn stack(conn: &Connection, id: i64, base: EditBase, geometry: Option<EditGeometry>) {
        let mut draft = DevelopStack {
            source_base: base,
            geometry,
            ..DevelopStack::default()
        };
        // 借 `save` 的入口写（写入口唯一；这里顺带验证尺寸列是同事务写的）
        draft.params.insert("exposure".into(), 0.5);
        draft.validate().unwrap();
        crate::store::develop::save(conn, id, &draft, T0).unwrap();
    }

    #[test]
    fn sooc_and_raw_fall_back_to_the_other_file_then_to_assets() {
        let conn = catalog();
        // 位图 4000×6000（竖拍，方向 6 的原文件），RAW 5208×3904（无方向）
        asset(
            &conn,
            1,
            &[
                ("bitmap", Some(4000), Some(6000), Some(6)),
                ("raw", Some(5208), Some(3904), Some(1)),
            ],
        );
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Sooc).unwrap(),
            Some((4000, 6000))
        );
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Raw).unwrap(),
            Some((5208, 3904))
        );

        // 老库：三列全 NULL → 退回 `assets` 的原始轴 + 方向现算
        asset(&conn, 2, &[("bitmap", None, None, None)]);
        conn.execute(
            "UPDATE assets SET width = 6000, height = 4000, orientation = 8 WHERE id = 2",
            [],
        )
        .unwrap();
        assert_eq!(
            display_size(&conn, 2, SizeVariant::Sooc).unwrap(),
            Some((4000, 6000)),
            "老库要按方向归一，不能直接给原始轴"
        );
        assert_eq!(
            display_size(&conn, 2, SizeVariant::Raw).unwrap(),
            None,
            "RAW 行没有、位图行也没回填时**不许**拿位图/基准的尺寸充数（崔总 2026-10-11：“raw 缩进了 sooc 里”）"
        );

        // 连 `assets` 都没有尺寸 → NULL（消费方走旧行为，不报错）
        asset(&conn, 3, &[("bitmap", None, None, None)]);
        assert_eq!(display_size(&conn, 3, SizeVariant::Sooc).unwrap(), None);

        // 只有 RAW 的文件：SOOC 变体退回 RAW 的展示尺寸
        asset(&conn, 4, &[("raw", Some(5208), Some(3904), Some(1))]);
        assert_eq!(
            display_size(&conn, 4, SizeVariant::Sooc).unwrap(),
            Some((5208, 3904))
        );

        // RAW-only 且 RAW 行没值：**未知**，不许拿 `assets` 的 EXIF/文件头尺寸顶上
        //（崔总 2026-10-11：「RAW 不能用私自解算像素数替代 rawler 出图的真实像素数」）
        asset(&conn, 5, &[("raw", None, None, None)]);
        conn.execute(
            "UPDATE assets SET width = 5264, height = 3904, orientation = 1 WHERE id = 5",
            [],
        )
        .unwrap();
        assert_eq!(
            display_size(&conn, 5, SizeVariant::Raw).unwrap(),
            None,
            "RAW 行没值就是未知，等 rawler 出图尺寸回填"
        );
    }

    /// 位图行存在但没回填、RAW 行有值 —— SOOC 必须用**资产自己**的尺寸（= 主文件，
    /// 与位图同源），不得跨到 RAW 行。真机症状（2026-10-11，MYP1000037）：
    /// sooc 在 100% 下按 RAW 的 5264 放大，而同源的定稿按 3712 显示。
    #[test]
    fn sooc_never_borrows_the_raw_size_when_the_bitmap_row_is_unfilled() {
        let conn = catalog();
        asset(
            &conn,
            1,
            &[
                ("bitmap", None, None, None),
                ("raw", Some(5264), Some(3904), Some(1)),
            ],
        );
        conn.execute(
            "UPDATE assets SET width = 3712, height = 2784, orientation = 1 WHERE id = 1",
            [],
        )
        .unwrap();
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Sooc).unwrap(),
            Some((3712, 2784)),
            "位图行没回填时退到资产自己的尺寸，不是 RAW 的"
        );
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Raw).unwrap(),
            Some((5264, 3904)),
            "RAW 变体仍按自己的行"
        );
        // 没编辑栈时默认显示（latest）也是基准：位图行 → 资产尺寸
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((3712, 2784))
        );
    }

    #[test]
    fn latest_uses_the_stack_row_then_recomputes_then_the_base() {
        let conn = catalog();
        asset(
            &conn,
            1,
            &[
                ("bitmap", Some(4000), Some(6000), Some(6)),
                ("raw", Some(5208), Some(3904), Some(1)),
            ],
        );
        // 没有栈行 → 基准展示尺寸
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((4000, 6000))
        );
        // 裁一半：输出尺寸由 `output_size` 现算，且与栈同一笔写
        let geometry = EditGeometry {
            rotation: 0.0,
            crop: Some(CropRect {
                x: 0.25,
                y: 0.25,
                width: 0.5,
                height: 0.5,
            }),
            crop_ratio: None,
        };
        stack(&conn, 1, EditBase::Sooc, Some(geometry));
        let stored: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT output_width, output_height FROM develop_stacks WHERE asset_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (Some(2000), Some(3000)), "裁一半就是一半");
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((2000, 3000))
        );

        // 有行但 NULL（老库）→ 现算
        conn.execute(
            "UPDATE develop_stacks SET output_width = NULL, output_height = NULL WHERE asset_id = 1",
            [],
        )
        .unwrap();
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((2000, 3000))
        );

        // 源尺寸也不知道 → NULL
        conn.execute("UPDATE asset_files SET display_width=NULL, display_height=NULL WHERE asset_id=1", [])
            .unwrap();
        conn.execute("UPDATE assets SET width=NULL, height=NULL WHERE id=1", [])
            .unwrap();
        assert_eq!(display_size(&conn, 1, SizeVariant::Latest).unwrap(), None);
    }

    #[test]
    fn issue_uses_its_own_row_then_recomputes_and_never_falls_back_to_base() {
        let conn = catalog();
        asset(
            &conn,
            1,
            &[
                ("bitmap", Some(4000), Some(6000), Some(6)),
                ("raw", Some(5208), Some(3904), Some(1)),
            ],
        );
        let geometry = EditGeometry {
            rotation: 0.0,
            crop: Some(CropRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
            }),
            crop_ratio: None,
        };
        let mut draft = DevelopStack {
            source_base: EditBase::Raw,
            geometry: Some(geometry),
            ..DevelopStack::default()
        };
        draft.params.insert("exposure".into(), 0.5);
        let issue = issues::create(&conn, 1, "柔光", &draft, T0).unwrap();
        let stored: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT output_width, output_height FROM issues WHERE id = ?1",
                [issue.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (Some(2604), Some(1952)), "RAW 3904×5208 的一半");
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Issue(issue.id)).unwrap(),
            Some((2604, 1952)),
            "定稿走自己那一行"
        );

        // NULL → 现算
        conn.execute(
            "UPDATE issues SET output_width = NULL, output_height = NULL WHERE id = ?1",
            [issue.id],
        )
        .unwrap();
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Issue(issue.id)).unwrap(),
            Some((2604, 1952))
        );

        // 定稿不存在 → NULL（不退回基准尺寸 —— 那是另一个变体的大小）
        assert_eq!(display_size(&conn, 1, SizeVariant::Issue(999)).unwrap(), None);
        // 源尺寸不知道 → NULL
        conn.execute("UPDATE asset_files SET display_width=NULL, display_height=NULL WHERE asset_id=1", []).unwrap();
        conn.execute("UPDATE assets SET width=NULL, height=NULL WHERE id=1", []).unwrap();
        assert_eq!(display_size(&conn, 1, SizeVariant::Issue(issue.id)).unwrap(), None);
    }

    #[test]
    fn identity_geometry_outputs_the_source_size() {
        let conn = catalog();
        // 资产要有 RAW 行：`DevelopStack::default()` 的基底就是 RAW，
        // 而 RAW 的尺寸**只认它自己那一行**（不许拿位图/资产的尺寸顶 —— 2026-10-11）。
        asset(
            &conn,
            1,
            &[
                ("bitmap", Some(4000), Some(3000), Some(1)),
                ("raw", Some(5208), Some(3904), Some(1)),
            ],
        );
        assert_eq!(
            stack_output_size(&conn, 1, &DevelopStack::default()).unwrap(),
            Some((5208, 3904))
        );
    }

    #[test]
    fn variant_keys_round_trip() {
        assert_eq!(SizeVariant::parse("sooc"), Some(SizeVariant::Sooc));
        assert_eq!(SizeVariant::parse("raw"), Some(SizeVariant::Raw));
        assert_eq!(SizeVariant::parse("latest"), Some(SizeVariant::Latest));
        assert_eq!(SizeVariant::parse("issue:12"), Some(SizeVariant::Issue(12)));
        assert_eq!(SizeVariant::parse("issue:0"), None);
        assert_eq!(SizeVariant::parse("issue:"), None);
        assert_eq!(SizeVariant::parse("Issue12"), None);
        assert_eq!(SizeVariant::parse(""), None);
    }

    #[test]
    fn ensure_file_sizes_reads_the_header_once_and_only_fills_nulls() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("photos")).unwrap();
        // 竖拍 RAW（裸 TIFF：存储 6000×4000、方向 6 ⇒ 展示 4000×6000）
        write_bare_tiff(&root.join("photos/a.rw2"), 6, 6000, 4000);
        // 另一个文件读不出尺寸：必须保持 NULL，不许写垃圾
        std::fs::write(root.join("photos/b.raw"), b"not an image").unwrap();

        {
            let mut conn = Connection::open(root.join("catalog.db")).unwrap();
            migration::apply(&mut conn, DbKind::Catalog, Backups::none(), T0).unwrap();
            conn.execute(
                "INSERT INTO repository_meta(key, value) VALUES ('repository_id', '0SizesTest000001')",
                [],
            )
            .unwrap();
        }
        let catalog = CatalogDb::open(&root, OpenOpts::unbacked_up(T0)).unwrap();
        catalog
            .write(|conn| {
                crate::store::assets::insert_asset(conn, T0)?;
                for rel in ["photos/a.rw2", "photos/b.raw"] {
                    conn.execute(
                        "INSERT INTO asset_files
                            (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
                             created_at, updated_at)
                         VALUES (1, 'bitmap', ?1, ?1, 'bin', 1, 1, 1, 1)",
                        [rel],
                    )?;
                }
                Ok(())
            })
            .unwrap();

        assert_eq!(ensure_file_sizes(&catalog, 1).unwrap(), 1);
        let (width, height, orientation, raw_width): (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
        ) = catalog
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT (SELECT display_width FROM asset_files WHERE rel_path='photos/a.rw2'),
                                (SELECT display_height FROM asset_files WHERE rel_path='photos/a.rw2'),
                                (SELECT orientation FROM asset_files WHERE rel_path='photos/a.rw2'),
                                (SELECT display_width FROM asset_files WHERE rel_path='photos/b.raw')",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?)
            })
            .unwrap();
        assert_eq!((width, height, orientation), (Some(4000), Some(6000), Some(6)));
        assert_eq!(raw_width, None, "读不出尺寸的文件不许写垃圾");

        // 幂等：第二次不再写（值已在，查询也就空了）
        assert_eq!(ensure_file_sizes(&catalog, 1).unwrap(), 0);
        // 已经补好的行不会被覆盖（写前比较）
        catalog
            .write(|conn| {
                conn.execute(
                    "UPDATE asset_files SET display_width = 20, display_height = 40 WHERE rel_path = 'photos/a.rw2'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(ensure_file_sizes(&catalog, 1).unwrap(), 0);
    }

    /// 裸 TIFF：`MM` 大端 + 一个 IFD，含 Orientation / ImageWidth / ImageLength
    /// （与 `media::meta` 的 RAW 读法同一形状 —— RW2 / ORF 就是这条路径）。
    fn write_bare_tiff(path: &std::path::Path, orientation: u16, width: u32, height: u32) {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MM\x00\x2a");
        bytes.extend_from_slice(&8u32.to_be_bytes());
        let entries: [(u16, u16, u32); 3] = [
            (0x0112, 3, u32::from(orientation)),
            (0x0100, 4, width),
            (0x0101, 4, height),
        ];
        bytes.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        for (tag, kind, value) in entries {
            bytes.extend_from_slice(&tag.to_be_bytes());
            bytes.extend_from_slice(&kind.to_be_bytes());
            bytes.extend_from_slice(&1u32.to_be_bytes());
            let mut slot = [0u8; 4];
            if kind == 3 {
                slot[..2].copy_from_slice(&(value as u16).to_be_bytes());
            } else {
                slot.copy_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&slot);
        }
        bytes.extend_from_slice(&0u32.to_be_bytes());
        std::fs::write(path, bytes).unwrap();
    }


    #[test]
    fn latest_keeps_the_stored_value_and_the_fill_only_fills_nulls() {
        // 崔总 2026-10-11 的模型：latest 是 editor **随时调随时写的现成值**，browse 纯读。
        // 所以存值就是权威（不许读侧拿现算去顶）；只有 NULL（老库 / 源尺寸当时未知）
        // 才由保底算一个**写回去**，下次读又是纯读。
        let conn = catalog();
        asset(&conn, 1, &[("bitmap", Some(4000), Some(6000), Some(1))]);
        stack(&conn, 1, EditBase::Sooc, None);
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((4000, 6000))
        );
        // 存值在 → 就算源行之后变了，读到的仍是存值（模型：源导入后不变；变了是导入 bug）
        conn.execute(
            "UPDATE asset_files SET display_width = 3000, display_height = 4500 WHERE asset_id = 1",
            [],
        )
        .unwrap();
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((4000, 6000)),
            "存值优先：读侧不现算"
        );
    }

    #[test]
    fn ensure_sizes_fills_missing_output_sizes_then_stops() {
        // `specs/display-size-lifecycle.md` §2：现算 → 立刻写回（幂等、只填空值）。
        let conn = catalog();
        asset(&conn, 1, &[("bitmap", Some(4000), Some(6000), Some(1))]);
        stack(&conn, 1, EditBase::Sooc, None);
        conn.execute("UPDATE develop_stacks SET output_width = NULL, output_height = NULL", [])
            .unwrap();
        // 缺值 → 现算（不写）
        assert_eq!(
            display_size(&conn, 1, SizeVariant::Latest).unwrap(),
            Some((4000, 6000))
        );
        // 保底：算完写回（这里树在内存库上，直接用 conn 版本）
        let root = std::path::Path::new("/nonexistent-root");
        assert_eq!(super::ensure_sizes_conn(&conn, root, 1).unwrap(), 1);
        let stored: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT output_width, output_height FROM develop_stacks WHERE asset_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (Some(4000), Some(6000)), "算完要写回");
        // 幂等：第二次不再写
        assert_eq!(super::ensure_sizes_conn(&conn, root, 1).unwrap(), 0);
    }

    #[test]
    fn display_size_source_prefers_rawler_and_never_invents_for_unknown_raw() {
        // 规则矩阵（崔总 2026-10-11）：
        //   * 非 RAW → 我们自己的文件头解析就是权威，`raw_output_size` 不参与；
        //   * RAW → 认 rawler 回传的出图尺寸；**读不到就原样（= 不写垃圾、不拿私自解的数顶上）**。
        let dir = std::env::temp_dir().join(format!("rb-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let jpg = dir.join("a.jpg");
        image::RgbImage::new(40, 20).save(&jpg).unwrap();
        let fake_raw = dir.join("b.rw2");
        std::fs::write(&fake_raw, b"not a raw").unwrap();

        assert_eq!(super::raw_output_size(&jpg), None, "位图不走 rawler");
        let meta = read_photo_meta(&jpg).unwrap();
        assert_eq!(
            super::apply_display_size_source(&jpg, meta),
            meta,
            "位图的展示尺寸就是文件头那份，原样返回"
        );
        assert_eq!(
            super::raw_output_size(&fake_raw),
            None,
            "解不开的 RAW 不报数（不许拿文件头/猜测顶）"
        );
        let raw_meta = PhotoMeta {
            width: 5264,
            height: 3904,
            orientation: 1,
            ..meta
        };
        assert_eq!(
            super::apply_display_size_source(&fake_raw, raw_meta),
            raw_meta,
            "读不到 rawler 的数就不动它 —— 由调用方回退（宁可差一圈也不能编数）"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
