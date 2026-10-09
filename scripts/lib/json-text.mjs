/**
 * 解析外部来源的 JSON（构建产物、gh 输出、node_modules 清单）：
 * 失败时点名来源，而不是只丢一个没有上下文的 SyntaxError。
 * 发布链上这些文件坏了必须一眼看出是谁坏了 —— 见 `specs/m5-crates-publish.md` 的配套说明。
 */
export function parseJson(text, label) {
  try { return JSON.parse(text); }
  catch (error) { throw new Error(`${label} 不是合法 JSON：${error.message}`); }
}
