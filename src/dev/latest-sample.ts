/** 限流保留尾样本：连续输入合并，停手之后最后一个位置也一定会送出。 */
export function latestSample<T>(
  send: (sample: T) => void,
  schedule: (callback: () => void) => () => void,
) {
  let pending: { value: T } | undefined;
  let cancel: (() => void) | undefined;
  let disposed = false;
  const flush = () => {
    cancel?.();
    cancel = undefined;
    const sample = pending;
    pending = undefined;
    if (!disposed && sample) send(sample.value);
  };
  return {
    push(value: T) {
      if (disposed) return;
      pending = { value };
      cancel ??= schedule(flush);
    },
    flush,
    dispose() {
      disposed = true;
      cancel?.();
      cancel = undefined;
      pending = undefined;
    },
  };
}
