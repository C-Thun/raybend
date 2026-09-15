/**
 * `copyText` 的单元测试（DESIGN.md §12.1 的底层）。
 *
 * 关键点：**空内容不能写剪贴板**（会清掉用户原有内容）、
 * 主通道失败要落到兜底通道、两条都失败要老实返回 false 而不是抛错。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { copyText } from "./clipboard.ts";

test("正常路径：写入成功 → true，且透传的文本一字不改", async () => {
  const written: string[] = [];
  const ok = await copyText("NIKON Z 7II · 24-70mm", {
    writeText: async (text) => {
      written.push(text);
    },
  });

  assert.equal(ok, true);
  assert.deepEqual(written, ["NIKON Z 7II · 24-70mm"]);
});

test("空串与 undefined 不写剪贴板（不清掉用户原有内容）", async () => {
  let called = 0;
  const deps = {
    writeText: async () => {
      called += 1;
    },
    fallback: () => {
      called += 1;
      return true;
    },
  };

  assert.equal(await copyText("", deps), false);
  assert.equal(await copyText(undefined as unknown as string, deps), false);
  assert.equal(called, 0);
});

test("多行与中文原样写入（EXIF 分组是多行文本）", async () => {
  const text = "机型：NIKON Z 7II\n镜头：NIKKOR Z 24-70mm f/2.8 S\n";
  let received = "";
  await copyText(text, {
    writeText: async (value) => {
      received = value;
    },
  });
  assert.equal(received, text);
});

test("主通道抛错 → 落到兜底通道", async () => {
  let fellBack = "";
  const ok = await copyText("D:\\照片", {
    writeText: async () => {
      throw new Error("不是安全上下文");
    },
    fallback: (text) => {
      fellBack = text;
      return true;
    },
  });

  assert.equal(ok, true);
  assert.equal(fellBack, "D:\\照片");
});

test("主通道不存在（旧环境）→ 直接用兜底", async () => {
  let used = false;
  const ok = await copyText("payload", {
    fallback: () => {
      used = true;
      return true;
    },
  });
  assert.equal(ok, true);
  assert.equal(used, true);
});

test("两条通道都失败 → false（不抛错，由界面决定不显示「已复制」）", async () => {
  const ok = await copyText("payload", {
    writeText: async () => {
      throw new Error("拒绝");
    },
    fallback: () => false,
  });
  assert.equal(ok, false);
});

test("兜底通道自己抛错 → 依然是 false，不把异常抛给界面", async () => {
  const ok = await copyText("payload", {
    fallback: () => {
      throw new Error("execCommand 在无 DOM 环境不可用");
    },
  });
  assert.equal(ok, false);
});
