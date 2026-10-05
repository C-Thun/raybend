/** Real Solid components with synthetic display facts; no photo or color E2E. */
import assert from "node:assert/strict";
import { launchChrome, connectCdp, requireServer, sleep } from "./lib/cdp.mjs";

const url = process.argv[2] ?? "http://localhost:1420/";
const port = Number(process.env.CDP_PORT ?? 9531);
await requireServer(url);
const chrome = launchChrome({port});
let cdp;
try {
  cdp = await connectCdp(port);
  const evaluate = async expression => {
    const result = await cdp.send("Runtime.evaluate",{expression,returnByValue:true,awaitPromise:true});
    if(result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  };
  await cdp.send("Page.navigate",{url});
  const deadline=Date.now()+30_000;
  while(!await evaluate('Boolean(document.querySelector("[data-flowbar]"))') && Date.now()<deadline) await sleep(100);
  assert(await evaluate('Boolean(document.querySelector("[data-flowbar]"))'),"App shell must mount");
  const sources=await Promise.all(["/src/shell/store.ts","/src/shell/FlowBar.tsx"].map(path=>fetch(new URL(path,url)).then(r=>r.text())));
  const solid=sources[0].match(/from\s+["']([^"']*\/solid-js\.js[^"']*)["']/)?.[1];
  const web=sources[1].match(/from\s+["']([^"']*\/solid-js_web\.js[^"']*)["']/)?.[1];
  assert(solid&&web,"Use pnpm dev for component smoke");
  const checks=await evaluate(`(async()=>{
    const {createComponent,createSignal}=await import(${JSON.stringify(solid)});
    const {render}=await import(${JSON.stringify(web)});
    const {DisplayColorStatus}=await import('/src/components/ui/DisplayColorStatus.tsx');
    const {t,locale,setLocale}=await import('/src/i18n/index.ts');
    const original=locale();
    const host=document.createElement('div');document.body.append(host);
    const [value,setValue]=createSignal(null);
    const [phase,setPhase]=createSignal('pending');
    const dispose=render(()=>createComponent(DisplayColorStatus,{get status(){return value();},get phase(){return phase();}}),host);
    const results=[];
    const pause=()=>new Promise(resolve=>setTimeout(resolve,0));
    const base={displayId:'屏幕一',profilePath:null,outputSpace:'srgb',sdrWhiteNits:null,reason:null,diagnostic:null,generation:7};
    try {
      for(const language of ['zh-CN','en-US']) {
        setLocale(language);
        for(const [state,key] of [
          [{...base,kind:'icc',profilePath:'C:/'+('摄影棚/'.repeat(120))+'显示.icc'},'color.display.icc'],
          [{...base,kind:'systemManaged',outputSpace:'scRgb',sdrWhiteNits:203},'color.display.scRgb'],
          [{...base,kind:'systemManaged',reason:'limitedOutput'},'color.display.systemSrgb'],
          [{...base,kind:'srgbFallback',reason:'preparationFailed',diagnostic:'CLUT accuracy exceeded\\n配置未生效'},'color.display.fallback'],
          [{...base,kind:'unavailable',reason:'systemUnavailable'},'color.display.unavailable'],
        ]) {
          setValue(state);await pause();
          const detail=host.querySelector('details');
          results.push({name:language+' '+state.kind+' '+state.outputSpace,ok:host.querySelector('p')?.textContent===t(key)&&(!detail||!detail.open)});
          if(detail){detail.open=true;await pause();results.push({name:'diagnostic survives '+language,ok:detail.querySelector('p')?.textContent===(state.diagnostic??state.profilePath)});detail.open=false;}
        }
        setValue(null);
        for(const next of ['pending','inactive','error']){setPhase(next);await pause();results.push({name:language+' '+next,ok:host.querySelector('p')?.textContent===t('color.display.'+next)&&!host.querySelector('details')});}
      }
      setLocale('zh-CN');
      const gear=document.querySelector('[data-flowbar] button[aria-label="设置"]');gear?.click();
      const until=Date.now()+5000;
      while(!document.querySelector('[role=dialog]')?.textContent.includes(t('color.display.inactive'))&&Date.now()<until) await new Promise(r=>setTimeout(r,50));
      const dialog=document.querySelector('[role=dialog]');
      results.push({name:'settings distinguish inactive canvas and OS facts',ok:Boolean(dialog?.textContent.includes(t('color.display.inactive'))&&dialog?.textContent.includes(t('settings.display.detected',{state:t('settings.display.unavailable')}))&&!dialog?.textContent.includes(t('color.display.icc')))});
      dialog?.querySelector('button[aria-label="关闭"]')?.click();await new Promise(r=>setTimeout(r,250));
      results.push({name:'settings close and clear gear state',ok:!document.querySelector('[role=dialog]')&&gear?.getAttribute('aria-pressed')===null});
      return results;
    } finally {dispose();host.remove();setLocale(original);}
  })()`);
  for(const check of checks) assert(check.ok,check.name);
  assert.equal(cdp.exceptions.length,0,JSON.stringify(cdp.exceptions));
  assert.equal(cdp.consoleErrors.length,0,JSON.stringify(cdp.consoleErrors));
  console.log(`✓ Color status component smoke: ${checks.length} checks; both languages, live updates, diagnostics and settings lifecycle`);
} finally {cdp?.close();chrome.kill("SIGKILL");}
