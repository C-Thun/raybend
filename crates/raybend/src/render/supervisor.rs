//! **长期 GPU worker 的监督策略**（M3-W2）。
//!
//! `AGENTS.md` §7.9 记着本仓最贵的一次事故：渲染线程里一次 `create_bind_group` panic
//! **把线程打死了** —— 主线程照旧响应、窗口照旧拖、图永远冻住，而且日志里什么都没有。
//! 所以「渲染线程」这个东西不能是一个裸 `thread::spawn` 的循环，它得有兜底：
//!
//! ```text
//!   建上下文失败（窗口还没好 / 没有适配器）
//!   渲染线程 panic      ──▶  记一笔 + 等一下 + 重建 + 再来
//!   设备丢失且恢复失败
//! ```
//!
//! 三件事共用一套策略：**试几次、每次等多久、什么时候认输**。
//! 策略是纯逻辑（不碰线程、不碰 wgpu），于是在这里单测；线程循环只负责「照着判决做」。
//!
//! ## 为什么「成功一次就清零」不够
//!
//! 「起来 → 立刻崩 → 起来 → 立刻崩」这种循环如果每轮都算一次成功，计数器永远回零、
//! 进程就永远在里面打转。所以清零的条件是**跑稳了**（连续活了 `stable_after` 那么久），
//! 由线程循环用时间判定后调 [`RestartPolicy::on_stable`] —— 策略本身不碰时钟，好测。

use std::time::Duration;

/// 一次失败之后该干什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 再来一次：等 `delay` 之后重建（`attempt` 从 1 起数）
    Retry { attempt: u32, delay: Duration },
    /// 试够了，认输（线程退出，把错误写进状态让界面上报）
    GiveUp,
}

/// 重启策略（默认：5 次、250ms 起、最长 5s）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    attempts: u32,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::new(5, Duration::from_millis(250), Duration::from_secs(5))
    }
}

impl RestartPolicy {
    /// `max_attempts = 0` 会被抬到 1 —— 「一次都不试」不是一种策略，是配置错误。
    #[must_use]
    pub fn new(max_attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            base_delay,
            max_delay: max_delay.max(base_delay),
            attempts: 0,
        }
    }

    /// 已经重启过几次（界面上的「重启次数」就是它）。
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// 一次失败之后的判决。
    pub fn on_failure(&mut self) -> Verdict {
        if self.attempts >= self.max_attempts {
            return Verdict::GiveUp;
        }
        self.attempts += 1;
        Verdict::Retry {
            attempt: self.attempts,
            delay: self.delay_for(self.attempts),
        }
    }

    /// 跑稳了（连续活了足够久）—— 计数清零，等于「这是一次正常的长跑，不算在试错预算里」。
    pub fn on_stable(&mut self) {
        self.attempts = 0;
    }

    /// 第 `attempt` 次的退避时长：指数增长（250ms → 500ms → 1s → …）并夹到上限。
    ///
    /// 指数而不是固定值：启动失败通常发生在「窗口/驱动还没就绪」这种短暂状态，
    /// 快速重试一次就可能过；但如果是真没有适配器，重试太密只会刷屏。
    #[must_use]
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(16);
        // 用「翻倍 16 次就停」而不是 `base * 2^n` 直接乘：`Duration * 2^16` 会溢出 panic
        let mut delay = self.base_delay;
        for _ in 0..shift {
            delay = delay.saturating_mul(2);
            if delay >= self.max_delay {
                return self.max_delay;
            }
        }
        delay.min(self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RestartPolicy {
        RestartPolicy::new(3, Duration::from_millis(100), Duration::from_secs(2))
    }

    #[test]
    fn retries_up_to_the_budget_then_gives_up() {
        let mut policy = policy();
        assert_eq!(
            policy.on_failure(),
            Verdict::Retry {
                attempt: 1,
                delay: Duration::from_millis(100)
            }
        );
        assert_eq!(policy.attempts(), 1);
        assert!(matches!(policy.on_failure(), Verdict::Retry { attempt: 2, .. }));
        assert!(matches!(policy.on_failure(), Verdict::Retry { attempt: 3, .. }));
        assert_eq!(policy.on_failure(), Verdict::GiveUp, "第 4 次就认输");
        assert_eq!(policy.attempts(), 3, "认输不该再涨计数");
        // 认输之后继续问也还是认输（幂等，别让调用方多试一次）
        assert_eq!(policy.on_failure(), Verdict::GiveUp);
    }

    #[test]
    fn a_stable_run_clears_the_budget() {
        let mut policy = policy();
        let _ = policy.on_failure();
        let _ = policy.on_failure();
        policy.on_stable();
        assert_eq!(policy.attempts(), 0);
        assert!(matches!(policy.on_failure(), Verdict::Retry { attempt: 1, .. }));
    }

    #[test]
    fn backoff_grows_exponentially_and_stops_at_the_ceiling() {
        let policy = RestartPolicy::new(20, Duration::from_millis(250), Duration::from_secs(5));
        assert_eq!(policy.delay_for(0), Duration::from_millis(250), "0 也按第一次算");
        assert_eq!(policy.delay_for(1), Duration::from_millis(250));
        assert_eq!(policy.delay_for(2), Duration::from_millis(500));
        assert_eq!(policy.delay_for(3), Duration::from_secs(1));
        assert_eq!(policy.delay_for(4), Duration::from_secs(2));
        assert_eq!(policy.delay_for(5), Duration::from_secs(4));
        assert_eq!(policy.delay_for(6), Duration::from_secs(5), "夹到上限");
        assert_eq!(policy.delay_for(64), Duration::from_secs(5), "再远也不溢出");
    }

    #[test]
    fn zero_budget_is_raised_to_one_instead_of_never_trying() {
        let mut policy = RestartPolicy::new(0, Duration::from_millis(10), Duration::from_millis(20));
        assert!(matches!(policy.on_failure(), Verdict::Retry { attempt: 1, .. }));
        assert_eq!(policy.on_failure(), Verdict::GiveUp);
    }

    #[test]
    fn ceiling_below_base_does_not_panic() {
        // 配置写反（上限比起点还小）时按起点走，而不是 panic 或返回 0
        let policy = RestartPolicy::new(2, Duration::from_secs(1), Duration::from_millis(1));
        assert_eq!(policy.delay_for(3), Duration::from_secs(1));
    }
}
