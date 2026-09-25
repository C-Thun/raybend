#!/usr/bin/env node
/** Read-only Windows IPC smoke: query a real asset without driving editor GUI interactions.
 * pnpm exec node scripts/check-lens-ipc-win.mjs --launch <repository-id> <asset-id>
 * Optional assertions: --expect-profile=maker|model, --expect-metadata-warning.
 * Reuses scripts/lib/cdp.mjs. Set LENS_IPC_HOST / LENS_IPC_PORT for an existing CDP endpoint.
 */
import { spawn, execFileSync } from 'node:child_process';
import { openSync, closeSync } from 'node:fs';
import { connectCdp, sleep } from './lib/cdp.mjs';
const options = process.argv.slice(2);
const args = options.filter((arg) => !arg.startsWith('--'));
const expectedProfile = options.find(arg => arg.startsWith('--expect-profile='))?.slice('--expect-profile='.length);
const expectMetadataWarning = options.includes('--expect-metadata-warning');
const [repositoryId, asset] = args;
const assetId = Number(asset);
if (!repositoryId || !Number.isSafeInteger(assetId)) throw new Error('Expected repository-id and integer asset-id');
const port = Number(process.env.LENS_IPC_PORT ?? 9333);
let child;
let cdp;
try {
  if (process.argv.includes('--launch')) {
    const log = openSync('/tmp/raybend-lens-ipc-win.log', 'w');
    child = spawn('/mnt/c/rb-target/raybend/debug/raybend-desktop.exe', [], {
      stdio: ['ignore', log, log],
      env: { ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
        WSLENV: [...(process.env.WSLENV ?? '').split(':'), 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'].filter(Boolean).join(':'),
      },
    });
    closeSync(log);
    await sleep(5000);
  }
  const gateway = execFileSync('ip', ['route', 'show', 'default'], { encoding: 'utf8' }).match(/via\s+(\S+)/)?.[1];
  const routes = process.env.LENS_IPC_HOST ? [[process.env.LENS_IPC_HOST, port]]
    : [[gateway, 9334], [gateway, port], ['127.0.0.1', port]];
  for (const [host, candidatePort] of routes) {
    if (!host) continue;
    try {
      const response = await fetch(`http://${host}:${candidatePort}/json/list`, { signal: AbortSignal.timeout(1500) });
      if (!response.ok) continue;
      cdp = await connectCdp(candidatePort, { host, timeoutMs: 15000, callTimeoutMs: 45000,
        targetFilter: target => { try { return new URL(target.url).pathname === "/"; } catch { return false; } },
      });
      break;
    } catch {}
  }
  if (!cdp) throw new Error('Cannot reach Windows WebView2 CDP; use LENS_IPC_HOST / LENS_IPC_PORT');
  const result = await cdp.send('Runtime.evaluate', {
    expression: `(async () => { const started = performance.now(); try {
      const result = await Promise.race([
        window.__TAURI_INTERNALS__.invoke('lens_match', ${JSON.stringify({repositoryId, assetId})}),
        new Promise((_, reject) => setTimeout(() => reject(new Error('lens_match timed out after 30s')), 30000))
      ]);
      return { ok: true, elapsedMs: Math.round(performance.now()-started), ready: result.ready,
        candidateCount: result.candidates?.length, detected: result.detected, lensName: result.lensName,
        warnings: result.warnings ?? [], hasPanasonic1260: result.candidates?.some(p=>p.maker==='Panasonic' && p.focalMin===12 && p.focalMax===60) };
    } catch(error) { return { ok:false, elapsedMs:Math.round(performance.now()-started), error:String(error) }; } })()`,
    awaitPromise: true, returnByValue: true,
  });
  const value = result.result?.value;
  console.log(JSON.stringify(value ?? result, null, 2));
  if (!value?.ok || !value.ready || value.candidateCount < 1000 || !value.hasPanasonic1260 ||
      (expectedProfile !== undefined && value.detected?.key !== expectedProfile) ||
      (expectMetadataWarning && !(value.warnings?.length > 0))) process.exitCode = 1;
} finally {
  if (child && cdp) {
    try { await cdp.send('Runtime.evaluate', { expression: "window.__TAURI_INTERNALS__.invoke('plugin:window|close', {label:'main'})", returnByValue:true }); } catch {}
  }
  cdp?.close();
  if (child) { child.unref(); }
}
