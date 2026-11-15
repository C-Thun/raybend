#!/usr/bin/env node
/**
 * 官网浏览器冒烟（零新增依赖：Node 自带 WebSocket + 本机 Chrome）。
 *
 * 先在一个终端跑 `pnpm serve`，再在另一个终端跑 `pnpm smoke:ui`。
 * 也可检查 dev server 或子路径预览：`pnpm smoke:ui http://localhost:3000/`。
 * Chrome 不在 PATH 时通过 CHROME_BIN 指定。
 */
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const baseUrl = new URL(process.argv[2] ?? 'http://localhost:4173/');
const profile = mkdtempSync(join(tmpdir(), 'raybend-website-smoke-'));
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let chrome;
let ws;
let nextId = 0;
let navigationCount = 0;
const pending = new Map();
const requestUrls = new Map();
const consoleErrors = [];
const exceptions = [];
const failedRequests = [];

const EN_HEADINGS = [
  'Why it is worth a look',
  'One complete workflow',
  'The details',
  'Open source, with the internals on the table',
  'Download',
  'Video tutorials',
];
const ZH_HEADINGS = [
  '为什么值得关注',
  '一条完整的工作流',
  '功能细节',
  '开源，也把底子摊开讲',
  '下载',
  '视频教程',
];

async function startChrome() {
  let resolveTarget;
  let rejectTarget;
  const target = new Promise((resolve, reject) => {
    resolveTarget = resolve;
    rejectTarget = reject;
  });
  const timeout = setTimeout(() => rejectTarget(new Error('Chrome 启动超时')), 10_000);

  chrome = spawn(
    process.env.CHROME_BIN ?? 'google-chrome',
    [
      '--headless=new',
      '--remote-debugging-port=0',
      `--user-data-dir=${profile}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--no-sandbox',
      '--disable-gpu',
      '--disable-background-networking',
      'about:blank',
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  );
  chrome.once('error', rejectTarget);
  chrome.stderr.on('data', (chunk) => {
    const match = String(chunk).match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)\//);
    if (match) {
      clearTimeout(timeout);
      resolveTarget(Number(match[1]));
    }
  });

  const port = await target;
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      const page = (await response.json()).find((entry) => entry.type === 'page');
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // Chrome 的 DevTools HTTP 端口可能稍晚于 stderr 就绪。
    }
    await sleep(100);
  }
  throw new Error('找不到 Chrome 页面目标');
}

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`CDP ${method} 超时`));
    }, 10_000);
    pending.set(id, { resolve, reject, timeout });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const response = await send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (response.result?.exceptionDetails) {
    throw new Error(response.result.exceptionDetails.text);
  }
  return response.result?.result?.value;
}

async function waitFor(expression, description) {
  const deadline = Date.now() + 10_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await evaluate(expression)) return;
    } catch (error) {
      // 导航换文档时 execution context 会短暂失效，等新 context 即可。
      lastError = error;
    }
    await sleep(100);
  }
  throw new Error(`等待超时：${description}${lastError ? `（${lastError.message}）` : ''}`);
}

async function waitForNavigation(action, description) {
  const before = navigationCount;
  await action();
  const deadline = Date.now() + 10_000;
  while (navigationCount === before && Date.now() < deadline) await sleep(50);
  assert.ok(navigationCount > before, `等待超时：${description}`);
}

async function clickLanguage(label) {
  const clicked = await evaluate(`(() => {
    const button = [...document.querySelectorAll('button')]
      .find((entry) => entry.textContent.trim() === ${JSON.stringify(label)});
    if (!button) return false;
    button.click();
    return true;
  })()`);
  assert.equal(clicked, true, `找不到语言按钮 ${label}`);
}

async function headings() {
  return evaluate(`[...document.querySelectorAll('h2')].map((node) => node.textContent.trim())`);
}

try {
  const target = await startChrome();
  ws = new WebSocket(target);
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('CDP WebSocket 连接超时')), 10_000);
    ws.addEventListener('open', () => {
      clearTimeout(timeout);
      resolve();
    }, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });
  ws.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const request = pending.get(message.id);
      pending.delete(message.id);
      clearTimeout(request.timeout);
      if (message.error) request.reject(new Error(JSON.stringify(message.error)));
      else request.resolve(message);
      return;
    }
    if (message.method === 'Page.frameNavigated' && !message.params.frame.parentId) {
      navigationCount++;
    }
    if (message.method === 'Network.requestWillBeSent') {
      requestUrls.set(message.params.requestId, message.params.request.url);
    }
    if (message.method === 'Network.loadingFailed' && message.params.errorText !== 'net::ERR_ABORTED') {
      failedRequests.push(`${requestUrls.get(message.params.requestId) ?? message.params.requestId}: ${message.params.errorText}`);
    }
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      consoleErrors.push(message.params.args.map((arg) => arg.value ?? arg.description ?? arg.type).join(' '));
    }
    if (message.method === 'Runtime.exceptionThrown') {
      exceptions.push(message.params.exceptionDetails.exception?.description ?? message.params.exceptionDetails.text);
    }
  });

  await send('Runtime.enable');
  await send('Page.enable');
  await send('Network.enable');
  await send('Emulation.setLocaleOverride', { locale: 'en-US' });
  await send('Emulation.setDeviceMetricsOverride', {
    width: 1440,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });

  await waitForNavigation(() => send('Page.navigate', { url: baseUrl.href }), '首页导航');
  await waitFor(`document.querySelectorAll('h2').length === 6 && !!document.querySelector('footer')`, '英文首页');
  // 下载区图片是懒加载的，先滚动到页尾再断言资源完整性。
  await evaluate(`window.scrollTo(0, document.body.scrollHeight)`);
  await sleep(300);
  await evaluate(`window.scrollTo(0, 0)`);
  await waitFor(`[...document.images].every((image) => image.complete && image.naturalWidth > 0)`, '首页图片');
  const home = await evaluate(`(() => ({
    title: document.title,
    lang: document.documentElement.lang,
    sections: document.querySelectorAll('section').length,
    stylesheets: document.styleSheets.length,
    fonts: document.fonts.size,
    brokenImages: [...document.images]
      .filter((image) => !image.complete || image.naturalWidth === 0)
      .map((image) => image.currentSrc || image.src),
  }))()`);
  assert.match(home.title, /^RayBend /);
  assert.equal(home.lang, 'en');
  assert.equal(home.sections, 7);
  assert.ok(home.stylesheets > 0, '样式表未加载');
  assert.ok(home.fonts > 0, '自托管字体未注册');
  assert.deepEqual(home.brokenImages, [], '存在加载失败的图片');
  assert.deepEqual(await headings(), EN_HEADINGS);

  await clickLanguage('中');
  await waitFor(`document.documentElement.lang === 'zh-CN'`, '切换中文');
  assert.match(await evaluate('document.title'), /光伴 RayBend/);
  assert.deepEqual(await headings(), ZH_HEADINGS);
  assert.equal(await evaluate(`localStorage.getItem('raybend.locale')`), 'zh');

  await waitForNavigation(() => send('Page.reload', { ignoreCache: true }), '重新载入');
  await waitFor(`document.documentElement.lang === 'zh-CN' && document.querySelectorAll('h2').length === 6`, '中文偏好持久化');
  assert.deepEqual(await headings(), ZH_HEADINGS);

  await clickLanguage('EN');
  await waitFor(`document.documentElement.lang === 'en'`, '切回英文');
  assert.deepEqual(await headings(), EN_HEADINGS);

  const notFoundUrl = new URL('__smoke_404__', baseUrl);
  await waitForNavigation(() => send('Page.navigate', { url: notFoundUrl.href }), '404 导航');
  await waitFor(`!!document.querySelector('h1')`, '404 路由');
  assert.equal(await evaluate(`document.querySelector('h1').textContent.trim()`), 'Page not found');
  assert.match(await evaluate('document.title'), /Page not found · RayBend/);

  assert.deepEqual(consoleErrors, [], '浏览器 console 有错误');
  assert.deepEqual(exceptions, [], '页面有未捕获异常');
  assert.deepEqual(failedRequests, [], '有资源请求失败');
  console.log(`✓ 官网冒烟通过：${baseUrl.href}（首页、语言切换与持久化、404、资源与运行时错误）`);
} catch (error) {
  console.error('✗ 官网冒烟失败');
  console.error(error);
  if (consoleErrors.length || exceptions.length || failedRequests.length) {
    console.error(JSON.stringify({ consoleErrors, exceptions, failedRequests }, null, 2));
  }
  process.exitCode = 1;
} finally {
  if (ws) ws.close();
  if (chrome?.pid) {
    const exited = new Promise((resolve) => chrome.once('exit', resolve));
    chrome.kill('SIGKILL');
    await exited;
  }
  rmSync(profile, { recursive: true, force: true });
}
