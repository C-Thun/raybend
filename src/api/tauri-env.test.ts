/**
 * `detectRuntime` 的单元测试。
 *
 * 覆盖：空环境 / 两种标记 / 「有键无值」的注入竞态 / 标记被显式置空 / 陌生环境的噪声键。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { detectRuntime, TAURI_MARKERS } from "./tauri-env.ts";

test("没有 scope（SSR / 异常环境）→ browser", () => {
 assert.equal(detectRuntime(undefined), "browser");
 assert.equal(detectRuntime(null), "browser");
});

test("空对象 → browser", () => {
 assert.equal(detectRuntime({}), "browser");
});

test("Tauri 2 的标记（__TAURI_INTERNALS__）→ tauri", () => {
 assert.equal(detectRuntime({ __TAURI_INTERNALS__: {} }), "tauri");
});

test("Tauri 1 的标记（__TAURI__）也认 → tauri", () => {
 assert.equal(detectRuntime({ __TAURI__: {} }), "tauri");
});

test("标记存在但值为 null / undefined → browser（防注入顺序竞态）", () => {
 assert.equal(detectRuntime({ __TAURI_INTERNALS__: undefined }), "browser");
 assert.equal(detectRuntime({ __TAURI_INTERNALS__: null }), "browser");
});

test("标记值为各种真值都能认（对象 / 字符串 / true / 0 之外的数字）", () => {
 assert.equal(detectRuntime({ __TAURI_INTERNALS__: "yes" }), "tauri");
 assert.equal(detectRuntime({ __TAURI_INTERNALS__: true }), "tauri");
 assert.equal(detectRuntime({ __TAURI_INTERNALS__: 1 }), "tauri");
});

test("无关的全局键不会误判（浏览器里到处都是这种东西）", () => {
 assert.equal(detectRuntime({ __TAURI_DEV__: true, __VITE__: {} }), "browser");
});

test("标记清单是导出的常量，改一处即全局生效", () => {
 assert.deepEqual([...TAURI_MARKERS], ["__TAURI_INTERNALS__", "__TAURI__"]);
});
