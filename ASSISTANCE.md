# 待你做的事（ASSISTANCE.md）

> 这里**只放「现在就必须由你做、不做会卡住我」的事**，一条一句话。
> 做完告诉我，我验证后**直接从本文件删掉** —— 不写总结、不留归档（历史在 `implementations/` 与 git log）。
> **越短越好，空着就是最好的状态。** 未来才做的（验收、拍板、授权）不属于这里。

## 一、现在需要你做（阻塞中）

（当前：**无**）

## 二、绕过去了的问题（攒着等外援）

> **这一节不阻塞任何工作**：每条都写清了「怎么绕的」与「风险是什么」，
> 所以可以攒着 —— 直到某一条**再也绕不过去**（继续走会踩空），它才升到上面「一、现在需要你做」。
> 修好之后同样**直接删掉**（历史在 `implementations/` 与 git log）。
>
> 规则见 `AGENTS.md` §2.12。

1. **中文在小控件里看着偏高（0.5–0.8px）** —— 已做度量修正（`vite.config.ts` 的
   `cjkMetricsOverride` 把 CJK 字体的 ascent/descent 改成方块字 em 盒，实测已生效），
   但**像素偏移一点没变**，判断是光栅化/基线吸格（本环境测不出来源）。
   → 你在真机上看一眼：**如果还是偏高就说一声**，那条改动一条 revert 就能撤
   （涉及三处：`vite.config.ts` 的插件、`src/index.tsx` 的字体 import、`scripts/ui-smoke.mjs` 的两条断言）。
   量法与数据见 `implementations/2026-09-17_exclude-chain_radius_language_font-metrics.md` §二。

2. **`cargo test` 偶发链接失败**：`rust-lld: error: undefined hidden symbol ... .llvm.*` ——
   增量编译的陈旧目标文件所致（本次遇到两次，都是改完代码之后）。
   绕法：`cargo clean -p raybend` 后重跑即可（约 1 分钟）。
   若你觉得太频繁，我可以把 `[profile.test]` 的 `incremental` 关掉（代价是每次改完重编稍慢）。

---

不属于这里的：设计取舍 → 各文档的「待决」小节；目视/真机验收 → `PLAN.md` 的「人类验收清单」；
工具约束 → `design/main.md` §6.1。
