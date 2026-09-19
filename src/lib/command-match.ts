/**
 * 命令面板的**模糊匹配**（`plans/M2-W3.md` §2.2）。
 *
 * 自己写（不引第三方库）：规则就三条 —— 前缀、词首、子序列 —— 但每一条都要可解释、可测，
 * 引一个库反而要读它的打分函数才知道为什么某条命令排第一。
 *
 * ## 打分规则（从高到低）
 *
 * | 情况 | 分 |
 * | --- | --- |
 * | 标题完全相等 | `1000` |
 * | 标题以它开头 | `800 − 多出来的长度`（越短越靠前） |
 * | 子序列命中（标题 / id / 分组） | 逐字符累加：连续命中 +5/字、落在词首 +8、起点越靠前越好 |
 * | 最近用过 | **额外 +（最多 20）**，只用来打破接近的平局 |
 *
 * 中文没有「词首」概念（`isWordStart` 只看 ASCII 边界与 `·/ /-` 这类分隔符），
 * 所以中文查询走的是纯子序列 —— 「按时间」搜「时间」能中，「筛」能中「筛选开关」。
 */

export interface CommandSearchItem {
  id: string;
  /** 已本地化的标题（界面语言变了，匹配的就是新语言） */
  title: string;
  /** 已本地化的分组名（用于分组显示；也参与匹配） */
  group?: string;
}

/** 这个位置算不算「词的开始」（ASCII 字母数字块的开头，或紧跟分隔符之后） */
function isWordStart(text: string, index: number): boolean {
  if (index === 0) return true;
  const previous = text[index - 1];
  if (/[\s·/\\|,.\-_:]/.test(previous)) return true;
  // 小写 → 大写 的驼峰边界（id 里常见：byTime / zoomIn）
  return /[a-z]/.test(previous) && /[A-Z]/.test(text[index]);
}

/** 子序列打分；`-1` = 不匹配 */
export function subsequenceScore(query: string, text: string): number {
  if (query === "") return 0;
  let score = 0;
  let streak = 0;
  let matched = 0;
  let firstIndex = -1;
  for (let index = 0; index < text.length && matched < query.length; index += 1) {
    if (text[index] === query[matched]) {
      if (firstIndex < 0) firstIndex = index;
      streak += 1;
      matched += 1;
      score += 10 + streak * 5 + (isWordStart(text, index) ? 8 : 0);
    } else {
      streak = 0;
    }
  }
  if (matched < query.length) return -1;
  // 起点越靠前越好（最多扣 20 分，别盖过命中质量本身）
  return score - Math.min(firstIndex, 20);
}

/**
 * 一条命令对查询的匹配分；`null` = 不匹配（面板里不显示）。
 * 空查询 = 人人都匹配（分 0，顺序交给调用方）。
 */
export function scoreCommand(query: string, item: CommandSearchItem): number | null {
  const needle = query.trim().toLowerCase();
  if (needle === "") return 0;

  const title = item.title.toLowerCase();
  if (title === needle) return 1000;
  if (title.startsWith(needle)) return 800 - Math.min(title.length - needle.length, 100);

  const titleScore = subsequenceScore(needle, title);
  const idScore = subsequenceScore(needle, item.id.toLowerCase());
  const groupScore = item.group === undefined ? -1 : subsequenceScore(needle, item.group.toLowerCase());

  const best = Math.max(
    titleScore,
    idScore < 0 ? -1 : idScore * 0.7,
    groupScore < 0 ? -1 : groupScore * 0.5,
  );
  return best < 0 ? null : best;
}

/** 最近用过的加分（越靠前加得越多，最多 20 —— 只打破接近的平局） */
function recentBoost(recentIds: readonly string[], id: string): number {
  const at = recentIds.indexOf(id);
  if (at < 0) return 0;
  return Math.max(0, 20 - at * 3);
}

/**
 * 排序结果。
 *
 * * 空查询：**最近使用优先**（按最近顺序），其余保持传入顺序（注册表顺序 = 人工编排的顺序）；
 * * 有查询：按分数（含最近加分）降序，同分保持传入顺序（稳定）。
 */
export function rankCommands<T extends CommandSearchItem>(
  query: string,
  items: readonly T[],
  recentIds: readonly string[] = [],
): T[] {
  const needle = query.trim();
  if (needle === "") {
    const recentRank = new Map(recentIds.map((id, index) => [id, index]));
    return [...items].sort((a, b) => {
      const ra = recentRank.get(a.id);
      const rb = recentRank.get(b.id);
      if (ra === undefined && rb === undefined) return 0;
      if (ra === undefined) return 1;
      if (rb === undefined) return -1;
      return ra - rb;
    });
  }
  const scored: { item: T; index: number; total: number }[] = [];
  items.forEach((item, index) => {
    const score = scoreCommand(needle, item);
    if (score === null) return;
    scored.push({ item, index, total: score + recentBoost(recentIds, item.id) });
  });
  scored.sort((a, b) => b.total - a.total || a.index - b.index);
  return scored.map((entry) => entry.item);
}
