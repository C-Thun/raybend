//! 曲线：每通道一条**单调三次**曲线（`design/editor.md` §3.7 的曲线编辑器）。
//!
//! # 为什么是单调三次（Fritsch–Carlson）而不是自然三次 / Catmull-Rom
//!
//! 照片曲线的控制点常被拖成「几乎水平的平台」——普通三次样条在那种形状下会**过冲**
//! （曲线冲出 [0,1]，表现为画面里出现死黑/死白或者反转的亮暗）。Fritsch–Carlson 的
//! 切线限幅保证曲线在控制点之间**单调**，不会出现比相邻控制点更高或更低的取值。
//!
//! # 域与值域
//!
//! * `x` = 输入（显示变换之后的 0..1），`y` = 输出；两点都夹在 [0, 1]；
//! * 控制点按 `x` 严格递增排列；
//! * **首尾两个点可以左右拖**（人类 2026-09-24 定）—— 那就是黑场 / 白场：
//!   把首点拖到 x = 0.1 表示「0.1 以下全丢（都输出首点的 y）」，末点同理。
//!   曲线之外的部分取端点值（`eval` 里夹取），仍然是函数。
//!
//! # 顺序（管线里的语义）
//!
//! 先 RGB 合成曲线、再各通道曲线（`design/editor.md`：RGB 是「一起调」的那条）。

use super::params::spec;

/// 曲线通道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CurveChannel {
    /// 合成（RGB 一起）。
    Rgb,
    Red,
    Green,
    Blue,
}

impl CurveChannel {
    /// 稳定名字（进 DB、进 IPC、进日志 —— **不要改**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Red => "r",
            Self::Green => "g",
            Self::Blue => "b",
        }
    }

    /// 从稳定名字解析。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "rgb" => Some(Self::Rgb),
            "r" => Some(Self::Red),
            "g" => Some(Self::Green),
            "b" => Some(Self::Blue),
            _ => None,
        }
    }

    /// 四个通道（界面顺序）。
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [Self::Rgb, Self::Red, Self::Green, Self::Blue]
    }
}

/// 控制点的上限（界面拖不出这么多；防的是坏数据把内存撑爆）。
pub const MAX_POINTS: usize = 32;

/// 一条曲线。
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// 按 x 递增；x 在 [0,1] 内（首尾不要求正好是 0 / 1 —— 那是黑场/白场）。
    points: Vec<[f32; 2]>,
    /// 采样表（`eval` 的快路径；点一变就重算）。
    table: Vec<f32>,
}

/// 采样表长度（1025 段足够：曲线是低频的，1/1025 的线性插值误差远小于 1/255）。
const TABLE_LEN: usize = 1025;

/// 相邻控制点 x 的**最小间距**（拖点时的碰撞判据；也防坏数据里两个点叠在一起）。
pub const MIN_X_GAP: f32 = 1.0 / 256.0;

impl Default for Curve {
    fn default() -> Self {
        Self::identity()
    }
}

impl Curve {
    /// 恒等曲线（一条对角线）。
    #[must_use]
    pub fn identity() -> Self {
        Self::from_points(vec![[0.0, 0.0], [1.0, 1.0]]).expect("对角线一定合法")
    }

    /// 从控制点建（排序 + 校验 + 建表）。
    ///
    /// # Errors
    /// 点数不在 2..=[`MAX_POINTS`]、坐标非有限或超 [0,1]、或 x 有重复。
    pub fn from_points(mut points: Vec<[f32; 2]>) -> Result<Self, String> {
        if points.len() < 2 {
            return Err(format!("曲线至少要两个控制点，给了 {}", points.len()));
        }
        if points.len() > MAX_POINTS {
            return Err(format!(
                "曲线控制点最多 {MAX_POINTS} 个，给了 {}",
                points.len()
            ));
        }
        for point in &points {
            if !point[0].is_finite() || !point[1].is_finite() {
                return Err(format!("控制点必须是有限数：{point:?}"));
            }
            if !(0.0..=1.0).contains(&point[0]) || !(0.0..=1.0).contains(&point[1]) {
                return Err(format!("控制点必须落在 [0,1]×[0,1] 里：{point:?}"));
            }
        }
        points.sort_by(|a, b| a[0].partial_cmp(&b[0]).expect("已经排除 NaN"));
        for pair in points.windows(2) {
            if (pair[1][0] - pair[0][0]).abs() < MIN_X_GAP {
                return Err(format!(
                    "两个控制点的 x 太近（至少隔开 {MIN_X_GAP}）：{} 与 {}",
                    pair[0][0], pair[1][0]
                ));
            }
        }
        let mut curve = Self {
            points,
            table: Vec::new(),
        };
        curve.rebuild_table();
        Ok(curve)
    }

    /// 控制点（只读）。
    #[must_use]
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    /// 是恒等曲线吗（界面上「这条通道动过没有」）。
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.points == [[0.0, 0.0], [1.0, 1.0]]
    }

    /// 加一个控制点（返回新曲线；x 与已有点太近就返回 `Err`）。
    ///
    /// # Errors
    /// 见 [`Self::from_points`]。
    pub fn with_point(&self, x: f32, y: f32) -> Result<Self, String> {
        let mut points = self.points.clone();
        points.push([x, y]);
        Self::from_points(points)
    }

    /// 改一个控制点（`index` 越界返回 `Err`）。
    ///
    /// 首尾点也能左右拖 —— 那就是**黑场 / 白场**（把 0.1 以下或 0.9 以上的明度丢掉）。
    /// 但不能越过邻居（黑场跑到白场右边就不再是函数了）—— 越界直接报错，由界面拦住。
    ///
    /// # Errors
    /// 下标越界、越过邻居，或见 [`Self::from_points`]。
    pub fn with_moved_point(&self, index: usize, x: f32, y: f32) -> Result<Self, String> {
        if index >= self.points.len() {
            return Err(format!("控制点下标越界：{index}"));
        }
        if let Some(previous) = index.checked_sub(1).and_then(|i| self.points.get(i))
            && x <= previous[0] + MIN_X_GAP
        {
            return Err(format!("这个点会越过左边的控制点（至少隔开 {MIN_X_GAP}）"));
        }
        if let Some(next) = self.points.get(index + 1)
            && x >= next[0] - MIN_X_GAP
        {
            return Err(format!("这个点会越过右边的控制点（至少隔开 {MIN_X_GAP}）"));
        }
        let mut points = self.points.clone();
        points[index] = [x, y];
        Self::from_points(points)
    }

    /// 删一个控制点（首尾不许删）。
    ///
    /// # Errors
    /// 下标越界或想删首尾。
    pub fn without_point(&self, index: usize) -> Result<Self, String> {
        if index == 0 || index + 1 >= self.points.len() {
            return Err("首尾控制点不能删".to_string());
        }
        let mut points = self.points.clone();
        points.remove(index);
        Self::from_points(points)
    }

    /// 求值（表 + 线性插值）。
    #[must_use]
    pub fn eval(&self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        let x = x.clamp(0.0, 1.0);
        let scaled = x * (TABLE_LEN - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = scaled.floor() as usize;
        if index + 1 >= TABLE_LEN {
            return self.table[TABLE_LEN - 1];
        }
        let fraction = scaled - index as f32;
        let a = self.table[index];
        let b = self.table[index + 1];
        a + (b - a) * fraction
    }

    /// 采样表（建 LUT 用；只读）。
    #[must_use]
    pub fn table(&self) -> &[f32] {
        &self.table
    }

    /// 重新采样（点变了就调它）。
    fn rebuild_table(&mut self) {
        let tangents = self.tangents();
        let mut table = vec![0.0f32; TABLE_LEN];
        #[allow(clippy::cast_precision_loss)]
        let denominator = (TABLE_LEN - 1) as f32;
        for (index, slot) in table.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let x = index as f32 / denominator;
            *slot = self.eval_exact(x, &tangents);
        }
        self.table = table;
    }

    /// Fritsch–Carlson 切线。
    fn tangents(&self) -> Vec<f32> {
        let n = self.points.len();
        if n == 2 {
            let slope =
                (self.points[1][1] - self.points[0][1]) / (self.points[1][0] - self.points[0][0]);
            return vec![slope, slope];
        }
        let mut slopes = Vec::with_capacity(n - 1);
        for pair in self.points.windows(2) {
            slopes.push((pair[1][1] - pair[0][1]) / (pair[1][0] - pair[0][0]));
        }
        let mut tangents = vec![0.0f32; n];
        tangents[0] = slopes[0];
        tangents[n - 1] = slopes[n - 2];
        for i in 1..n - 1 {
            // 局部极值处必须水平；只缩短切线不能修正方向相反的过冲。
            tangents[i] = if slopes[i - 1] * slopes[i] <= 0.0 {
                0.0
            } else {
                (slopes[i - 1] + slopes[i]) / 2.0
            };
        }
        // 限幅：保证单调（Fritsch–Carlson 的条件）
        for i in 0..n - 1 {
            let delta = slopes[i];
            if delta.abs() < 1e-9 {
                tangents[i] = 0.0;
                tangents[i + 1] = 0.0;
                continue;
            }
            let alpha = tangents[i] / delta;
            let beta = tangents[i + 1] / delta;
            let magnitude = alpha * alpha + beta * beta;
            if magnitude > 9.0 {
                let tau = 3.0 / magnitude.sqrt();
                tangents[i] = tau * alpha * delta;
                tangents[i + 1] = tau * beta * delta;
            }
        }
        tangents
    }

    /// 精确求值（不查表；建表时用）。
    fn eval_exact(&self, x: f32, tangents: &[f32]) -> f32 {
        let n = self.points.len();
        if x <= self.points[0][0] {
            return self.points[0][1];
        }
        if x >= self.points[n - 1][0] {
            return self.points[n - 1][1];
        }
        // 找到 x 落在哪一段（点数很少，线性扫足够）
        let mut segment = 0usize;
        for index in 0..n - 1 {
            if x >= self.points[index][0] && x <= self.points[index + 1][0] {
                segment = index;
                break;
            }
        }
        let [x0, y0] = self.points[segment];
        let [x1, y1] = self.points[segment + 1];
        let h = x1 - x0;
        let t = (x - x0) / h;
        let t2 = t * t;
        let t3 = t2 * t;
        // Hermite 基函数
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        let value =
            h00 * y0 + h10 * h * tangents[segment] + h01 * y1 + h11 * h * tangents[segment + 1];
        value.clamp(0.0, 1.0)
    }

    /// 进 DB / IPC 的形态：`[[x, y], …]`（**归一化 0..1**，与界面坐标同一口径）。
    #[must_use]
    pub fn to_pairs(&self) -> Vec<[f32; 2]> {
        self.points.clone()
    }
}

/// 四个通道的曲线（管线里的一条参数）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CurveSet {
    pub rgb: Curve,
    pub r: Curve,
    pub g: Curve,
    pub b: Curve,
}

impl CurveSet {
    /// 全恒等（= 没调过曲线）。
    #[must_use]
    pub fn identity() -> Self {
        Self {
            rgb: Curve::identity(),
            r: Curve::identity(),
            g: Curve::identity(),
            b: Curve::identity(),
        }
    }

    /// 取一条通道。
    #[must_use]
    pub fn channel(&self, channel: CurveChannel) -> &Curve {
        match channel {
            CurveChannel::Rgb => &self.rgb,
            CurveChannel::Red => &self.r,
            CurveChannel::Green => &self.g,
            CurveChannel::Blue => &self.b,
        }
    }

    /// 设一条通道。
    pub fn set_channel(&mut self, channel: CurveChannel, curve: Curve) {
        match channel {
            CurveChannel::Rgb => self.rgb = curve,
            CurveChannel::Red => self.r = curve,
            CurveChannel::Green => self.g = curve,
            CurveChannel::Blue => self.b = curve,
        }
    }

    /// 全恒等吗（决定要不要走曲线快路径）。
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.rgb.is_identity()
            && self.r.is_identity()
            && self.g.is_identity()
            && self.b.is_identity()
    }

    /// 动过的通道（DB 只存这些）。
    #[must_use]
    pub fn dirty_channels(&self) -> Vec<CurveChannel> {
        CurveChannel::all()
            .into_iter()
            .filter(|channel| !self.channel(*channel).is_identity())
            .collect()
    }
}

/// 参数表里有「曲线」这一项吗 —— 没有（曲线是独立模型）。这条断言把这件事钉住，
/// 免得以后有人顺手往 `develop-params.json` 里加一个 `curve`。
#[must_use]
pub fn curve_is_not_a_param() -> bool {
    spec("curve").is_none()
}

/// 与 TS 侧共用的**外部给定测试向量**（见 `src/lib/curve-vectors.json`）。
///
/// 前端也要画这条曲线（拖动的每一帧），所以求值有两份实现 ——
/// 这份文件是它们的**共同基准**：两侧都对着同一组采样值断言，公式一改漏一处就红。
#[cfg(test)]
const CURVE_VECTORS_JSON: &str = include_str!("../../../../src/lib/curve-vectors.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_matches_the_shared_test_vectors() {
        // 与 `src/lib/curve.test.ts` 对着**同一份**向量断言（跨语言不许漂）
        let document: serde_json::Value =
            serde_json::from_str(CURVE_VECTORS_JSON).expect("向量文件必须能解析");
        let cases = document["cases"].as_array().expect("cases 是数组");
        assert!(!cases.is_empty(), "向量文件不能是空的");
        for case in cases {
            let points: Vec<[f32; 2]> = case["points"]
                .as_array()
                .expect("points")
                .iter()
                .map(|point| {
                    let pair = point.as_array().expect("点是一对坐标");
                    [
                        pair[0].as_f64().expect("x") as f32,
                        pair[1].as_f64().expect("y") as f32,
                    ]
                })
                .collect();
            let steps = case["steps"].as_u64().expect("steps") as usize;
            let samples: Vec<f32> = case["samples"]
                .as_array()
                .expect("samples")
                .iter()
                .map(|value| value.as_f64().expect("采样值") as f32)
                .collect();
            assert_eq!(samples.len(), steps + 1, "采样点数与 steps 对不上");

            let curve = Curve::from_points(points.clone()).expect("向量里的曲线必须合法");
            for (index, expected) in samples.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let x = index as f32 / steps as f32;
                let got = curve.eval(x);
                assert!(
                    (got - expected).abs() < 5e-4,
                    "控制点 {points:?} 在 x={x} 处：向量给 {expected}，本实现算 {got}"
                );
            }
        }
    }

    #[test]
    fn identity_curve_is_the_diagonal() {
        let curve = Curve::identity();
        assert!(curve.is_identity());
        for step in 0..=10 {
            let x = step as f32 / 10.0;
            assert!((curve.eval(x) - x).abs() < 1e-4, "恒等曲线在 {x} 处偏了");
        }
    }

    #[test]
    fn moved_endpoints_are_dirty_on_every_channel() {
        for channel in CurveChannel::all() {
            for points in [vec![[0.1, 0.0], [1.0, 1.0]], vec![[0.0, 0.0], [0.9, 1.0]]] {
                let mut set = CurveSet::identity();
                set.set_channel(channel, Curve::from_points(points).unwrap());
                assert!(!set.is_identity(), "移动黑白场必须算编辑过");
                assert_eq!(set.dirty_channels(), vec![channel]);
            }
        }
    }

    #[test]
    fn turning_points_do_not_overshoot_neighboring_values() {
        for points in [
            vec![[0.0, 0.2], [0.25, 0.8], [0.5, 0.7], [1.0, 0.9]],
            vec![[0.0, 0.8], [0.25, 0.2], [0.5, 0.3], [1.0, 0.1]],
        ] {
            let curve = Curve::from_points(points).unwrap();
            for pair in curve.points().windows(2) {
                let lo = pair[0][1].min(pair[1][1]);
                let hi = pair[0][1].max(pair[1][1]);
                for step in 0..=100 {
                    let x = pair[0][0] + (pair[1][0] - pair[0][0]) * step as f32 / 100.0;
                    let y = curve.eval(x);
                    assert!(y >= lo - 1e-5 && y <= hi + 1e-5, "{pair:?}, {x} → {y}");
                }
            }
        }
    }

    #[test]
    fn eval_is_monotone_and_stays_in_range() {
        // 一个刻意难看的形状：平台 + 陡坡 + 平台（普通三次样条会在这里过冲）
        let curve = Curve::from_points(vec![
            [0.0, 0.0],
            [0.25, 0.05],
            [0.5, 0.5],
            [0.75, 0.95],
            [1.0, 1.0],
        ])
        .expect("合法曲线");
        let mut previous = -1.0f32;
        for step in 0..=1000 {
            let x = step as f32 / 1000.0;
            let y = curve.eval(x);
            assert!((0.0..=1.0).contains(&y), "越界：{x} → {y}");
            assert!(
                y >= previous - 1e-4,
                "不单调：{x} → {y}（前一个 {previous}）"
            );
            previous = y;
        }
    }

    #[test]
    fn control_points_are_interpolated_exactly() {
        let curve = Curve::from_points(vec![[0.0, 0.1], [0.3, 0.4], [0.7, 0.6], [1.0, 0.95]])
            .expect("合法曲线");
        for point in curve.points() {
            let y = curve.eval(point[0]);
            assert!((y - point[1]).abs() < 2e-3, "控制点 {point:?} 处得到 {y}");
        }
    }

    #[test]
    fn endpoints_are_the_black_and_white_points() {
        // 人类 2026-09-24：端点可以**左右拖**（丢明度范围），不是钉死在 0 / 1
        let curve = Curve::identity();
        let lifted = curve.with_moved_point(0, 0.12, 0.0).expect("黑场右移");
        assert!((lifted.points()[0][0] - 0.12).abs() < 1e-6);
        assert_eq!(lifted.eval(0.0), 0.0, "黑场以下全丢（输出端点的 y）");
        assert_eq!(lifted.eval(0.06), 0.0);
        assert!(lifted.eval(0.2) > 0.0, "黑场之上要有值");
        let both = lifted
            .with_moved_point(lifted.points().len() - 1, 0.9, 1.0)
            .expect("白场左移");
        assert!((both.points()[both.points().len() - 1][0] - 0.9).abs() < 1e-6);
        assert_eq!(both.eval(1.0), 1.0, "白场以上取端点的 y");
        // 越过邻居要报错（曲线必须还是函数）
        assert!(both.with_moved_point(0, 0.95, 0.0).is_err());
    }

    #[test]
    fn add_and_remove_points() {
        let curve = Curve::identity().with_point(0.5, 0.7).expect("加点");
        assert_eq!(curve.points().len(), 3);
        assert!(!curve.is_identity());
        let back = curve.without_point(1).expect("删点");
        assert!(back.is_identity());
        assert!(curve.without_point(0).is_err(), "首点不能删");
        assert!(curve.without_point(2).is_err(), "末点不能删");
        assert!(curve.without_point(99).is_err(), "越界要报错");
    }

    #[test]
    fn bad_points_are_rejected() {
        assert!(Curve::from_points(vec![[0.0, 0.0]]).is_err(), "一个点不行");
        assert!(
            Curve::from_points(vec![[0.0, 0.0], [0.5, 1.5], [1.0, 1.0]]).is_err(),
            "y 超范围不行"
        );
        assert!(
            Curve::from_points(vec![[0.0, 0.0], [0.5, 0.5], [0.5, 0.6], [1.0, 1.0]]).is_err(),
            "x 重复不行"
        );
        assert!(
            Curve::from_points(vec![[0.0, 0.0], [0.001, 0.5], [1.0, 1.0]]).is_err(),
            "两个点挨得太近不行（至少要 MIN_X_GAP）"
        );
        let mut many = vec![[0.0f32, 0.0]];
        for index in 1..40 {
            many.push([index as f32 / 40.0, 0.5]);
        }
        many.push([1.0, 1.0]);
        assert!(Curve::from_points(many).is_err(), "点数上限要拦住");
    }

    #[test]
    fn table_and_exact_evaluation_agree() {
        let curve = Curve::from_points(vec![[0.0, 0.0], [0.2, 0.05], [0.55, 0.6], [1.0, 1.0]])
            .expect("合法曲线");
        let tangents = curve.tangents();
        for step in 0..=2000 {
            let x = step as f32 / 2000.0;
            let exact = curve.eval_exact(x, &tangents);
            let table = curve.eval(x);
            assert!(
                (exact - table).abs() < 1e-3,
                "{x} 处表值 {table} 与精确值 {exact} 差太多"
            );
        }
    }

    #[test]
    fn curve_set_tracks_dirty_channels() {
        let mut set = CurveSet::identity();
        assert!(set.is_identity());
        assert!(set.dirty_channels().is_empty());
        set.set_channel(
            CurveChannel::Blue,
            Curve::identity().with_point(0.5, 0.4).unwrap(),
        );
        assert!(!set.is_identity());
        assert_eq!(set.dirty_channels(), vec![CurveChannel::Blue]);
        assert!(curve_is_not_a_param());
    }

    #[test]
    fn channel_names_round_trip() {
        for channel in CurveChannel::all() {
            assert_eq!(CurveChannel::parse(channel.as_str()), Some(channel));
        }
        assert_eq!(CurveChannel::parse("x"), None);
    }
}
