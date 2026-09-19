//! 资产查询：**范围 + 筛选 + 排序 + 分页**（浏览网格的数据源）。
//!
//! `AGENTS.md` §6.4 把 catalog 定为库的真相源，而「按库浏览」要回答的问题就是这里的三条：
//!
//! 1. **有哪些照片**（范围：整库 / 某个目录子树 / 日期区间）；
//! 2. **留下哪些**（筛选：评分、色标、喜欢、锁、日期、器材、标签、文本）；
//! 3. **按什么顺序、从第几张开始**（排序 + 分页）。
//!
//! # 三条设计约束
//!
//! * **参数一律绑定**：筛选条件是**数据**不是 SQL 片段，任何用户输入都走 `?` 占位符
//!   （拼字符串迟早出事，而且中文/引号会立刻暴露）；
//! * **组合子是参数**：多个条件之间是「与」还是「或」由调用方给
//!   （`BROWSE.md` §10 问题 1 的真空白，见 `plans/M2.md` §3 —— 两种都实现，UI 默认「或」）；
//!   单个维度内部的多个取值**永远是「或」**（评分 ∈ {3,4} 就是两个都要）；
//! * **分页给窗口，不给全量**：网格是虚拟化的，只需要可视窗口那几十行；
//!   全量列表另有 [`timeline`]（只取 id + 拍摄时间，给分组与键盘导航算顺序用）。
//!
//! # 旗标（flag）为什么不在筛选里
//!
//! `BROWSE.md` §3.2 定了旗标**只活在内存里**（跨库跨目录、关软件即清），库里没有这一列。
//! 所以「按旗标筛选」在**前端**的内存集合上做，不进 SQL。
//!
//! # 索引：实测结论是**不新增**
//!
//! `plans/M2.md` 步骤 2.4 原本要按 `EXPLAIN QUERY PLAN` 决定是否加
//! `catalog_0004_query_indexes.sql`。10 万条合成库的实测（
//! `examples/query-bench.rs`，2026-09-17）**没有发现需要新索引的地方** ——
//! v1/v2 已有的索引（`taken_at`、三个局部索引、`rel_path_folded` 唯一索引）
//! 已经盖住了所有查询路径，而真正拖慢的是**写法**而不是缺索引：
//!
//! | 问题写法 | 实测 | 改法 |
//! | --- | --- | --- |
//! | `ORDER BY a.taken_at IS NULL, a.taken_at DESC` | 分页 61ms（整表排序） | 去掉 `IS NULL` 项（SQLite 在 DESC 时 NULL 天然靠后）→ **0.23ms** |
//! | `LIKE '前缀/%'` 做子树范围 | 子树计数 26ms（LIKE 用不上索引） | 改成 `>= 前缀/` 且 `< 前缀0` 的范围比较 |
//! | 四条 `GROUP BY` 各扫一遍算分面 | 109ms | 合并成一趟扫描 → **53ms** |
//! | 深翻页 `OFFSET 50000` | 110ms | 排序走索引后 → **25ms** |
//!
//! 所以**不提前加索引**（`PLAN.md` §0 原则 4：基础设施只做当前需要的那部分）。
//! 将来真出现慢查询时，先看这张表里的「写法」一栏。

use rusqlite::types::Value;
use rusqlite::{Connection, Row, params_from_iter};

use crate::error::Result;

/// 查询范围。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scope {
    /// 整个库。
    #[default]
    Repository,
    /// 某个目录**子树**（库内相对路径，`'\'` 与 `'/'` 都接受；空串 = 库根）。
    Subtree { rel_path: String },
}

impl Scope {
    /// 目录子树（写起来方便）。
    #[must_use]
    pub fn subtree(rel_path: impl Into<String>) -> Self {
        Self::Subtree {
            rel_path: rel_path.into(),
        }
    }
}

/// 多个条件之间怎么组合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Combinator {
    /// 全部满足（更精确，但容易筛空）。
    And,
    /// 任一满足（「筛出同类」的手感，`BROWSE.md` §3.1 的典型用法）。
    #[default]
    Or,
}

impl Combinator {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

/// 筛选条件。**所有字段都是「不限 = `None` / 空集合」**。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// 评分取值（0–5；0 表示「没打星」）。
    pub ratings: Vec<u8>,
    /// 色标取值（`red`/`yellow`/`green`/`cyan`/`blue`/`purple`，外加 [`NO_COLOR`] 表示「无色」）。
    pub colors: Vec<String>,
    /// 喜欢状态（`like` / `dislike`，外加 [`NO_LIKE`] 表示「没表态」）。
    pub likes: Vec<String>,
    /// 锁级别（0/1/2）。
    pub locks: Vec<u8>,
    /// 拍摄时间区间（含端点，Unix 毫秒）。
    pub taken_from: Option<i64>,
    pub taken_to: Option<i64>,
    /// 机型（`make|model`；空串代表「无记录」，用 [`NO_CAMERA`]）。
    pub cameras: Vec<String>,
    pub lenses: Vec<String>,
    pub iso_from: Option<i64>,
    pub iso_to: Option<i64>,
    pub focal_from: Option<f64>,
    pub focal_to: Option<f64>,
    /// 标签（任一命中即算）。
    pub tags: Vec<i64>,
    /// 文本（文件名 / 机型 / 镜头 / 描述；中文走 FTS5 trigram，短词回退 LIKE）。
    pub text: Option<String>,
    /// 多条件组合方式。
    pub combinator: Combinator,
}

/// 「无色标」在 `colors` 里的表示（UI 上是那个空心圈）。
pub const NO_COLOR: &str = "none";
/// 「没表过态」在 `likes` 里的表示。
pub const NO_LIKE: &str = "none";
/// 「没有器材信息」在 `cameras` / `lenses` 里的表示。
pub const NO_CAMERA: &str = "none";

impl Filter {
    /// 一个条件都没有？
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.conditions().is_empty()
    }

    /// 把筛选拆成若干「条件」（每个条件自带 SQL 与参数）。
    fn conditions(&self) -> Vec<(String, Vec<Value>)> {
        let mut out: Vec<(String, Vec<Value>)> = Vec::new();

        if !self.ratings.is_empty() {
            let placeholders = placeholders(self.ratings.len());
            let params = self
                .ratings
                .iter()
                .map(|r| Value::Integer(i64::from(*r)))
                .collect();
            out.push((format!("a.rating IN ({placeholders})"), params));
        }

        if let Some((sql, params)) = nullable_in("a.color_label", &self.colors) {
            out.push((sql, params));
        }

        if let Some((sql, params)) = nullable_in("a.like_state", &self.likes) {
            out.push((sql, params));
        }

        if !self.locks.is_empty() {
            let placeholders = placeholders(self.locks.len());
            let params = self
                .locks
                .iter()
                .map(|l| Value::Integer(i64::from(*l)))
                .collect();
            out.push((format!("a.lock_level IN ({placeholders})"), params));
        }

        if let Some(from) = self.taken_from {
            out.push(("a.taken_at >= ?".to_string(), vec![Value::Integer(from)]));
        }
        if let Some(to) = self.taken_to {
            out.push(("a.taken_at <= ?".to_string(), vec![Value::Integer(to)]));
        }

        if let Some((sql, params)) = nullable_in(CAMERA_EXPR, &self.cameras) {
            out.push((sql, params));
        }
        if let Some((sql, params)) = nullable_in("a.lens", &self.lenses) {
            out.push((sql, params));
        }

        if let Some(from) = self.iso_from {
            out.push(("a.iso >= ?".to_string(), vec![Value::Integer(from)]));
        }
        if let Some(to) = self.iso_to {
            out.push(("a.iso <= ?".to_string(), vec![Value::Integer(to)]));
        }
        if let Some(from) = self.focal_from {
            out.push(("a.focal_mm >= ?".to_string(), vec![Value::Real(from)]));
        }
        if let Some(to) = self.focal_to {
            out.push(("a.focal_mm <= ?".to_string(), vec![Value::Real(to)]));
        }

        if !self.tags.is_empty() {
            let placeholders = placeholders(self.tags.len());
            let mut params: Vec<Value> = Vec::with_capacity(self.tags.len());
            params.extend(self.tags.iter().map(|t| Value::Integer(*t)));
            out.push((
                format!(
                    "EXISTS (SELECT 1 FROM asset_tags t \
                     WHERE t.asset_id = a.id AND t.tag_id IN ({placeholders}))"
                ),
                params,
            ));
        }

        if let Some(text) = self
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            out.push(text_condition(text));
        }

        out
    }
}

/// 机型表达式：`make|model`；**两边都空时是 `NULL`**。
///
/// 用 `|` 而不是空格：品牌名里本来就有空格（`OM Digital Solutions`），
/// 用空格拼会让 `make|model` 的边界变得不可判定。
///
/// `NULLIF(…, '|')` 不是装饰：没有它，这个表达式**永远不为 NULL**，
/// 「筛出没有器材信息的照片」就永远筛不到（`NO_CAMERA` 会静默失效）。
const CAMERA_EXPR: &str =
    "NULLIF(COALESCE(a.camera_make, '') || '|' || COALESCE(a.camera_model, ''), '|')";

/// 生成 `列 IN (...)`；若集合里含 `"none"`，同时接受 `NULL`。
fn nullable_in(column: &str, values: &[String]) -> Option<(String, Vec<Value>)> {
    if values.is_empty() {
        return None;
    }
    let wants_null = values
        .iter()
        .any(|v| v == NO_COLOR || v == NO_LIKE || v == NO_CAMERA);
    let concrete: Vec<&String> = values
        .iter()
        .filter(|v| !(v.as_str() == NO_COLOR || v.as_str() == NO_LIKE || v.as_str() == NO_CAMERA))
        .collect();

    let mut sql = String::new();
    let mut params: Vec<Value> = Vec::new();
    if !concrete.is_empty() {
        sql.push_str(&format!("{column} IN ({})", placeholders(concrete.len())));
        params.extend(concrete.into_iter().map(|v| Value::Text(v.clone())));
    }
    if wants_null {
        if !sql.is_empty() {
            sql.insert(0, '(');
            sql.push_str(") OR ");
        }
        sql.push_str(&format!("{column} IS NULL"));
    }
    Some((sql, params))
}

/// 文本条件：中文/长词走 FTS5 trigram，短词（< 3 字符）回退 `LIKE`。
///
/// 为什么要回退：**trigram 按 3 个字符切分**，查询词少于 3 个字符永远搜不到
/// （`AGENTS.md` §7.2 与 `catalog_0001_init.sql` 的注释都记着这条）。
fn text_condition(text: &str) -> (String, Vec<Value>) {
    let chars = text.chars().count();
    if chars >= 3 {
        (
            "a.id IN (SELECT rowid FROM assets_fts WHERE assets_fts MATCH ?)".to_string(),
            vec![Value::Text(fts_query(text))],
        )
    } else {
        // 短词：只在**文件名**上找（其它列短词命中率低，而且 LIKE 要全表扫，代价得压住）。
        // 用折叠路径比较：与文件身份、配对、去重同一套大小写口径（`AGENTS.md` §7.3）。
        (
            "EXISTS (SELECT 1 FROM asset_files sf WHERE sf.asset_id = a.id \
             AND sf.rel_path_folded LIKE ?)"
                .to_string(),
            vec![Value::Text(format!("%{}", text.to_lowercase()))],
        )
    }
}

/// 把用户输入变成一个**安全的 FTS5 查询串**。
///
/// FTS5 的查询语法里 `"`、`*`、`-`、`(`、`)` 都有含义，用户随手输入的标点会把整条
/// MATCH 变成语法错误。办法：把整串包成**一个带引号的短语**，内部的 `"` 双写转义。
fn fts_query(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// 生成 `?, ?, …`。
fn placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 3);
    for i in 0..n {
        if i > 0 {
            s.push_str(", ");
        }
        s.push('?');
    }
    s
}

/// 排序键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// 拍摄时间（默认，新的在前）。没有拍摄时间的排最后。
    #[default]
    TakenAt,
    /// 导入时间。
    ImportedAt,
    /// 文件名（路径折叠形式，与文件管理器一致）。
    FileName,
    /// 评分（高的在前）。
    Rating,
    /// 机型。
    Camera,
}

impl SortKey {
    /// `ORDER BY` 子句（**不含**方向，方向由 [`Sort`] 决定）。
    ///
    /// 每个键后面都跟 `id` 做**稳定次级排序**：没有它，同值行的顺序在不同查询之间
    /// 可能漂移，分页就会看到重复/漏掉的行。
    fn order_by(self, desc: bool) -> String {
        let dir = if desc { "DESC" } else { "ASC" };
        /*
         * ⚠️ **不要在这里加 `a.taken_at IS NULL,` 这样的前缀项** ——
         * 它会让 SQLite 用不上索引排序，退化成「整表排序 + 取前 N 行」：
         * 10 万条的实测是分页 61ms→ 去掉后降到个位数。
         *
         * 「没有拍摄时间的排最后」不需要它：**SQLite 里 NULL 在 DESC 时天然排最后**
         * （升序时才排最前 —— 那时也符合「不知道的先放前面」的直觉）。
         */
        match self {
            Self::TakenAt => format!("a.taken_at {dir}, a.id {dir}"),
            Self::ImportedAt => format!("a.imported_at {dir}, a.id {dir}"),
            Self::FileName => format!("f.rel_path_folded {dir}, a.id {dir}"),
            Self::Rating => format!("a.rating {dir}, a.taken_at {dir}, a.id {dir}"),
            Self::Camera => format!(
                "({CAMERA_EXPR}) IS NULL, ({CAMERA_EXPR}) {dir}, a.taken_at {dir}, a.id {dir}"
            ),
        }
    }
}

/// 排序（键 + 方向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sort {
    pub key: SortKey,
    /// `true` = 降序。默认按拍摄时间**降序**（新的在前）。
    pub desc: bool,
}

impl Sort {
    #[must_use]
    pub const fn new(key: SortKey, desc: bool) -> Self {
        Self { key, desc }
    }

    /// 该排序键的默认方向（拍摄时间/导入时间/评分习惯上「新的/高的在前」）。
    #[must_use]
    pub const fn default_desc(key: SortKey) -> bool {
        !matches!(key, SortKey::FileName | SortKey::Camera)
    }
}

/// 一次查询（范围 + 筛选 + 排序）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub scope: Scope,
    pub filter: Filter,
    pub sort: Sort,
}

impl Query {
    #[must_use]
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            filter: Filter::default(),
            sort: Sort {
                key: SortKey::TakenAt,
                desc: true,
            },
        }
    }

    /// 组装 `WHERE`（含范围）与参数。
    fn where_clause(&self) -> (String, Vec<Value>) {
        let mut params: Vec<Value> = Vec::new();
        let mut parts: Vec<String> = Vec::new();

        // 范围永远是「与」——它是「我在看哪一块」，不是筛选条件。
        // 空前缀 = 库根 = 整库：不产生任何条件（否则会变成 `LIKE '/%'` 一条都匹配不上）
        if let Scope::Subtree { rel_path } = &self.scope
            && !normalize_rel_prefix(rel_path).is_empty()
        {
            let prefix = normalize_rel_prefix(rel_path);
            /*
             * 用**范围比较**而不是 `LIKE '前缀/%'`：
             *
             * * `LIKE` 在 SQLite 里默认大小写不敏感 ⇒ 用不上普通索引（要扫全表）；
             * * 范围比较是索引的直接用法：`>= 'photos/x/'` 且 `< 'photos/x0'`
             *   （`'/'` 是 0x2F，`'0'` 是 0x30，任何以 `photos/x/` 开头的串都落在这个区间）。
             *
             * 实测：10 万条的子树计数 26ms → 个位数。
             */
            let lower = format!("{prefix}/");
            let upper = format!("{prefix}0");
            parts.push(
                "EXISTS (SELECT 1 FROM asset_files sf WHERE sf.asset_id = a.id \
                 AND (sf.rel_path_folded = ? OR (sf.rel_path_folded >= ? AND sf.rel_path_folded < ?)))"
                    .to_string(),
            );
            params.push(Value::Text(prefix));
            params.push(Value::Text(lower));
            params.push(Value::Text(upper));
        }

        let conditions = self.filter.conditions();
        match conditions.len() {
            0 => {}
            1 => {
                let (sql, cond_params) = conditions.into_iter().next().expect("长度已确认");
                parts.push(sql);
                params.extend(cond_params);
            }
            _ => {
                let joiner = match self.filter.combinator {
                    Combinator::And => " AND ",
                    Combinator::Or => " OR ",
                };
                let mut sql = String::from("(");
                for (i, (cond, cond_params)) in conditions.into_iter().enumerate() {
                    if i > 0 {
                        sql.push_str(joiner);
                    }
                    sql.push_str(&cond);
                    params.extend(cond_params);
                }
                sql.push(')');
                parts.push(sql);
            }
        }

        let where_sql = if parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", parts.join(" AND "))
        };
        (where_sql, params)
    }
}

/// 库内相对路径的前缀：统一分隔符、去掉首尾空白与尾部分隔符。
fn normalize_rel_prefix(input: &str) -> String {
    input
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

/// 一行资产（网格要渲染的东西，一次查询拿齐，不再逐行回查）。
#[derive(Debug, Clone, PartialEq)]
pub struct AssetRow {
    pub id: i64,
    /// 展示用文件的库内相对路径（有 bitmap 就是 bitmap，否则 RAW）。
    pub rel_path: String,
    /// 该文件的扩展名（小写，不含点）。
    pub ext: String,
    /// 展示用文件是位图还是 RAW。
    pub is_raw: bool,
    pub taken_at: Option<i64>,
    /// 拍摄时间用的时区偏移（分钟）；`None` = 相机没写，按 UTC 看。
    pub taken_at_offset_min: Option<i64>,
    pub rating: u8,
    pub color_label: Option<String>,
    pub like_state: Option<String>,
    pub lock_level: u8,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub f_number: Option<f64>,
    pub exposure_ms: Option<f64>,
    pub iso: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub size_bytes: Option<i64>,
    /// 展示用文件已被标记为缺失（磁盘上找不到了）。
    pub missing: bool,
}

/// 文件名（含扩展名）——从库内相对路径取最后一段。
#[must_use]
pub fn file_name_of(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path)
}

const ROW_COLUMNS: &str = "\
    a.id, \
    COALESCE(f.rel_path, '') AS rel_path, \
    COALESCE(f.ext, '') AS ext, \
    COALESCE(f.role, '') AS role, \
    a.taken_at, a.taken_at_offset_min, a.rating, a.color_label, a.like_state, a.lock_level, \
    a.camera_make, a.camera_model, a.lens, a.focal_mm, a.f_number, a.exposure_ms, a.iso, \
    a.width, a.height, a.orientation, f.size_bytes, f.missing_since";

/// 展示用文件的选取规则：**有位图就位图，没有就 RAW**。
///
/// `'bitmap' < 'raw' < 'sidecar'`（文本序），所以 `MIN(role)` 正好是这个优先级。
/// 这样「库内同时有位图和 RAW」的照片在网格里显示位图（读得快、颜色也对），
/// 而将来做「用 RAW 重新显影」时按 `role = 'raw'` 再取一次即可。
const DISPLAY_FILE_JOIN: &str = "\
    LEFT JOIN asset_files f ON f.asset_id = a.id \
     AND f.role = (SELECT MIN(role) FROM asset_files WHERE asset_id = a.id)";

fn row_from(row: &Row<'_>) -> rusqlite::Result<AssetRow> {
    let rel_path: String = row.get(1)?;
    let role: String = row.get(3)?;
    Ok(AssetRow {
        id: row.get(0)?,
        rel_path,
        ext: row.get(2)?,
        is_raw: role == "raw",
        taken_at: row.get(4)?,
        taken_at_offset_min: row.get(5)?,
        rating: u8::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
        color_label: row.get(7)?,
        like_state: row.get(8)?,
        lock_level: u8::try_from(row.get::<_, i64>(9)?).unwrap_or(0),
        camera_make: row.get(10)?,
        camera_model: row.get(11)?,
        lens: row.get(12)?,
        focal_mm: row.get(13)?,
        f_number: row.get(14)?,
        exposure_ms: row.get(15)?,
        iso: row.get(16)?,
        width: row.get(17)?,
        height: row.get(18)?,
        orientation: row.get(19)?,
        size_bytes: row.get(20)?,
        missing: row.get::<_, Option<i64>>(21)?.is_some(),
    })
}

/// 查询前的准备：**用到文本检索时，先保证 FTS 索引是最新的**。
///
/// `assets_fts` 是派生索引（`AGENTS.md` §6.5），而它**没有挂在导入的热路径上** ——
/// 靠这里的新鲜度检查兜底（见 [`crate::store::fts`]）。没有文本条件时这一步不做任何事。
fn prepare(conn: &Connection, query: &Query) -> Result<()> {
    if query
        .filter
        .text
        .as_deref()
        .map(str::trim)
        .is_some_and(|t| !t.is_empty())
    {
        crate::store::fts::ensure_fresh(conn)?;
    }
    Ok(())
}

/// 符合条件的有多少张（**不 join 文件表** —— 计数只需要 `assets` 与筛选条件）。
pub fn count(conn: &Connection, query: &Query) -> Result<i64> {
    prepare(conn, query)?;
    let (where_sql, params) = query.where_clause();
    let sql = format!("SELECT count(*) FROM assets a {where_sql}");
    let total = conn.query_row(&sql, params_from_iter(params.iter()), |r| r.get(0))?;
    Ok(total)
}

/// 取可视窗口那一页（虚拟网格只要这几十行）。
pub fn page(
    conn: &Connection,
    query: &Query,
    offset: usize,
    limit: usize,
) -> Result<Vec<AssetRow>> {
    prepare(conn, query)?;
    let (where_sql, mut params) = query.where_clause();
    let order = query.sort.key.order_by(query.sort.desc);
    let sql = format!(
        "SELECT {ROW_COLUMNS} FROM assets a {DISPLAY_FILE_JOIN} {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
    );
    params.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
    params.push(Value::Integer(i64::try_from(offset).unwrap_or(i64::MAX)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), row_from)?;
    let mut out = Vec::with_capacity(limit.min(4096));
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 时间线上的一行。
///
/// `rel_path` 是**给排序用的**：片内要按「文件名自然序」排，而自然序不是 SQL 能表达的
/// （`P1000019` 该排在 `P1000020` 前面，但字符串比较会反过来）—— 所以路径得跟回来前端排。
/// 代价是多一列文本（10 万张约 +3–4MB）。它是「同一时间段内 JPG 与 RAW 接着」的前提，
/// 而那是人看片时的实际期待。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineRow {
    pub id: i64,
    pub taken_at: Option<i64>,
    /// 库内相对路径（原始大小写）。只用于排序与展示回退。
    pub rel_path: String,
}

/// 只要「id + 拍摄时间 + 相对路径」的有序列表 —— 给分组、区间选择与键盘导航算顺序用。
pub fn timeline(conn: &Connection, query: &Query, limit: usize) -> Result<Vec<TimelineRow>> {
    prepare(conn, query)?;
    let (where_sql, mut params) = query.where_clause();
    let order = query.sort.key.order_by(query.sort.desc);
    // join 现在**总是**要：`rel_path` 是选择的列之一
    // （以前只有按文件名排序时才需要它 —— 那时 `ORDER BY` 里用到 `f.`）。
    let join = DISPLAY_FILE_JOIN;
    let limit_sql = if limit == 0 {
        String::new()
    } else {
        params.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
        " LIMIT ?".to_string()
    };
    let sql = format!(
        "SELECT a.id, a.taken_at, COALESCE(f.rel_path, '') FROM assets a {join} {where_sql} \
         ORDER BY {order}{limit_sql}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), |r| {
        Ok(TimelineRow {
            id: r.get(0)?,
            taken_at: r.get(1)?,
            rel_path: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 一个维度的取值分布（筛选面板用它显示「这个取值有几张」/灰掉 0 张的项）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    /// 评分 → 张数（**只含有效评分**）。
    pub ratings: Vec<(u8, i64)>,
    /// 色标 → 张数（`None` = 无色标）。
    pub colors: Vec<(Option<String>, i64)>,
    /// 喜欢状态 → 张数。
    pub likes: Vec<(Option<String>, i64)>,
    /// 锁级别 → 张数。
    pub locks: Vec<(u8, i64)>,
}

/// 统计各取值分布。**筛选条件会先应用**（得到的是「在现有筛选下还剩多少」）。
pub fn facets(conn: &Connection, query: &Query) -> Result<Facets> {
    prepare(conn, query)?;
    let (where_sql, params) = query.where_clause();
    let mut out = Facets::default();

    /*
     * **一趟扫描**算四个维度：分开跑四条 `GROUP BY` 要扫四遍 10 万行
     * （实测 109ms），合成一条只有一遍（分组数最多 6×6×3×3，可以忽略）。
     * 折叠在 Rust 里做 —— SQL 的 `GROUP BY` 已经替我们把大部分行合并掉了。
     */
    let mut ratings: std::collections::BTreeMap<u8, i64> = std::collections::BTreeMap::new();
    let mut colors: std::collections::BTreeMap<Option<String>, i64> =
        std::collections::BTreeMap::new();
    let mut likes: std::collections::BTreeMap<Option<String>, i64> =
        std::collections::BTreeMap::new();
    let mut locks: std::collections::BTreeMap<u8, i64> = std::collections::BTreeMap::new();

    let mut stmt = conn.prepare(&format!(
        "SELECT a.rating, a.color_label, a.like_state, a.lock_level, count(*)
           FROM assets a {where_sql}
          GROUP BY a.rating, a.color_label, a.like_state, a.lock_level"
    ))?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    while let Some(row) = rows.next()? {
        let rating = u8::try_from(row.get::<_, i64>(0)?).unwrap_or(0);
        let color: Option<String> = row.get(1)?;
        let like: Option<String> = row.get(2)?;
        let lock = u8::try_from(row.get::<_, i64>(3)?).unwrap_or(0);
        let n: i64 = row.get(4)?;
        *ratings.entry(rating).or_insert(0) += n;
        *colors.entry(color).or_insert(0) += n;
        *likes.entry(like).or_insert(0) += n;
        *locks.entry(lock).or_insert(0) += n;
    }

    out.ratings = ratings.into_iter().collect();
    out.colors = colors.into_iter().collect();
    out.likes = likes.into_iter().collect();
    out.locks = locks.into_iter().collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;

    const T0: i64 = 1_789_516_800_000;
    const DAY: i64 = 86_400_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    struct Spec<'a> {
        rel: &'a str,
        role: &'a str,
        taken_at: Option<i64>,
        rating: i64,
        color: Option<&'a str>,
        like_state: Option<&'a str>,
        lock: i64,
        make: Option<&'a str>,
        model: Option<&'a str>,
        lens: Option<&'a str>,
        iso: Option<i64>,
        focal: Option<f64>,
        description: Option<&'a str>,
    }

    impl<'a> Default for Spec<'a> {
        fn default() -> Self {
            Self {
                rel: "",
                role: "bitmap",
                taken_at: Some(T0),
                rating: 0,
                color: None,
                like_state: None,
                lock: 0,
                make: None,
                model: None,
                lens: None,
                iso: None,
                focal: None,
                description: None,
            }
        }
    }

    /// 调用方不关心路径时（`rel` 为空）给一个**唯一**的：
    /// `asset_files.rel_path_folded` 上有唯一索引，都用同一个路径会直接插不进去。
    fn add(conn: &Connection, spec: Spec<'_>) -> i64 {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        let rel = if spec.rel.is_empty() {
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!("photos/auto-{n}.jpg")
        } else {
            spec.rel.to_string()
        };
        let spec = Spec { rel: &rel, ..spec };
        conn.execute(
            "INSERT INTO assets
               (taken_at, rating, color_label, like_state, lock_level,
                camera_make, camera_model, lens, iso, focal_mm, description,
                imported_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
            rusqlite::params![
                spec.taken_at,
                spec.rating,
                spec.color,
                spec.like_state,
                spec.lock,
                spec.make,
                spec.model,
                spec.lens,
                spec.iso,
                spec.focal,
                spec.description,
                T0
            ],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let ext = spec.rel.rsplit('.').next().unwrap_or("").to_lowercase();
        conn.execute(
            "INSERT INTO asset_files
               (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1000, ?6, ?6)",
            rusqlite::params![id, spec.role, spec.rel, spec.rel.to_lowercase(), ext, T0],
        )
        .unwrap();
        id
    }

    fn ids_of(rows: &[AssetRow]) -> Vec<i64> {
        rows.iter().map(|r| r.id).collect()
    }

    // ---------- 空库与单条 ----------

    #[test]
    fn an_empty_repository_answers_everything_with_zero() {
        let conn = catalog();
        let query = Query::new(Scope::Repository);
        assert_eq!(count(&conn, &query).unwrap(), 0);
        assert!(page(&conn, &query, 0, 50).unwrap().is_empty());
        assert!(timeline(&conn, &query, 0).unwrap().is_empty());
        let f = facets(&conn, &query).unwrap();
        assert_eq!(f, Facets::default());
    }

    #[test]
    fn a_single_asset_round_trips() {
        let conn = catalog();
        let id = add(
            &conn,
            Spec {
                rel: "photos/2026-08-15/MYP0001.jpg",
                ..Spec::default()
            },
        );
        let query = Query::new(Scope::Repository);
        assert_eq!(count(&conn, &query).unwrap(), 1);
        let rows = page(&conn, &query, 0, 10).unwrap();
        assert_eq!(ids_of(&rows), vec![id]);
        assert_eq!(rows[0].rel_path, "photos/2026-08-15/MYP0001.jpg");
        assert_eq!(rows[0].ext, "jpg");
        assert!(!rows[0].is_raw);
        assert!(!rows[0].missing);
    }

    #[test]
    fn paging_beyond_the_end_is_empty_not_an_error() {
        let conn = catalog();
        add(&conn, Spec::default());
        let query = Query::new(Scope::Repository);
        assert!(page(&conn, &query, 5, 10).unwrap().is_empty());
        assert!(page(&conn, &query, 0, 0).unwrap().is_empty());
    }

    // ---------- 展示用文件的选择 ----------

    #[test]
    fn the_bitmap_is_the_display_file_when_raw_exists_too() {
        let conn = catalog();
        let id = add(
            &conn,
            Spec {
                rel: "photos/2026-08-15/_RAW/MYP0001.ORF",
                role: "raw",
                ..Spec::default()
            },
        );
        // 同一个资产再挂一个位图
        conn.execute(
            "INSERT INTO asset_files
               (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, created_at, updated_at)
             VALUES (?1, 'bitmap', 'photos/2026-08-15/MYP0001.jpg',
                     'photos/2026-08-15/myp0001.jpg', 'jpg', 2000, ?2, ?2)",
            rusqlite::params![id, T0],
        )
        .unwrap();

        let rows = page(&conn, &Query::new(Scope::Repository), 0, 10).unwrap();
        assert_eq!(rows.len(), 1, "一张照片只有一行");
        assert_eq!(rows[0].rel_path, "photos/2026-08-15/MYP0001.jpg");
        assert!(!rows[0].is_raw, "有位图就显示位图");
    }

    #[test]
    fn a_raw_only_asset_reports_itself_as_raw() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                rel: "photos/IMG_0001.ORF",
                role: "raw",
                ..Spec::default()
            },
        );
        let rows = page(&conn, &Query::new(Scope::Repository), 0, 10).unwrap();
        assert!(rows[0].is_raw);
    }

    // ---------- 范围 ----------

    #[test]
    fn subtree_scope_matches_the_folder_and_its_children_only() {
        let conn = catalog();
        let day1 = add(
            &conn,
            Spec {
                rel: "photos/2026-08-15/a.jpg",
                ..Spec::default()
            },
        );
        let day1_deep = add(
            &conn,
            Spec {
                rel: "photos/2026-08-15/sub/b.jpg",
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rel: "photos/2026-08-16/c.jpg",
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rel: "photos/2026-08-150/d.jpg",
                ..Spec::default()
            },
        );

        let query = Query::new(Scope::subtree("photos/2026-08-15"));
        let mut got = ids_of(&page(&conn, &query, 0, 50).unwrap());
        got.sort_unstable();
        assert_eq!(got, vec![day1, day1_deep]);
        assert_eq!(count(&conn, &query).unwrap(), 2);
    }

    #[test]
    fn subtree_scope_tolerates_backslashes_and_stray_slashes() {
        let conn = catalog();
        let id = add(
            &conn,
            Spec {
                rel: "photos/2026-08-15/a.jpg",
                ..Spec::default()
            },
        );
        for raw in [
            "photos/2026-08-15",
            "/photos/2026-08-15/",
            "photos\\2026-08-15",
            "  photos/2026-08-15  ",
        ] {
            let query = Query::new(Scope::subtree(raw));
            assert_eq!(
                ids_of(&page(&conn, &query, 0, 10).unwrap()),
                vec![id],
                "{raw}"
            );
        }
    }

    #[test]
    fn an_empty_subtree_scope_means_the_whole_repository() {
        let conn = catalog();
        add(&conn, Spec::default());
        let query = Query::new(Scope::subtree(""));
        assert_eq!(count(&conn, &query).unwrap(), 1, "空路径 = 库根");
    }

    // ---------- 筛选 ----------

    #[test]
    fn rating_filter_takes_a_set_of_values() {
        let conn = catalog();
        let three = add(
            &conn,
            Spec {
                rating: 3,
                ..Spec::default()
            },
        );
        let five = add(
            &conn,
            Spec {
                rating: 5,
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rating: 1,
                ..Spec::default()
            },
        );

        let mut query = Query::new(Scope::Repository);
        query.filter.ratings = vec![3, 5];
        let mut got = ids_of(&page(&conn, &query, 0, 10).unwrap());
        got.sort_unstable();
        assert_eq!(got, vec![three, five]);
    }

    #[test]
    fn zero_rating_means_unrated_and_is_selectable() {
        let conn = catalog();
        let unrated = add(&conn, Spec::default());
        add(
            &conn,
            Spec {
                rating: 2,
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        query.filter.ratings = vec![0];
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![unrated]);
    }

    #[test]
    fn color_filter_can_mean_no_color_at_all() {
        let conn = catalog();
        let red = add(
            &conn,
            Spec {
                color: Some("red"),
                ..Spec::default()
            },
        );
        let plain = add(&conn, Spec::default());

        let mut query = Query::new(Scope::Repository);
        query.filter.colors = vec![NO_COLOR.to_string()];
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![plain]);

        query.filter.colors = vec!["red".to_string(), NO_COLOR.to_string()];
        assert_eq!(page(&conn, &query, 0, 10).unwrap().len(), 2);
        assert!(red > 0);
    }

    #[test]
    fn lock_filter_and_exact_lock_levels() {
        let conn = catalog();
        add(&conn, Spec::default());
        let locked = add(
            &conn,
            Spec {
                lock: 2,
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        query.filter.locks = vec![2];
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![locked]);
    }

    #[test]
    fn date_range_is_inclusive_on_both_ends() {
        let conn = catalog();
        let first = add(
            &conn,
            Spec {
                taken_at: Some(T0),
                ..Spec::default()
            },
        );
        let last = add(
            &conn,
            Spec {
                taken_at: Some(T0 + 10 * DAY),
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                taken_at: Some(T0 + 20 * DAY),
                ..Spec::default()
            },
        );

        let mut query = Query::new(Scope::Repository);
        query.filter.combinator = Combinator::And; // 区间两头都要满足
        query.filter.taken_from = Some(T0);
        query.filter.taken_to = Some(T0 + 10 * DAY);
        let mut got = ids_of(&page(&conn, &query, 0, 10).unwrap());
        got.sort_unstable();
        assert_eq!(got, vec![first, last]);
    }

    #[test]
    fn photos_without_a_taken_at_never_match_a_date_range() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                taken_at: None,
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        query.filter.taken_from = Some(0);
        assert_eq!(count(&conn, &query).unwrap(), 0);
        // 但不带日期条件时它们照常出现（NULL 是「不知道」，不是「不属于」）
        assert_eq!(count(&conn, &Query::new(Scope::Repository)).unwrap(), 1);
    }

    #[test]
    fn camera_filter_uses_make_and_model_together() {
        let conn = catalog();
        let g9 = add(
            &conn,
            Spec {
                make: Some("Panasonic"),
                model: Some("DC-G9"),
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                make: Some("Canon"),
                model: Some("EOS R5"),
                ..Spec::default()
            },
        );

        let mut query = Query::new(Scope::Repository);
        query.filter.cameras = vec!["Panasonic|DC-G9".to_string()];
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![g9]);

        // 没有器材信息的照片也能被点名筛出来
        add(&conn, Spec::default());
        query.filter.cameras = vec![NO_CAMERA.to_string()];
        assert_eq!(page(&conn, &query, 0, 10).unwrap().len(), 1);
    }

    #[test]
    fn iso_and_focal_ranges() {
        let conn = catalog();
        let in_range = add(
            &conn,
            Spec {
                iso: Some(800),
                focal: Some(35.0),
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                iso: Some(6400),
                focal: Some(200.0),
                ..Spec::default()
            },
        );

        let mut query = Query::new(Scope::Repository);
        // 区间是多条条件，默认「或」会把只满足一头也算上 —— 这里显式「与」
        query.filter.combinator = Combinator::And;
        query.filter.iso_from = Some(400);
        query.filter.iso_to = Some(1600);
        query.filter.focal_from = Some(20.0);
        query.filter.focal_to = Some(50.0);
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![in_range]);
    }

    #[test]
    fn tag_filter_matches_any_of_the_given_tags() {
        let conn = catalog();
        let a = add(&conn, Spec::default());
        add(&conn, Spec::default());
        for (asset, tag) in [(a, 7_i64), (a, 9)] {
            conn.execute(
                "INSERT INTO asset_tags (asset_id, tag_id, tagged_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![asset, tag, T0],
            )
            .unwrap();
        }
        let mut query = Query::new(Scope::Repository);
        query.filter.tags = vec![9];
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![a]);
    }

    // ---------- 组合子 ----------

    #[test]
    fn and_requires_every_condition_or_accepts_any() {
        let conn = catalog();
        let both = add(
            &conn,
            Spec {
                rating: 3,
                color: Some("red"),
                ..Spec::default()
            },
        );
        let only_rating = add(
            &conn,
            Spec {
                rating: 3,
                ..Spec::default()
            },
        );
        let only_color = add(
            &conn,
            Spec {
                color: Some("red"),
                ..Spec::default()
            },
        );

        let mut query = Query::new(Scope::Repository);
        query.filter.ratings = vec![3];
        query.filter.colors = vec!["red".to_string()];

        query.filter.combinator = Combinator::And;
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![both]);

        query.filter.combinator = Combinator::Or;
        let mut got = ids_of(&page(&conn, &query, 0, 10).unwrap());
        got.sort_unstable();
        let mut want = vec![both, only_rating, only_color];
        want.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn or_defaults_to_the_whole_repository_when_nothing_is_set() {
        let conn = catalog();
        add(&conn, Spec::default());
        add(&conn, Spec::default());
        let query = Query::new(Scope::Repository);
        assert!(query.filter.is_empty());
        assert_eq!(count(&conn, &query).unwrap(), 2, "没有条件 = 不筛");
    }

    // ---------- 排序 ----------

    #[test]
    fn default_sort_is_newest_first_with_unknown_times_last() {
        let conn = catalog();
        let unknown = add(
            &conn,
            Spec {
                taken_at: None,
                ..Spec::default()
            },
        );
        let old = add(
            &conn,
            Spec {
                taken_at: Some(T0),
                ..Spec::default()
            },
        );
        let new = add(
            &conn,
            Spec {
                taken_at: Some(T0 + DAY),
                ..Spec::default()
            },
        );
        let rows = page(&conn, &Query::new(Scope::Repository), 0, 10).unwrap();
        assert_eq!(ids_of(&rows), vec![new, old, unknown]);
    }

    #[test]
    fn filename_sort_is_case_insensitive_and_stable() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                rel: "photos/B.jpg",
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rel: "photos/a.jpg",
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        query.sort = Sort::new(SortKey::FileName, false);
        let rows = page(&conn, &query, 0, 10).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.rel_path.as_str()).collect::<Vec<_>>(),
            vec!["photos/a.jpg", "photos/B.jpg"]
        );
        // 同一个排序键在 timeline 上也要能用（那条 SQL 走的是另一条路径）
        let times = timeline(&conn, &query, 0).unwrap();
        assert_eq!(times.len(), 2);
    }

    #[test]
    fn rating_sort_puts_high_ratings_first() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                rating: 1,
                ..Spec::default()
            },
        );
        let best = add(
            &conn,
            Spec {
                rating: 5,
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        query.sort = Sort::new(SortKey::Rating, true);
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap())[0], best);
    }

    #[test]
    fn paging_is_consistent_across_pages() {
        let conn = catalog();
        for i in 0..25 {
            add(
                &conn,
                Spec {
                    taken_at: Some(T0 + i * 1000),
                    ..Spec::default()
                },
            );
        }
        let query = Query::new(Scope::Repository);
        let mut seen: Vec<i64> = Vec::new();
        for offset in (0..25).step_by(10) {
            seen.extend(ids_of(&page(&conn, &query, offset, 10).unwrap()));
        }
        let mut unique = seen.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 25, "翻页不该重复或漏行");
        assert_eq!(seen.len(), 25);
    }

    // ---------- 文本 ----------

    #[test]
    fn chinese_text_needs_the_index_and_works_once_fresh() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                rel: "photos/婚礼现场.jpg",
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        query.filter.text = Some("婚礼现场".to_string());
        // 索引是派生数据：查询前会自动补上
        assert_eq!(count(&conn, &query).unwrap(), 1);
        assert!(crate::store::fts::is_fresh(&conn).unwrap());
    }

    #[test]
    fn short_queries_fall_back_to_like_on_the_file_name() {
        let conn = catalog();
        let id = add(
            &conn,
            Spec {
                rel: "photos/MYP0007.jpg",
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        // trigram 切不出 2 个字符，必须走 LIKE 兜底（AGENTS.md §7.2）
        query.filter.text = Some("myp".to_string());
        assert_eq!(ids_of(&page(&conn, &query, 0, 10).unwrap()), vec![id]);
        query.filter.text = Some("MYP".to_string());
        assert_eq!(
            ids_of(&page(&conn, &query, 0, 10).unwrap()),
            vec![id],
            "大小写不敏感"
        );
    }

    #[test]
    fn quotes_and_punctuation_do_not_break_the_fts_query() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                rel: "photos/海边 \"日落\" 照片.jpg",
                ..Spec::default()
            },
        );
        let mut query = Query::new(Scope::Repository);
        for text in ["\"日落\"", "a(b)c", "-x", "日落*"] {
            query.filter.text = Some(text.to_string());
            // 只要不报错就行（能不能搜到是另一回事）
            let _ = count(&conn, &query).unwrap();
        }
    }

    #[test]
    fn an_empty_or_whitespace_search_is_not_a_condition() {
        let conn = catalog();
        add(&conn, Spec::default());
        let mut query = Query::new(Scope::Repository);
        for text in ["", "   "] {
            query.filter.text = Some(text.to_string());
            assert!(query.filter.is_empty(), "{text:?} 不该成为条件");
            assert_eq!(count(&conn, &query).unwrap(), 1);
        }
    }

    // ---------- 分面 ----------

    #[test]
    fn facets_count_each_value_and_respect_the_scope() {
        let conn = catalog();
        add(
            &conn,
            Spec {
                rating: 3,
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rating: 3,
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rating: 0,
                color: Some("blue"),
                ..Spec::default()
            },
        );
        add(
            &conn,
            Spec {
                rel: "other/x.jpg",
                color: Some("blue"),
                ..Spec::default()
            },
        );

        let f = facets(&conn, &Query::new(Scope::subtree("photos"))).unwrap();
        assert_eq!(f.ratings, vec![(0, 1), (3, 2)]);
        assert_eq!(
            f.colors,
            vec![(None, 2), (Some("blue".to_string()), 1)],
            "分面只统计范围内的照片"
        );
    }

    // ---------- 纯函数 ----------

    #[test]
    fn file_name_of_takes_the_last_segment() {
        assert_eq!(file_name_of("photos/2026-08-15/a.jpg"), "a.jpg");
        assert_eq!(file_name_of("a.jpg"), "a.jpg");
        assert_eq!(file_name_of(""), "");
    }

    #[test]
    fn placeholders_count() {
        assert_eq!(placeholders(0), "");
        assert_eq!(placeholders(1), "?");
        assert_eq!(placeholders(3), "?, ?, ?");
    }

    #[test]
    fn fts_query_wraps_and_escapes_quotes() {
        assert_eq!(fts_query("婚礼"), "\"婚礼\"");
        assert_eq!(fts_query("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn combinator_names_are_stable() {
        assert_eq!(Combinator::And.as_str(), "and");
        assert_eq!(Combinator::Or.as_str(), "or");
        assert_eq!(Combinator::default(), Combinator::Or);
    }

    #[test]
    fn very_long_paths_and_names_do_not_blow_up() {
        let conn = catalog();
        let deep = format!("photos/{}/x.jpg", "深".repeat(120));
        add(
            &conn,
            Spec {
                rel: &deep,
                ..Spec::default()
            },
        );
        let rows = page(&conn, &Query::new(Scope::subtree("photos")), 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(file_name_of(&rows[0].rel_path), "x.jpg");
    }
}
