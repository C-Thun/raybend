/** 原生 u64 十进制 revision；非法输入排序在任何有效版本之前。 */
export function revision(value: string): bigint {
  return /^\d+$/.test(value) ? BigInt(value) : -1n;
}
