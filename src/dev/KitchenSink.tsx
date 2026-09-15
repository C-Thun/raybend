/**
 * 组件陈列室（DESIGN.md §10.1 / plans/M1.md §4.2.1）。
 *
 * 目的：把 16 个基础组件的**全部状态 × 两主题 × 两密度**一眼铺开，
 * 供人（不是 Agent）与 `design/main.pen` 逐项比对。`AGENTS.md` §2.8：
 * 视觉正确性属 E2E，归人类目视确认；Agent 只保证「能渲染、不报错」。
 *
 * 仅开发期可达（路由在 `src/index.tsx` 里用 `import.meta.env.DEV` 守卫）。
 *
 * 关于本页里的中文：**不进语言包**。它是开发期工具，不是产品界面；
 * 混进 `i18n/*.ts` 只会让语言包的 key 与真实界面文案混在一起。
 * 产品界面的文案仍然零硬编码（§11.1）。
 */

import { createEffect, createSignal, For, onMount, Show, type JSX } from "solid-js";
import {
  IconAlertTriangle,
  IconCircleCheck,
  IconClock,
  IconCopy,
  IconFolder,
  IconFolderFilled,
  IconInfoCircle,
  IconPhoto,
  IconPlus,
} from "@tabler/icons-solidjs";
import { Splitter as ArkSplitter } from "@ark-ui/solid";
import { t } from "../i18n";
import { LOCALE_IDS, locale, setLocale, type LocaleId } from "../i18n";
import { applyAppearance, DEFAULT_APPEARANCE, writeAppearance } from "../lib/appearance";
import { Badge, CountBadge } from "../components/ui/Badge";
import { Button, IconButton } from "../components/ui/Button";
import { Checkbox, Input, RadioCircle, Switch } from "../components/ui/Form";
import { SegmentedControl } from "../components/ui/SegmentedControl";
import { Tooltip } from "../components/ui/Tooltip";
import { Menu } from "../components/ui/Menu";
import { ConfirmDialog, Dialog } from "../components/ui/Dialog";
import { PathText } from "../components/ui/PathText";
import { Tile } from "../components/ui/Tile";
import { TreeNode } from "../components/ui/TreeNode";
import { Panel } from "../components/ui/Panel";
import { SplitHandle, SplitHandleDots } from "../components/ui/SplitHandle";
import { ScrollBox } from "../components/ui/ScrollBar";
import { ToggleBlock } from "../components/ui/ToggleBlock";
import { RemoveButton } from "../components/ui/RemoveButton";
import { EasyCopy } from "../components/ui/EasyCopy";
import { EasyDestroyButton } from "../components/ui/EasyDestroy";

/* ── 布局小工具（只服务于本页） ───────────────────────────── */

function Section(props: { title: string; note?: string; children: JSX.Element }) {
  return (
    <section class="flex flex-col gap-2 pb-6">
      <div>
        <h2 class="text-[13px] font-semibold text-fg-1">{props.title}</h2>
        <Show when={props.note}>
          <p class="text-[11px] text-fg-3">{props.note}</p>
        </Show>
      </div>
      <div class="flex flex-col gap-2">{props.children}</div>
    </section>
  );
}

function Row(props: { label: string; children: JSX.Element }) {
  return (
    <div class="flex items-start gap-4">
      <span class="w-36 shrink-0 pt-1 text-[11px] text-fg-3">{props.label}</span>
      <div class="flex min-w-0 flex-1 flex-wrap items-center gap-2">
        {props.children}
      </div>
    </div>
  );
}

/* ── 页面 ─────────────────────────────────────────────────── */

export default function KitchenSink() {
  const [theme, setTheme] = createSignal<"dark" | "light">(DEFAULT_APPEARANCE.theme);
  const [density, setDensity] = createSignal<"compact" | "loose">(
    DEFAULT_APPEARANCE.density,
  );

  const [checked, setChecked] = createSignal(true);
  const [radio, setRadio] = createSignal(true);
  const [switchOn, setSwitchOn] = createSignal(false);
  const [flow, setFlow] = createSignal<"import" | "browse" | "edit" | "export">(
    "import",
  );
  const [toggleSnap, setToggleSnap] = createSignal(false);
  const [toggleByTime, setToggleByTime] = createSignal(true);
  const [dialogOpen, setDialogOpen] = createSignal(false);
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const [collapsed, setCollapsed] = createSignal(false);
  const [selectedTile, setSelectedTile] = createSignal(1);
  const [expandedNodes, setExpandedNodes] = createSignal<string[]>(["d", "d/photo"]);
  const [selectedNode, setSelectedNode] = createSignal("d/photo");
  const [checkedNodes, setCheckedNodes] = createSignal<string[]>(["d/photo/2024"]);
  const [removedBars, setRemovedBars] = createSignal<string[]>([]);

  /**
   * 演示用图片：从**令牌**里取色现场生成一个 SVG data URL。
   * 这样既不违反「色值只出现在 tokens.css」的纪律，又能让 tile 的「有图」状态
   * 真的可被眼睛验证（而不是永远看着占位方块）。
   */
  const [demoImage, setDemoImage] = createSignal("");
  const buildDemoImage = () => {
    const style = getComputedStyle(document.documentElement);
    const brand = style.getPropertyValue("--brand").trim();
    const onBrand = style.getPropertyValue("--fg-on-brand").trim();
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="300" height="200">
      <rect width="300" height="200" fill="${brand}"/>
      <circle cx="150" cy="88" r="44" fill="${onBrand}" opacity="0.28"/>
      <rect x="60" y="150" width="180" height="14" rx="7" fill="${onBrand}" opacity="0.18"/>
    </svg>`;
    setDemoImage(`data:image/svg+xml,${encodeURIComponent(svg)}`);
  };
  onMount(buildDemoImage);

  // 主题/密度落到 <html> 上（令牌选择器由此生效），同时重算演示图
  createEffect(() => {
    const appearance = { theme: theme(), density: density() };
    applyAppearance(appearance);
    writeAppearance(appearance);
    // 主题换了 → 演示图的取色也要跟着换
    if (typeof document !== "undefined") buildDemoImage();
  });

  const treeChecked = (id: string) => checkedNodes().includes(id);

  return (
    <div class="flex h-full flex-col bg-surface-main text-fg-1">
      {/* ── 顶部控制条：主题 / 密度 / 语言 ─────────────────── */}
      <header class="flex h-bar-title-h shrink-0 items-center gap-4 bg-surface-bar px-pad-x">
        <span class="text-[13px] font-semibold">组件陈列室</span>
        <span class="text-[11px] text-fg-3">
          对齐 design/main.pen 逐项目视比对（Agent 只保证能跑）
        </span>
        <div class="h-px flex-1" />

        <SegmentedControl
          label="主题"
          value={theme()}
          onValueChange={setTheme}
          options={[
            { value: "dark", label: "深色" },
            { value: "light", label: "浅色" },
          ]}
        />
        <SegmentedControl
          label={t("titlebar.density.compact")}
          value={density()}
          onValueChange={setDensity}
          options={[
            { value: "compact", label: t("titlebar.density.compact") },
            { value: "loose", label: t("titlebar.density.loose") },
          ]}
        />
        <SegmentedControl
          label="语言"
          value={locale()}
          onValueChange={(value) => setLocale(value as LocaleId)}
          options={LOCALE_IDS.map((id) => ({ value: id, label: id }))}
        />
      </header>

      <ScrollBox class="px-6 py-4">
        <div class="mx-auto flex max-w-5xl flex-col">
          {/* ── 表面分层与前景 ───────────────────────────── */}
          <Section
            title="表面分层与前景（DESIGN.md §2 / §4）"
            note="四级面 track < main < bar < layer；前景 fg-1/2/3 + fg-on-brand"
          >
            <div class="flex flex-wrap gap-3">
              <div class="flex h-20 w-40 flex-col justify-center rounded-ui bg-surface-track px-3">
                <span class="text-[11px] text-fg-2">surface-track</span>
                <span class="text-[11px] text-fg-3">凹槽轨道</span>
              </div>
              <div class="flex h-20 w-40 flex-col justify-center rounded-ui bg-surface-main px-3">
                <span class="text-[11px] text-fg-2">surface-main</span>
                <span class="text-[11px] text-fg-3">基础面</span>
              </div>
              <div class="flex h-20 w-40 flex-col justify-center rounded-ui bg-surface-bar px-3">
                <span class="text-[11px] text-fg-2">surface-bar</span>
                <span class="text-[11px] text-fg-3">第二层</span>
              </div>
              <div class="flex h-20 w-40 flex-col justify-center rounded-ui bg-surface-layer px-3">
                <span class="text-[11px] text-fg-2">surface-layer</span>
                <span class="text-[11px] text-fg-3">浮层</span>
              </div>
            </div>
            <Row label="文字与状态">
              <span class="text-[12px] text-fg-1">fg-1 正文</span>
              <span class="text-[12px] text-fg-2">fg-2 次级</span>
              <span class="text-[12px] text-fg-3">fg-3 注释</span>
              <span class="rounded-ui bg-brand px-2 py-0.5 text-[12px] text-fg-on-brand">
                on-brand
              </span>
              <span class="rounded-ui bg-state-hover px-2 py-0.5 text-[12px]">
                state-hover 辅色底
              </span>
              <span class="rounded-ui bg-state-selected px-2 py-0.5 text-[12px]">
                state-selected 主色底
              </span>
            </Row>
          </Section>

          {/* ── Button / IconButton ──────────────────────── */}
          <Section
            title="Button / IconButton（#1 / #2）"
            note="指向=辅色底，点击后=主色底；禁用只降前景不动面；加载中宽度不跳"
          >
            <Row label="primary / secondary / ghost">
              <Button variant="primary">导入</Button>
              <Button variant="secondary">取消</Button>
              <Button variant="ghost">更多</Button>
            </Row>
            <Row label="选中 / 带图标 / 尺寸">
              <Button selected>已选</Button>
              <Button icon={<IconPlus size={14} />}>新建库</Button>
              <Button size="sm" variant="secondary">
                小号
              </Button>
            </Row>
            <Row label="加载 / 禁用">
              <Button variant="primary" loading>
                导入中
              </Button>
              <Button disabled>禁用</Button>
              <Button variant="primary" disabled>
                禁用主按钮
              </Button>
            </Row>
            <Row label="IconButton 方 / 圆">
              <IconButton label="复制">
                <IconCopy size={14} />
              </IconButton>
              <IconButton label="新建" shape="circle">
                <IconPlus size={14} />
              </IconButton>
              <IconButton label="已选中" selected>
                <IconCircleCheck size={14} />
              </IconButton>
              <IconButton label="禁用" disabled>
                <IconPlus size={14} />
              </IconButton>
            </Row>
            <Row label="RemoveButton（禁行图标）">
              <RemoveButton label="移除" />
              <span class="rounded-ui bg-state-selected px-2 py-1">
                <RemoveButton label="选中底上的移除" />
              </span>
              <span class="rounded-ui bg-state-hover px-2 py-1">
                <RemoveButton label="悬停底上的移除" />
              </span>
            </Row>
          </Section>

          {/* ── 表单原语 ─────────────────────────────────── */}
          <Section title="Checkbox / RadioCircle / Switch / Input（#3 / #4 / #6）">
            <Row label="Checkbox">
              <Checkbox
                checked={checked()}
                onCheckedChange={setChecked}
                label="包含子目录"
              />
              <Checkbox checked={false} onCheckedChange={() => {}} label="未勾选" />
              <Checkbox
                checked
                onCheckedChange={() => {}}
                label="禁用"
                disabled
              />
            </Row>
            <Row label="RadioCircle">
              <RadioCircle checked={radio()} onCheckedChange={setRadio} label="勾选" />
              <RadioCircle checked={false} onCheckedChange={() => {}} label="未勾选" />
              <RadioCircle
                checked
                onCheckedChange={() => {}}
                label="禁用"
                disabled
              />
            </Row>
            <Row label="Switch">
              <Switch
                checked={switchOn()}
                onCheckedChange={setSwitchOn}
                label="包含子目录"
              />
              <Switch checked onCheckedChange={() => {}} label="开（禁用）" disabled />
            </Row>
            <Row label="Input">
              <Input placeholder="库名称" class="w-40" />
              <Input value="已填写" class="w-40" />
              <Input placeholder="非法值" invalid class="w-40" />
              <Input placeholder="禁用" disabled class="w-40" />
            </Row>
          </Section>

          {/* ── SegmentedControl / ToggleBlock ───────────── */}
          <Section
            title="SegmentedControl（#5）与 ToggleBlock（§12.8）"
            note="分段控件=单选凹槽+移动色块；ToggleBlock=按下式模式按钮（吸附/按时间）"
          >
            <Row label="工作流（单选）">
              <SegmentedControl
                label="工作流"
                value={flow()}
                onValueChange={setFlow}
                options={[
                  { value: "import", label: t("flow.import") },
                  { value: "browse", label: t("flow.browse") },
                  { value: "edit", label: t("flow.edit") },
                  { value: "export", label: t("flow.export") },
                ]}
              />
            </Row>
            <Row label="带图标 / 禁用项">
              <SegmentedControl
                label="带图标"
                value={flow()}
                onValueChange={setFlow}
                options={[
                  {
                    value: "import",
                    label: t("flow.import"),
                    icon: <IconFolder size={14} />,
                  },
                  {
                    value: "browse",
                    label: t("flow.browse"),
                    icon: <IconPhoto size={14} />,
                  },
                  { value: "edit", label: t("flow.edit"), disabled: true },
                ]}
              />
            </Row>
            <Row label="ToggleBlock">
              <ToggleBlock
                pressed={toggleSnap()}
                onPressedChange={setToggleSnap}
                icon={<IconAlertTriangle size={16} />}
                label={t("flow.tool.snap")}
              />
              <ToggleBlock
                pressed={toggleByTime()}
                onPressedChange={setToggleByTime}
                icon={<IconClock size={16} />}
              >
                {t("grid.by_time")}
              </ToggleBlock>
              <ToggleBlock
                pressed={false}
                onPressedChange={() => {}}
                icon={<IconClock size={16} />}
                label="禁用"
                disabled
              />
            </Row>
          </Section>

          {/* ── Badge ────────────────────────────────────── */}
          <Section title="Badge / CountBadge（#16）" note="计数必须 tabular-nums">
            <Row label="Badge">
              <Badge>已排除</Badge>
              <Badge tone="brand" icon={<IconCircleCheck size={12} />}>
                当前
              </Badge>
              <Badge tone="accent">点缀</Badge>
            </Row>
            <Row label="CountBadge">
              <CountBadge count={1248} />
              <CountBadge count={0} />
              <CountBadge count={999999} max={999} />
              <CountBadge count={1248} locale="en-US" tone="brand" />
            </Row>
          </Section>

          {/* ── Tooltip / Menu / Dialog ─────────────────── */}
          <Section
            title="Tooltip（#7）/ Menu（#8）/ Dialog（#9）"
            note="浮层底 surface-layer；无阴影、无毛玻璃（§6）"
          >
            <Row label="Tooltip">
              <Tooltip content="提示气泡">
                {(triggerProps) => (
                  <Button {...triggerProps()} variant="secondary">
                    悬停我
                  </Button>
                )}
              </Tooltip>
              <Tooltip content="下方弹出" placement="bottom">
                {(triggerProps) => (
                  <Button {...triggerProps()} variant="ghost">
                    下方
                  </Button>
                )}
              </Tooltip>
            </Row>
            <Row label="Menu">
              <Menu
                label="帮助"
                items={[
                  { value: "about", label: t("titlebar.menu.help.about"), icon: <IconInfoCircle size={14} /> },
                  {
                    value: "docs",
                    label: "文档",
                    separatorBefore: true,
                    selected: true,
                  },
                  { value: "disabled", label: "禁用项", disabled: true },
                ]}
                onSelect={() => {}}
              >
                {(triggerProps) => (
                  <Button {...triggerProps()} variant="secondary">
                    {t("titlebar.menu.help")}
                  </Button>
                )}
              </Menu>
            </Row>
            <Row label="Dialog / ConfirmDialog">
              <Button variant="secondary" onClick={() => setDialogOpen(true)}>
                打开弹窗
              </Button>
              <Button variant="secondary" onClick={() => setConfirmOpen(true)}>
                打开确认框
              </Button>
            </Row>
          </Section>

          {/* ── PathText ─────────────────────────────────── */}
          <Section
            title="PathText（#10）"
            note="shortpath 缩写 + 悬停给完整路径；maxLength 由调用方决定"
          >
            <Row label="不同长度上限">
              <span class="w-64">
                <PathText
                  path="D:\\Photos\\2024\\Vacation\\Japan\\Kyoto"
                  maxLength={18}
                  icon={<IconFolder size={14} />}
                />
              </span>
              <span class="w-64">
                <PathText
                  path="D:\\Photos\\2024\\Vacation\\Japan\\Kyoto"
                  maxLength={30}
                  icon={<IconFolderFilled size={14} />}
                />
              </span>
              <span class="w-64">
                <PathText path="D:\\Photos\\2024" full icon={<IconFolder size={14} />} />
              </span>
            </Row>
            <Row label="POSIX 与新路径">
              <span class="w-64">
                <PathText path="/home/andares/Pictures/Wallpapers" maxLength={22} />
              </span>
              <span class="w-64">
                <PathText path="" />
              </span>
            </Row>
          </Section>

          {/* ── Tile ────────────────────────────────────── */}
          <Section
            title="Tile（#11）"
            note="尺寸由 --tile-cell-w/h 决定（JS 可改）；Space=选中、Enter=打开"
          >
            <Row label="有图 / 选中 / 悬停">
              <Tile
                label="DSC_0192.NEF"
                sublabel="NEF · 45.7 MP"
                src={demoImage()}
                selected={selectedTile() === 1}
                onClick={() => setSelectedTile(1)}
              />
              <Tile
                label="DSC_0193.NEF"
                sublabel="NEF · 45.7 MP"
                src={demoImage()}
                selected={selectedTile() === 2}
                onClick={() => setSelectedTile(2)}
              />
            </Row>
            <Row label="加载 / 空 / 图标 / 禁用">
              <Tile label="DSC_0194.NEF" loading />
              <Tile label="损坏文件" empty src={demoImage()} />
              <Tile label="文件夹" icon={<IconFolderFilled size={20} />} />
              <Tile label="不可用" sublabel="离线卷" disabled />
            </Row>
          </Section>

          {/* ── TreeNode ────────────────────────────────── */}
          <Section
            title="TreeNode（#12）"
            note="勾选（圈）与选中（整行主色底）是两套独立状态；展开只跟用户操作有关"
          >
            <div class="w-full max-w-md rounded-ui bg-surface-main p-1">
              <TreeNode label="D:\\" depth={0} hasChildren expanded={expandedNodes().includes("d")} icon={<IconFolder size={14} />}
                onToggleExpand={() =>
                  setExpandedNodes((prev) =>
                    prev.includes("d") ? prev.filter((x) => x !== "d") : [...prev, "d"],
                  )
                }
                onClick={() => setSelectedNode("d")}
                selected={selectedNode() === "d"}
              />
              <TreeNode label="Photos" depth={1} hasChildren expanded={expandedNodes().includes("d/photo")} icon={<IconFolder size={14} />}
                onToggleExpand={() =>
                  setExpandedNodes((prev) =>
                    prev.includes("d/photo")
                      ? prev.filter((x) => x !== "d/photo")
                      : [...prev, "d/photo"],
                  )
                }
                onClick={() => setSelectedNode("d/photo")}
                selected={selectedNode() === "d/photo"}
              />
              <TreeNode label="2024" depth={2} hasChildren={false} icon={<IconFolder size={14} />}
                checked={treeChecked("d/photo/2024")}
                checkLabel="勾选 2024"
                onCheckedChange={() =>
                  setCheckedNodes((prev) =>
                    prev.includes("d/photo/2024")
                      ? prev.filter((x) => x !== "d/photo/2024")
                      : [...prev, "d/photo/2024"],
                  )
                }
                onClick={() => setSelectedNode("d/photo/2024")}
                selected={selectedNode() === "d/photo/2024"}
                trailing="1 248"
              />
              <TreeNode label="2025" depth={2} hasChildren={false} icon={<IconFolder size={14} />}
                checked={false}
                checkLabel="勾选 2025"
                onCheckedChange={() => {}}
                onClick={() => setSelectedNode("d/photo/2025")}
                selected={selectedNode() === "d/photo/2025"}
                trailing="76"
              />
              <TreeNode label="只读卷" depth={1} icon={<IconFolder size={14} />} disabled />
            </div>
          </Section>

          {/* ── Panel / SplitHandle / ScrollBar ─────────── */}
          <Section
            title="Panel（#13）/ SplitHandle（#14）/ ScrollBar（#15）"
            note="三段拖拽用 Ark Splitter 承载（选型的两大理由之一，这里必须真跑一次）"
          >
            <Row label="Panel（折叠 / 空态）">
              <div class="flex h-48 w-72 flex-col rounded-ui bg-surface-main p-1">
                <Panel
                  title={t("source.recent")}
                  collapsible
                  collapsed={collapsed()}
                  onCollapsedChange={setCollapsed}
                  scroll
                  actions={
                    <IconButton label="更多">
                      <IconPlus size={14} />
                    </IconButton>
                  }
                >
                  <For each={["D:\\Photos", "E:\\2024", "//nas/photos", "D:\\照片\\2024 秋"]}>
                    {(path) => (
                      <PathText
                        path={path}
                        maxLength={24}
                        icon={<IconFolder size={14} />}
                        class="h-row-h"
                      />
                    )}
                  </For>
                </Panel>
              </div>
              <div class="flex h-48 w-72 flex-col rounded-ui bg-surface-main p-1">
                <Panel title={t("repo.title")} scroll empty={t("repo.empty")} />
              </div>
            </Row>

            <Row label="三段式 + Splitter">
              <ArkSplitter.Root
                orientation="vertical"
                panels={[
                  { id: "recent", minSize: 15 },
                  { id: "source", minSize: 20 },
                  { id: "selected", minSize: 15 },
                ]}
                defaultSize={[25, 45, 30]}
                class="flex h-64 w-80 flex-col overflow-hidden rounded-ui bg-surface-main p-1"
              >
                <ArkSplitter.Panel id="recent" class="min-h-0 overflow-hidden">
                  <Panel title={t("source.recent")} scroll>
                    <For each={["D:\\Photos", "E:\\2024"]}>
                      {(path) => (
                        <PathText path={path} maxLength={20} icon={<IconFolder size={14} />} class="h-row-h" />
                      )}
                    </For>
                  </Panel>
                </ArkSplitter.Panel>
                <ArkSplitter.ResizeTrigger
                  id="recent:source"
                  class="group/split flex h-2 shrink-0 items-center justify-center hover:bg-state-hover data-[state=dragging]:bg-state-selected"
                >
                  <SplitHandleDots />
                </ArkSplitter.ResizeTrigger>
                <ArkSplitter.Panel id="source" class="min-h-0 overflow-hidden">
                  <Panel title={t("source.tree")} scroll>
                    <For each={["D:\\", "E:\\", "//nas/photos", "C:\\Users\\me\\Pictures"]}>
                      {(path) => (
                        <TreeNode
                          label={path}
                          depth={0}
                          icon={<IconFolder size={14} />}
                          onClick={() => {}}
                        />
                      )}
                    </For>
                  </Panel>
                </ArkSplitter.Panel>
                <ArkSplitter.ResizeTrigger
                  id="source:selected"
                  class="group/split flex h-2 shrink-0 items-center justify-center hover:bg-state-hover data-[state=dragging]:bg-state-selected"
                >
                  <SplitHandleDots />
                </ArkSplitter.ResizeTrigger>
                <ArkSplitter.Panel id="selected" class="min-h-0 overflow-hidden">
                  <Panel title={t("source.selected")} scroll>
                    <For each={["D:\\Photos", "E:\\2024", "//nas/photos", "D:\\照片\\2024 秋"]}>
                      {(path) => (
                        <Show when={!removedBars().includes(path)}>
                          <div class="flex h-selected-bar-h items-center gap-2 rounded-ui bg-surface-track px-2">
                            <IconFolder size={20} />
                            <PathText path={path} class="min-w-0 flex-1" />
                            <EasyDestroyButton
                              label={`移除 ${path}`}
                              onRemove={() => {
                                setRemovedBars((prev) => [...prev, path]);
                              }}
                            />
                          </div>
                        </Show>
                      )}
                    </For>
                  </Panel>
                </ArkSplitter.Panel>
              </ArkSplitter.Root>

              <div class="flex h-64 w-40 flex-col">
                <SplitHandle />
                <SplitHandle orientation="vertical" class="h-full" />
              </div>

              <div class="h-64 w-56 rounded-ui bg-surface-main p-2">
                <ScrollBox class="h-full">
                  <For each={Array.from({ length: 40 }, (_, i) => i + 1)}>
                    {(n) => (
                      <div class="flex h-row-h items-center gap-2 px-2 text-[12px] text-fg-2 hover:bg-state-hover">
                        <IconPhoto size={14} />
                        <span class="tnum">DSC_{String(n).padStart(4, "0")}.NEF</span>
                      </div>
                    )}
                  </For>
                </ScrollBox>
              </div>
            </Row>
          </Section>

          {/* ── easy copy / easy destroy ─────────────────── */}
          <Section
            title="easy copy（§12.1）/ easy destroy（§12.2）"
            note="悬停出细边框 + 点击复制；点完弹「已复制」。移除默认确认，按住 Shift 跳过"
          >
            <Row label="easy copy（EXIF 三组）">
              <div class="flex flex-wrap items-center gap-4 text-[12px]">
                <EasyCopy value={"机型 NIKON Z 7II\n镜头 NIKKOR Z 24-70mm f/2.8 S"}>
                  <span class="text-fg-2">机型</span>
                  <span>NIKON Z 7II</span>
                  <span class="text-fg-2">镜头</span>
                  <span>NIKKOR Z 24-70mm f/2.8 S</span>
                </EasyCopy>
                <EasyCopy value="焦距 52mm · 光圈 f/4 · 快门 1/250s · ISO 400">
                  <span class="text-fg-2">焦距</span>
                  <span class="tnum">52mm</span>
                  <span class="text-fg-2">光圈</span>
                  <span class="tnum">f/4</span>
                  <span class="text-fg-2">快门</span>
                  <span class="tnum">1/250s</span>
                  <span class="text-fg-2">ISO</span>
                  <span class="tnum">400</span>
                </EasyCopy>
                <EasyCopy value="8256 × 5504 · 45.4 MP · NEF">
                  <span class="text-fg-2">尺寸</span>
                  <span class="tnum">8256 × 5504</span>
                </EasyCopy>
                <EasyCopy value="禁用状态" disabled>
                  <span class="text-fg-3">禁用（未选照片）</span>
                </EasyCopy>
              </div>
            </Row>
            <Row label="easy destroy">
              <EasyDestroyButton
                label="移除演示条目"
                onRemove={() => {
                  setRemovedBars((prev) => [...prev, "demo"]);
                }}
              />
              <span class="text-[11px] text-fg-3">
                普通点击弹确认；按住 Shift 点击直接移除
              </span>
            </Row>
          </Section>
        </div>
      </ScrollBox>

      {/* ── 弹窗 ─────────────────────────────────────────── */}
      <Dialog
        open={dialogOpen()}
        onOpenChange={setDialogOpen}
        title={t("repo.create")}
        description="弹窗只用浮层面 + 遮罩，不描边、不投影"
        footer={
          <>
            <Button variant="secondary" onClick={() => setDialogOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button variant="primary" onClick={() => setDialogOpen(false)}>
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <Input placeholder="库名称" class="w-full" />
      </Dialog>

      <ConfirmDialog
        open={confirmOpen()}
        message={t("common.easy_destroy.confirm")}
        onConfirm={() => setConfirmOpen(false)}
        onCancel={() => setConfirmOpen(false)}
      />
    </div>
  );
}
