import { A } from "@solidjs/router";

/**
 * 应用外壳骨架 —— 按 design/main.md §2 的三行结构实现。
 *
 * 本阶段的职责：
 *   1. 验证语义工具类（bg-surface-bar / text-fg-1 / h-bar-title-h …）确实生成且可切主题与密度
 *   2. 确立表面分层（DESIGN.md §2）：titlebar=bar / flowbar+toolsbar=main / 工作区两侧=main、中央=bar
 *   3. 确立「无边线设计」：块与块之间不加分隔线，只靠面色差
 *
 * 尚未实现（后续步骤）：菜单、主题/密度开关、窗口三键、EXIF 区、三列内容。
 */
export default function App() {
  return (
    <div class="flex h-full w-full flex-col bg-surface-main text-fg-1">
      {/* ── titlebar：沉浸式，无系统标题行 ───────────────────────── */}
      <header
        data-tauri-drag-region
        class="flex h-bar-title-h shrink-0 items-center gap-0 bg-surface-bar px-pad-x"
      >
        {/* 应用图标（占位：品牌色圆角块） */}
        <div class="flex size-5 items-center justify-center rounded-ui bg-brand">
          <span class="text-[11px] leading-none text-fg-on-brand">光</span>
        </div>
        <div class="w-2" />
        <span class="text-[13px] font-semibold">光伴</span>

        {/* 拖拽区：撑开中间空白，窗口由此拖动（双击自动最大化，免权限） */}
        <div class="h-px flex-1" />

        {/* 开发期入口：用三元而不是 `Show` —— 生产构建下整条分支会被摇掉（连字符串都不留） */}
        {import.meta.env.DEV ? (
          <A
            href="/dev/kitchen-sink"
            class="rounded-ui px-2 py-0.5 text-[11px] text-fg-2 hover:bg-state-hover hover:text-fg-1"
          >
            组件陈列室
          </A>
        ) : null}
      </header>

      {/* ── flowbar：工作流 + 图片信息 + 开关组 ──────────────────── */}
      <div class="flex h-[var(--bar-flow-h)] shrink-0 items-center bg-surface-main px-pad-x">
        <span class="text-[11px] text-fg-3">工作流切换（待实现）</span>
        <div class="h-px flex-1" />
        <span class="text-[11px] text-fg-2 tnum">EXIF 信息区（待实现）</span>
      </div>

      {/* ── toolsbar：无内容时整行隐藏 ───────────────────────────── */}
      <div class="flex h-[var(--bar-tool-h)] shrink-0 items-center justify-center bg-surface-main">
        <span class="text-[11px] text-fg-3">批量排除（待实现）</span>
      </div>

      {/* ── workspace：两侧 main / 中央 bar（中央聚焦，无需分隔线）── */}
      <div class="flex min-h-0 flex-1">
        <aside class="w-panel-w-left shrink-0 bg-surface-main p-panel-pad">
          <p class="text-[11px] tracking-wide text-fg-2 uppercase">来源</p>
        </aside>
        <main class="flex min-w-0 flex-1 items-center justify-center bg-surface-bar">
          <p class="text-fg-3">照片区（待实现）</p>
        </main>
        <aside class="w-panel-w-right shrink-0 bg-surface-main p-panel-pad">
          <p class="text-[11px] tracking-wide text-fg-2 uppercase">库</p>
        </aside>
      </div>
    </div>
  );
}
