/**
 * 「文件名自然序」比较器（`DESIGN.md` §12.7 的分组口径、人类 2026-09-17 定）。
 *
 * **为什么必须有它**：同一时间段里要按文件名排，而字符串比较给不出人期待的顺序 ——
 * `"P1000020" < "P1000019"`（逐字符比到第 5 位就分出了大小），而人期望
 * `P1000019` 在前。数字段必须按**数值**比。
 *
 * 规则（顺序即优先级）：
 *   1. 按「段」走：连续数字算一段、其余字符算一段，逐段比；
 *   2. 数字段按数值比（先去前导零；位数多的更大，位数相同再逐字符比）；
 *   3. 非数字段忽略大小写比（`A` 与 `a` 同权）；
 *   4. 全部相等时用**原始串**兜底，保证顺序确定（可断言，也免得排序在两次调用间漂移）。
 *
 * 只认 ASCII 数字：`P①`、全角数字这类不按数值处理（按普通字符比）——
 * 相机文件名一律是 ASCII，多余的花样只会让规则变得不可预测。
 *
 * 返回值语义与 `Array.prototype.sort` 的比较函数一致：负数 = `a` 在前。
 */
export function compareNatural(a: string, b: string): number {
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    const ca = a[i] as string;
    const cb = b[j] as string;
    const da = isDigit(ca);
    const db = isDigit(cb);

    if (da && db) {
      // 数字段：整段取出再比
      const sa = readDigits(a, i);
      const sb = readDigits(b, j);
      const byValue = compareDigits(sa, sb);
      if (byValue !== 0) return byValue;
      i += sa.length;
      j += sb.length;
      continue;
    }

    // 非数字段（或一边数字一边不是）：忽略大小写比一个字符
    const la = ca.toLowerCase();
    const lb = cb.toLowerCase();
    if (la !== lb) return la < lb ? -1 : 1;
    i += 1;
    j += 1;
  }

  // 一边走完：短的在前（`"P1" < "P12"` 这种前缀关系）
  if (i < a.length) return 1;
  if (j < b.length) return -1;
  // 只有大小写/前导零等差异时，用原始串兜底
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

function isDigit(ch: string): boolean {
  return ch >= "0" && ch <= "9";
}

function readDigits(text: string, from: number): string {
  let end = from;
  while (end < text.length && isDigit(text[end] as string)) end += 1;
  return text.slice(from, end);
}

/** 两个（纯数字）串按数值比：先比去掉前导零后的位数，再逐字符比。 */
function compareDigits(a: string, b: string): number {
  const sa = stripLeadingZeros(a);
  const sb = stripLeadingZeros(b);
  if (sa.length !== sb.length) return sa.length < sb.length ? -1 : 1;
  if (sa === sb) return 0;
  return sa < sb ? -1 : 1;
}

function stripLeadingZeros(digits: string): string {
  let start = 0;
  while (start < digits.length - 1 && digits[start] === "0") start += 1;
  return digits.slice(start);
}
