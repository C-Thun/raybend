/**
 * flowbar 提示区的状态机：**一个位置多条来源**。
 *
 * 单测的价值在于两条容易写错的语义：
 * * 后指到的覆盖前一条（鼠标只能指一个地方）；
 * * **离开只清自己那条** —— A 离开时不能把 B 刚提的提示一起吃掉
 *   （否则「先划过控制点、再划过别处」会看到提示闪一下就没了）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  clearFlowHint,
  flowHint,
  flowHintsEnabled,
  setFlowHintsEnabled,
  showFlowHint,
} from "./flow-hint.ts";

test("提示区：显示、覆盖、只清自己那条", () => {
  assert.equal(flowHint(), null, "没指任何东西时不显示");

  showFlowHint("a", "按住 Ctrl 键可拖动");
  assert.equal(flowHint(), "按住 Ctrl 键可拖动");

  // 同一条重复 show 不改变结果（也不会把别的东西挤掉）
  showFlowHint("a", "按住 Ctrl 键可拖动");
  assert.equal(flowHint(), "按住 Ctrl 键可拖动");

  // 后指到的覆盖前一条
  showFlowHint("b", "拖边缘可改宽度");
  assert.equal(flowHint(), "拖边缘可改宽度");

  // A 离开：B 那条还在
  clearFlowHint("a");
  assert.equal(flowHint(), "拖边缘可改宽度", "不是自己那条就不许清");

  // B 离开：清掉
  clearFlowHint("b");
  assert.equal(flowHint(), null);

  // 同一 owner 换文案（例如同一个标记在不同状态下的说明）
  showFlowHint("a", "一");
  showFlowHint("a", "二");
  assert.equal(flowHint(), "二");
  clearFlowHint("a");
  assert.equal(flowHint(), null);
});

test("开关：关掉后不再显示，且正在显示的那条会被清掉", () => {
  // 默认是开着的
  assert.equal(flowHintsEnabled(), true, "默认应该开着（崔总 2026-11-02）");

  showFlowHint("a", "按住 Ctrl 键可拖动");
  assert.equal(flowHint(), "按住 Ctrl 键可拖动");

  setFlowHintsEnabled(false);
  assert.equal(flowHintsEnabled(), false);
  assert.equal(flowHint(), null, "关掉时立刻清掉当前那条");

  /*
   * 关着的时候调用点照常 `showFlowHint(...)`（调用点不需要知道开关）：
   * 这一下**直接丢掉**，不排队 —— 否则重新打开时会把「关着时指过的地方」的提示
   * 凭空弹出来（指针早已移开）。
   */
  showFlowHint("a", "拖边缘可改宽度");
  assert.equal(flowHint(), null, "关着时不显示");

  setFlowHintsEnabled(true);
  assert.equal(flowHintsEnabled(), true);
  assert.equal(flowHint(), null, "重新打开不会补发关着时丢掉的提示");

  // 打开后照常工作
  showFlowHint("a", "拖边缘可改宽度");
  assert.equal(flowHint(), "拖边缘可改宽度");
  clearFlowHint("a");
  assert.equal(flowHint(), null);
});
