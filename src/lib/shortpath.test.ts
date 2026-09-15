/**
 * `shortpath` 单元测试（DESIGN.md §12.3）。
 *
 * 运行：`pnpm test`（= `node --test`，Node 26 原生跑 TS，无需任何测试依赖）
 *
 * 覆盖重点按 AGENTS.md §2.10：
 *   空输入 / 单元素 / 上下限 / 非法值 / Unicode 与中文 / 超长 / 分隔符差异 / 不变式
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { shortPath } from "./shortpath.ts";

// ─── 退化输入 ──────────────────────────────────────────────

test("空字符串返回空", () => {
  assert.equal(shortPath(""), "");
});

test("非字符串输入不炸（防御 IPC 来的脏数据）", () => {
  assert.equal(shortPath(undefined as unknown as string), "");
  assert.equal(shortPath(null as unknown as string), "");
  assert.equal(shortPath(123 as unknown as string), "");
});

test("单级路径原样返回", () => {
  assert.equal(shortPath("Kyoto"), "Kyoto");
  assert.equal(shortPath("D:"), "D:");
  assert.equal(shortPath("/"), "/");
});

test("只有分隔符的路径不产生垃圾", () => {
  // 退化输入：不崩溃、不出现 U+FFFD、不凭空变长
  for (const s of ["\\\\\\", "///", "\\", "/"]) {
    const out = shortPath(s);
    assert.equal(typeof out, "string");
    assert.ok(!out.includes("\uFFFD"), `"${s}" → ${out}`);
    assert.ok(out.length <= s.length, `"${s}" 不应变长，实得：${out}`);
  }
});

// ─── Windows 规则：盘符 + 中间首字母 + 末级全名 ────────────

test("Windows 深层路径：除末级外全部取首字母", () => {
  assert.equal(
    shortPath("D:\\Photos\\2024\\Vacation\\Japan\\Kyoto"),
    "D:\\P\\2\\V\\J\\Kyoto",
  );
});

test("Windows 两级路径得到 盘符\\首字母\\末级", () => {
  assert.equal(shortPath("D:\\Photos\\Kyoto"), "D:\\P\\Kyoto");
});

test("Windows 末级含空格与点号时不被破坏", () => {
  assert.equal(
    shortPath("E:\\Work\\2024.03 Releases\\v1.2-final"),
    "E:\\W\\2\\v1.2-final",
  );
});

test("UNC 路径：prefix 保留 \\\\server\\share", () => {
  assert.equal(
    shortPath("\\\\nas\\photos\\2024\\Japan\\Kyoto"),
    "\\\\nas\\photos\\2\\J\\Kyoto",
  );
});

test("盘符大小写不被改变", () => {
  assert.equal(
    shortPath("d:\\a\\b\\c"),
    "d:\\a\\b\\c".replace("\\a\\b\\c", "\\a\\b\\c"),
  );
  assert.ok(shortPath("d:\\Photos\\Kyoto").startsWith("d:"));
});

// ─── POSIX 规则：第一级与最后一级写全 ─────────────────────

test("POSIX 保留首末两级全名", () => {
  assert.equal(
    shortPath("/home/andares/Pictures/Wallpapers"),
    "/home/a/P/Wallpapers",
  );
});

test("POSIX 两级路径无中间可缩", () => {
  assert.equal(shortPath("/home/andares"), "/home/andares");
});

test("POSIX 单级绝对路径原样", () => {
  assert.equal(shortPath("/mnt"), "/mnt");
});

test("相对路径按 POSIX 规则处理", () => {
  assert.equal(shortPath("photos/2024/japan/kyoto"), "photos/2/j/kyoto");
});

// ─── 第二层：仍超长则整体重缩 ─────────────────────────────

test("超过长度上限时触发第二层（末级也取首字母）", () => {
  const full = "D:\\Photos\\2024\\Vacation\\Japan\\Kyoto";
  const first = shortPath(full); // D:\P\2\V\J\Kyoto
  assert.equal(first, "D:\\P\\2\\V\\J\\Kyoto");
  assert.equal(first.length, 16);

  // 上限 12：第二层 D:\P\2\V\J\K 恰好 12 字符 —— 末级也被缩成了首字母（K 而非 S）
  const second = shortPath(full, { maxLength: 12 });
  assert.equal(second, "D:\\P\\2\\V\\J\\K");
  assert.ok(!second.endsWith("Kyoto"), `第二层应缩掉末级，实得：${second}`);
});

test("层级深时：先整体重缩，再塌成 ... 并优先保住两头", () => {
  // 浅路径上「保留两头」的分支永远轮不到（第二层比它还短），必须用深路径才能走到
  const deep = "D:\\a\\b\\c\\d\\e\\f\\g\\h\\Kyoto";

  // 第一层（只缩中间）：各级已是一字符，结果 24
  assert.equal(shortPath(deep).length, 24);
  // 第二层（整体重缩）：末级取首字母，结果 20
  assert.equal(
    shortPath(deep, { maxLength: 20 }),
    "D:\\a\\b\\c\\d\\e\\f\\g\\h\\K",
  );
  // 第三层：优先同时保住盘符与末级
  assert.equal(shortPath(deep, { maxLength: 14 }), "D:\\...\\Kyoto");
  // 再窄一档：盘符也保不住，只留末级
  assert.equal(shortPath(deep, { maxLength: 10 }), "...\\Kyoto");
});

test("末级名本身也放不下时，保留其开头并以单个 … 收尾", () => {
  const full = "D:\\Photos\\2024\\Vacation\\Japan\\SakuraMatsuriFestival";
  const out = shortPath(full, { maxLength: 10 });
  assert.equal(out, "SakuraMat\u2026");
  assert.ok(out.length <= 10, `实得 ${out.length} 字符：${out}`);
  assert.ok(!out.includes("\uFFFD"));
});

test("极小的 maxLength 也不返回空（下限保护）", () => {
  for (const max of [1, 2, 3, 4, 5]) {
    const out = shortPath("D:\\Photos\\2024\\Kyoto", { maxLength: max });
    assert.ok(out.length > 0, `maxLength=${max} 时不应为空`);
  }
});

test("maxLength 为 0 或负数视为「不限长度」", () => {
  const full = "D:\\Photos\\2024\\Vacation\\Japan\\Kyoto";
  assert.equal(shortPath(full, { maxLength: 0 }), shortPath(full));
  assert.equal(shortPath(full, { maxLength: -5 }), shortPath(full));
});

// ─── Unicode 与中文 ───────────────────────────────────────

test("中文目录名按首字缩写且不被截成半个字符", () => {
  const out = shortPath("D:\\照片\\2024年\\日本旅行\\京都");
  assert.equal(out, "D:\\照\\2\\日\\京都");
  // 不能出现 U+FFFD 替换字符
  assert.ok(!out.includes("\uFFFD"));
});

test("emoji 目录名按码点取首字符（不切代理对）", () => {
  const out = shortPath("D:\\Pictures\\📸Album\\Kyoto");
  assert.ok(out.startsWith("D:\\P\\"), `实得：${out}`);
  assert.ok(!out.includes("\uFFFD"), `不应出现半个码点：${out}`);
});

test("超长中文路径受 maxLength 约束", () => {
  const out = shortPath(
    "D:\\照片库\\二零二四年\\秋季旅行\\日本\\京都岚山竹林小径",
    { maxLength: 12 },
  );
  assert.ok(out.length <= 12, `期望 ≤12，实得 ${out.length}：${out}`);
  assert.ok(!out.includes("\uFFFD"));
});

// ─── 分隔符处理 ───────────────────────────────────────────

test("混合分隔符能正确切分且输出统一", () => {
  assert.equal(shortPath("D:\\Photos/2024\\Kyoto"), "D:\\P\\2\\Kyoto");
});

test("POSIX 路径里的反斜杠会被当作分隔符（Windows 规则优先）", () => {
  const out = shortPath("/home/andares\\Pictures\\Kyoto");
  assert.ok(out.includes("Kyoto"));
});

test("结尾分隔符不产生空段", () => {
  assert.equal(shortPath("D:\\Photos\\2024\\Kyoto\\"), "D:\\P\\2\\Kyoto");
  assert.equal(shortPath("/home/andares/Pictures/"), "/home/a/Pictures");
});

test("连续分隔符折叠", () => {
  assert.equal(
    shortPath("D:\\\\Photos\\\\\\\\2024\\\\Kyoto"),
    "D:\\P\\2\\Kyoto",
  );
  assert.equal(shortPath("/home//andares///Pictures/Kyoto"), "/home/a/P/Kyoto");
});

test("可强制指定分隔符", () => {
  const out = shortPath("a/b/c", { separator: "/" });
  assert.equal(out, "a/b/c".replace("a/b/c", "a/b/c"));
  assert.ok(out.includes("/"));
});

// ─── 不变式 ───────────────────────────────────────────────

test("不变式：给了 maxLength 就绝不超长", () => {
  const samples = [
    "D:\\Photos\\2024\\Vacation\\Japan\\Kyoto",
    "/home/andares/Pictures/Wallpapers/4K/Nature",
    "\\\\nas\\share\\a\\b\\c\\d\\e\\f",
    "D:\\照片库\\二零二四年\\秋季旅行\\日本\\京都岚山",
    "C:\\a",
    "/x",
  ];
  for (const s of samples) {
    for (const max of [1, 5, 8, 12, 16, 20, 30, 64]) {
      const out = shortPath(s, { maxLength: max });
      assert.ok(
        out.length <= max || out.length <= 1,
        `"${s}" @${max} 得到 ${out.length} 字符：${out}`,
      );
    }
  }
});

test("不变式：不产生连续分隔符（缩略后不出现 D:\\\\Kyoto 这类）", () => {
  const samples = [
    "D:\\Photos\\2024\\Kyoto",
    "/home/andares/Pictures/Kyoto",
    "\\\\nas\\share\\a\\b",
  ];
  for (const s of samples) {
    for (const max of [4, 8, 12, 20]) {
      const out = shortPath(s, { maxLength: max });
      assert.ok(
        !/[\\/]{2,}/.test(out.replace(/^\\\\/, "")),
        `"${s}" @${max} → ${out}`,
      );
    }
  }
});
