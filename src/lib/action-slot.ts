/** 动作槽的注册状态也必须响应更新：外壳控件常先于工作区 onMount 读取它。 */
import { createSignal, type Accessor } from "solid-js";

export function createActionSlot<T>(): {
  read: Accessor<T | null>;
  register: (actions: T | null) => void;
} {
  const [read, set] = createSignal<T | null>(null);
  return { read, register: (actions) => { set(() => actions); } };
}
