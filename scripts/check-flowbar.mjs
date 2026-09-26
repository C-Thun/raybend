/** Component smoke: real FlowBar/Solid/Ark with synthetic state, no photo/visual E2E. */
import assert from "node:assert/strict";
import { launchChrome, connectCdp, requireServer, sleep } from "./lib/cdp.mjs";

const url = process.argv[2] ?? "http://localhost:1420/";
await requireServer(url);
const chrome = launchChrome({ port: Number(process.env.CDP_PORT ?? 9529) });
let cdp;
try {
  cdp = await connectCdp(Number(process.env.CDP_PORT ?? 9529));
  const evaluate = async (expression) => {
    const result = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  };
  await cdp.send("Page.navigate", { url });
  const ready = 'document.querySelector("[data-part=item] input[value=import]") !== null';
  const deadline = Date.now() + 30_000;
  while (!(await evaluate(ready)) && Date.now() < deadline) await sleep(100);
  assert(await evaluate(ready), "FlowBar must mount");

  // Use Vite's exact module URLs so the fixture shares the application's Solid runtime.
  const [storeSource, flowSource] = await Promise.all([
    fetch(new URL("/src/shell/store.ts", url)).then((response) => response.text()),
    fetch(new URL("/src/shell/FlowBar.tsx", url)).then((response) => response.text()),
  ]);
  const solidUrl = storeSource.match(/from\s+["']([^"']*\/solid-js\.js[^"']*)["']/)?.[1];
  const webUrl = flowSource.match(/from\s+["']([^"']*\/solid-js_web\.js[^"']*)["']/)?.[1];
  assert(solidUrl && webUrl, "Run against pnpm dev (the fixture imports source components)");

  const results = await evaluate(`(async () => {
    const { createComponent, createSignal } = await import(${JSON.stringify(solidUrl)});
    const { render } = await import(${JSON.stringify(webUrl)});
    const { FlowBar } = await import('/src/shell/FlowBar.tsx');
    const { createShellStore } = await import('/src/shell/store.ts');
    const { WORKFLOWS, WORKFLOW_LABEL_KEY } = await import('/src/shell/flow.ts');
    const { locale, setLocale, t } = await import('/src/i18n/index.ts');
    const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
    const settle = async () => { await new Promise(requestAnimationFrame); await pause(220); };
    const checks = [];
    const originalLocale = locale();
    const originalDensity = document.documentElement.getAttribute('data-density');
    const roots = () => [...document.querySelectorAll('[data-scope=segment-group][data-part=root]')];
    const appFlow = roots().find(root => root.querySelector('input[value=import]'));
    const host = document.createElement('div');
    document.body.append(host);
    let dispose;
    const check = (name, root, expected, original) => {
      const items = [...root.querySelectorAll('[data-part=item]')];
      const checked = items.filter(item => item.dataset.state === 'checked');
      const indicators = [...root.querySelectorAll('[data-part=indicator]')];
      const indicator = indicators[0];
      const item = checked[0];
      const actual = item?.querySelector('input')?.value;
      const rect = indicator?.getBoundingClientRect();
      const target = item?.getBoundingClientRect();
      const sameNodes = !original || (original.root === root && original.indicator === indicator &&
        original.items.every((node, index) => items[index] === node));
      const aligned = rect && target && ['x', 'y', 'width', 'height'].every(key => Math.abs(rect[key] - target[key]) <= 1);
      checks.push({ name, ok: checked.length === 1 && actual === expected && indicators.length === 1 &&
        !indicator.hidden && rect.width > 0 && rect.height > 0 && aligned && sameNodes,
        actual, hidden: indicator?.hidden, rect: rect?.toJSON(), target: target?.toJSON(), sameNodes });
    };
    try {
      await settle();
      check('app startup', appFlow, appFlow.querySelector('[data-part=item][data-state=checked] input').value);
      let store, setProcessing;
      dispose = render(() => {
        store = createShellStore();
        // Queue snapshots invalidate processing() even when its boolean result stays false.
        const [processing, update] = createSignal(false, { equals: false });
        setProcessing = update;
        return createComponent(FlowBar, { store, get exportProcessing() { return processing(); } });
      }, host);
      const root = host.querySelector('[data-scope=segment-group][data-part=root]');
      const original = { root, indicator: root.querySelector('[data-part=indicator]'), items: [...root.querySelectorAll('[data-part=item]')] };
      await settle();
      check('initial import', root, 'import', original);
      const input = root.querySelector('input[value=import]');
      input.focus();
      for (const processing of [false, false, true, false]) {
        setProcessing(processing);
        await settle();
        check('queue refresh processing=' + processing, root, 'import', original);
        checks.push({ name: 'queue refresh preserves focus', ok: document.activeElement === input });
        checks.push({ name: 'export shimmer follows processing', ok: Boolean(root.querySelector('[data-flow-processing]')) === processing });
        if(processing)checks.push({name:'export icon participates in text shimmer',ok:root.querySelector('[data-flow-processing] svg')!==null});
      }
      for (const flow of ['browse', 'export', 'edit', 'import']) {
        store.setWorkflow(flow);
        setProcessing(flow === 'export');
        await settle();
        check('switch + queue refresh ' + flow, root, flow, original);
      }
      for (const language of ['en-US', 'zh-CN']) {
        setLocale(language);
        for (const density of ['loose', 'compact']) {
          document.documentElement.setAttribute('data-density', density);
          for (const flow of WORKFLOWS) {
            store.setWorkflow(flow);
            await settle();
            check(language + ' / ' + density + ' / ' + flow, root, flow, original);
          }
        }
        checks.push({ name: 'translated labels ' + language, ok: original.items.every((item, index) =>
          item.querySelector('[data-part=item-text]').textContent === t(WORKFLOW_LABEL_KEY[WORKFLOWS[index]])) });
      }
      for (let i = 0; i < 16; i++) {
        store.setWorkflow(WORKFLOWS[i % WORKFLOWS.length]);
        setProcessing(i % 2 === 0);
        await new Promise(requestAnimationFrame);
      }
      await settle();
      check('rapid switches settle', root, 'export', original);
      return checks;
    } finally {
      dispose?.();
      host.remove();
      setLocale(originalLocale);
      if (originalDensity === null) document.documentElement.removeAttribute('data-density');
      else document.documentElement.setAttribute('data-density', originalDensity);
    }
  })()`);
  const failed = results.filter((result) => !result.ok);
  console.log(JSON.stringify({ checks: results.length, failed, exceptions: cdp.exceptions, consoleErrors: cdp.consoleErrors }, null, 2));
  assert.equal(failed.length, 0, "FlowBar must keep its selected background and DOM nodes across state updates");
  assert.deepEqual(cdp.exceptions, []);
  assert.deepEqual(cdp.consoleErrors, []);
} finally {
  cdp?.close();
  chrome.kill("SIGKILL");
}
