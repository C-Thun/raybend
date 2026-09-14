/**
 * 应用外壳（M0 阶段的最小骨架）。
 *
 * 这里只验证两件事：Tailwind 令牌生效、三栏布局基线可用。
 * 真正的界面在 UI 设计阶段产出 `.pen` 设计稿后再实现。
 *
 * 布局对应未来的工作面：左侧相片仓 / 文件夹树、中间视口（M0-2 之后
 * 由原生 wgpu 直绘的挖洞区域）、右侧元数据与直方图、底部状态栏与提示行。
 */
export default function App() {
  return (
    <div class="flex h-full w-full flex-col bg-ui-bg-1 text-fg-1">
      {/* 顶部：占位工具条（未来的命令面板与视图切换） */}
      <header class="flex h-9 shrink-0 items-center gap-2 border-b border-ui-line bg-ui-bg-2 px-3">
        <span class="text-[13px] font-medium">RayBend</span>
        <span class="text-[11px] text-fg-3">M0 骨架</span>
      </header>

      <div class="flex min-h-0 flex-1">
        {/* 左侧：占位面板 */}
        <aside class="w-56 shrink-0 border-r border-ui-line bg-ui-bg-2 p-3">
          <p class="text-[11px] tracking-wide text-fg-3 uppercase">相片仓</p>
        </aside>

        {/* 中间：空视口 */}
        <main class="flex min-w-0 flex-1 items-center justify-center bg-ui-bg-1">
          <p class="text-fg-3">视口（M0-2 渲染验证后接入）</p>
        </main>

        {/* 右侧：占位面板 */}
        <aside class="w-64 shrink-0 border-l border-ui-line bg-ui-bg-2 p-3">
          <p class="text-[11px] tracking-wide text-fg-3 uppercase">信息</p>
        </aside>
      </div>

      {/* 底部：状态栏 + 提示行（弱化菜单的关键承载） */}
      <footer class="flex h-6 shrink-0 items-center justify-between border-t border-ui-line bg-ui-bg-2 px-3 text-[11px] text-fg-3">
        <span>就绪</span>
        <span>M0 · 可行性验证与项目骨架</span>
      </footer>
    </div>
  );
}
