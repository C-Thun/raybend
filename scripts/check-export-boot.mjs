/** Repeatable DOM/API smoke with populated export fixtures. No visual/color/photo E2E claims. */
import assert from "node:assert/strict";
import { launchChrome, connectCdp, sleep } from "./lib/cdp.mjs";
const url = process.argv[2] ?? "http://localhost:1420/";
const port = Number(process.env.CDP_PORT ?? 9517);
const response = await fetch(url);
assert(response.ok, "Start pnpm dev before running check:export");
function backend() {
  const repo = {id:"repro0000000000",name:"测试库",importTemplate:":FILENAME",createdAt:1700000000000,lastOpenedAt:null,online:true,root:"C:/Photos/demo",displayPath:"C:/Photos/demo",paths:[],photosCount:1,imagesCount:1,triedPaths:0};
  const photo = {id:1,relPath:"photos/2026-08-15/测试.JPG",fileName:"测试.JPG",ext:"jpg",isRaw:false,hasRaw:false,width:4000,height:3000,orientation:1,sizeBytes:1000,missing:false,createdMs:1789000000000,takenAt:1789000000000,takenAtOffsetMin:null,rating:0,colorLabel:null,likeState:null,lockLevel:0,cameraMake:null,cameraModel:null,lens:null,focalMm:null,fNumber:null,exposureMs:null,iso:null,author:null,description:null,gpsLat:null,gpsLon:null,country:null,provinceState:null,city:null,sublocation:null};
  const handlers = new Map(); let next=0;
  let queue={revision:0,generation:0,queues:{},enabled:[]},sequence=0;
  function notify(){for(const args of handlers.values())if(args.event==="export://state")args.handler({payload:structuredClone(queue)});}
  window.__exportQueue={state:()=>queue,skip:()=>{for(const items of Object.values(queue.queues))for(const item of items)if(item.status==="running"){item.status="skipped";item.output="C:/Output/existing.jpg";}queue.revision++;notify();},fail:()=>{for(const items of Object.values(queue.queues))for(const item of items)if(item.status==="running"){item.status="failed";item.error="目录无法写入：C:/输出";}queue.revision++;notify();},finish:()=>{for(const items of Object.values(queue.queues))for(const item of items)if(item.status==="running")item.status="done";queue.revision++;notify();}};
  window.__exportTest = {calls:[],pages:[], edited:true, delayed:false, more:false, count:1,
    change(empty=false) { for (const [id, h] of handlers) if(h.event === "catalog://changed") h.handler({id,event:h.event,payload:{repositoryId:repo.id,root:repo.root,scopePath:"photos/2026-08-15",assetIds:empty?[]:[1],relativePaths:empty?[]:[photo.relPath]}}); }
  };
  localStorage.clear();
  localStorage.setItem("raybend.browse-session.v1", JSON.stringify({repositoryId:repo.id,scopePath:"photos/2026-08-15"}));
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:"main"},currentWebview:{label:"main"}},transformCallback:cb=>cb,unregisterCallback:()=>{},convertFileSrc:p=>p,
    invoke:async(cmd,args={})=>{
      const state=window.__exportTest;state.calls.push(cmd);
      if(cmd==="plugin:event|listen") { const id=++next;handlers.set(id,args);return id; }
      if(cmd==="plugin:event|unlisten") {handlers.delete(args.eventId);return;}
      if(cmd==="repositories_list") {if(state.delayed)await new Promise(r=>setTimeout(r,100)); return [structuredClone(repo)];}
      if(cmd==="setting_get" && args.key==="export.presets.v1" && !location.search.includes("export-empty"))return JSON.stringify({version:1,presets:[{id:"test-preset",name:"测试输出",format:"jpeg",quality:90,maxEdge:0,directory:"C:/Output",template:":FILENAME"},{id:"second-preset",name:"第二输出",format:"png",quality:90,maxEdge:0,directory:"C:/Second",template:":FILENAME"}]});
      if(cmd==="external_applications")return [];
      if(cmd==="external_task")return {id:0,revision:0,status:"idle",output:null,error:null,missingApplication:null};
      if(cmd==="export_queue"){
        const {action,items,presetId,ids=[]}=args;
        if(action==="enqueue"){for(const item of items){const q=queue.queues[item.preset.id]??=[];if(q.some(i=>i.snapshot.profileHash===item.snapshot.profileHash))continue;q.unshift({...item,id:`${queue.generation}:${++sequence}`,sequence,status:"pending"});}}
        if(action==="enable"){if(!queue.enabled.includes(presetId))queue.enabled.push(presetId);const item=queue.queues[presetId]?.findLast(i=>i.status==="pending");if(item)item.status="running";}
        if(action==="disable")queue.enabled=queue.enabled.filter(id=>id!==presetId);
        if(action==="stop")queue.enabled=[];
        if(action==="remove")for(const [id,items]of Object.entries(queue.queues))queue.queues[id]=items.filter(i=>!ids.includes(i.id)||["running","done","skipped"].includes(i.status));
        if(action==="retry")for(const items of Object.values(queue.queues))for(const item of items)if(ids.includes(item.id)&&item.status==="failed"){item.status="running";item.error=null;}
        if(action==="reset")queue={revision:queue.revision,generation:queue.generation+1,queues:{},enabled:[]};
        if(action!=="status"){queue.revision++;notify();}return structuredClone(queue);
      }
      if(cmd==="export_snapshots")return args.references.map(reference=>({reference,name:reference.variant.startsWith("issue:")?"中文定稿":reference.variant,relPath:photo.relPath,profileHash:reference.variant,sourceSignature:"source",stack:{values:{}}}));
      if(cmd==="browse_page") { const offset=args.offset??0,limit=args.limit??256;state.pages.push(offset);return {total:state.count,offset,items:Array.from({length:Math.max(0,Math.min(limit,state.count-offset))},(_,i)=>({...photo,id:offset+i+1}))}; }
      if(cmd==="browse_timeline") return {total:state.count,entries:Array.from({length:state.count},(_,i)=>({id:i+1,relPath:photo.relPath,takenAt:photo.takenAt}))};
      if(cmd==="browse_facets")return {ratings:[],colors:[],likes:[],locks:[]};
      if(cmd==="browse_flags_get")return {total:0,picks:[],rejects:[]};
      if(["volumes_list","recent_dirs_list","browse_markings","tag_list"].includes(cmd))return [];
      if(cmd==="dir_list")return String(args.path).replaceAll("\\","/").endsWith("/photos")?[{name:"2026-08-15",path:repo.root+"/photos/2026-08-15",hasChildren:false}]:[];
      if(cmd==="dir_meta_ensure")return (args.files??[]).map(f=>({relative:f.relative,width:4000,height:3000,orientation:1}));
      if(cmd==="file_exif")return {...photo,orientation:1};
      if(cmd==="export_preset_validate")return {errors:{},warnings:[]};
      if(cmd==="repository_sync_dir"){state.change(true);return [[1,1],[1,1]];}
      if(cmd==="export_variants" && state.delayed) await new Promise(r=>setTimeout(r,120));
      if(cmd==="export_variants")return args.assetIds.map(assetId=>({assetId,variants:(state.edited?["sooc","latest",...(state.more?Array.from({length:8},(_,i)=>"issue:"+(i+7)):["issue:7"])]:["sooc"]).map(variant=>({reference:{assetId,variant},relPath:photo.relPath,name:variant.startsWith("issue:")?"中文定稿 "+variant.split(":")[1]:variant,sourceBase:"sooc",profileHash:variant,main:variant==="latest" || (!state.edited && variant==="sooc"),edited:state.edited,createdAt:1}))}));
      if(["thumb_get","view_image","export_variant_image"].includes(cmd))return Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="),c=>c.charCodeAt(0)).buffer;
      return null;
    }};
}
const chrome=launchChrome({port});let cdp;
try {
  cdp=await connectCdp(port);
  await cdp.send("Runtime.enable");await cdp.send("Page.enable");
  await cdp.send("Page.addScriptToEvaluateOnNewDocument",{source:`(${backend.toString()})();`});
  await cdp.send("Page.navigate",{url});
  const evaluate=async expression=>{
    const answer=await cdp.send("Runtime.evaluate",{expression,awaitPromise:true,returnByValue:true});
    if(answer.exceptionDetails)throw new Error(answer.exceptionDetails.exception?.description??answer.exceptionDetails.text);
    return answer.result?.value;
  };
  const until=async(expression,label)=>{
    const end=Date.now()+30000;
    while(Date.now()<end){if(await evaluate(expression))return;await sleep(100);}
    throw new Error(label+": "+await evaluate("document.body.innerText.slice(0,1200)"));
  };
  await until("[...document.querySelectorAll('[data-part=item]')].some(b=>b.textContent.trim()==='导出')","flowbar startup");
  await until("[...document.querySelectorAll('button')].some(b=>b.textContent.includes('2026-08-15'))","startup repository restored before flow switch");
  const point=await evaluate("(()=>{const el=[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导出');const r=el.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()");
  await cdp.send("Input.dispatchMouseEvent",{type:"mousePressed",button:"left",buttons:1,clickCount:1,...point});
  await cdp.send("Input.dispatchMouseEvent",{type:"mouseReleased",button:"left",buttons:0,clickCount:1,...point});
  await until("document.querySelector('[data-export-issue=\"issue:7\"]')","populated grid requests variants");
  assert(await evaluate("document.querySelectorAll('[data-export-issue]').length===2"),"one photo exposes SOOC, latest and named issue");
  await until("[...document.querySelectorAll('[data-export-left] button')].some(b=>b.textContent.includes('2026-08-15'))","directory tree");
  await until("document.querySelector('[data-export-issue=\"issue:7\"] img')", "issue thumbnail ready");
  const unchangedRefresh = await evaluate(`(async()=>{
    const tile=document.querySelector('[data-export-issue="issue:7"]');
    const img=tile.querySelector('img'),src=img.src;
    window.__exportTest.change(true);
    await new Promise(r=>setTimeout(r,300));
    return {tile:tile===document.querySelector('[data-export-issue="issue:7"]'),image:img===tile.querySelector('img'),src:src===tile.querySelector('img')?.src};
  })()`);
  assert.deepEqual(unchangedRefresh,{tile:true,image:true,src:true},"unchanged catalog refresh preserves issue tile/image identity and URL");
  const focusRefresh = await evaluate(`(async()=>{
    const tile=document.querySelector('[data-export-issue="issue:7"]');
    const img=tile.querySelector('img'),src=img.src;
    const reads=window.__exportTest.calls.filter(c=>c==='export_variants').length;
    for(let i=0;i<3;i++){window.dispatchEvent(new Event('focus'));await new Promise(r=>setTimeout(r,250));}
    return {tile:tile===document.querySelector('[data-export-issue="issue:7"]'),image:img===tile.querySelector('img'),src:src===tile.querySelector('img')?.src,reads:window.__exportTest.calls.filter(c=>c==='export_variants').length-reads};
  })()`);
  assert.deepEqual(focusRefresh,{tile:true,image:true,src:true,reads:0},"window refocus rereads disk without rebuilding or refetching unchanged issues");
  const changedRefresh = await evaluate(`(async()=>{
    const tile=document.querySelector('[data-export-issue="issue:7"]'),img=tile.querySelector('img');
    const height=tile.closest('[data-virtual-scroller]').scrollHeight;
    window.__exportTest.delayed=true;
    window.__exportTest.change();
    await new Promise(r=>setTimeout(r,40));
    const during=tile===document.querySelector('[data-export-issue="issue:7"]') && img===tile.querySelector('img');
    await new Promise(r=>setTimeout(r,250));
    window.__exportTest.delayed=false;
    return {during,after:tile===document.querySelector('[data-export-issue="issue:7"]'),image:img===tile.querySelector('img'),height:height===tile.closest('[data-virtual-scroller]')?.scrollHeight};
  })()`);
  assert.deepEqual(changedRefresh,{during:true,after:true,image:true,height:true},"slow metadata/image refresh keeps complete tiles and geometry");

  const mainSelector="[data-export-area=gallery] [role=option]";
  assert.equal(await evaluate(`document.querySelector('${mainSelector}').getBoundingClientRect().width`),240,"upper initial minimum");
  assert.equal(await evaluate("document.querySelector('[data-export-area=gallery] [role=slider]').getAttribute('aria-valuemax')"),"5");
  assert.equal(await evaluate("document.querySelector('[data-export-area=queue] [role=slider]').getAttribute('aria-valuemax')"),"16");
  await evaluate("window.__exportNodes={bar:document.querySelector('[data-export-area=gallery] [data-tiles-control-bar]'),shell:document.querySelector('[data-export-area=gallery] [data-tiles-shell]'),slider:document.querySelector('[data-export-area=gallery] [role=slider]')};window.__exportNodes.slider.focus()");
  for(const [key,size] of [['End',320],['Home',240]]){
    await cdp.send('Input.dispatchKeyEvent',{type:'keyDown',key,code:key});
    await cdp.send('Input.dispatchKeyEvent',{type:'keyUp',key,code:key});
    await until(`document.querySelector('${mainSelector}').getBoundingClientRect().width===${size}`,"zoom boundary "+size);
    assert(await evaluate("window.__exportNodes.bar===document.querySelector('[data-export-area=gallery] [data-tiles-control-bar]') && window.__exportNodes.shell===document.querySelector('[data-export-area=gallery] [data-tiles-shell]') && window.__exportNodes.slider===document.querySelector('[data-export-area=gallery] [role=slider]')"),"shell, bar and slider node identity survives zoom");
  }
  assert(await evaluate("!document.querySelector('[data-export-area=gallery] [data-tiles-fit-row]').disabled"),"fit allowed for this measured viewport");
  await evaluate("document.querySelector('[data-export-area=gallery] [data-tiles-fit-row]').click()");
  await until(`document.querySelector('${mainSelector}').getBoundingClientRect().width>240`,"fit provider reaches grid");
  assert(await evaluate(`document.querySelector('${mainSelector}').getBoundingClientRect().width<=320`));
  await evaluate("window.__exportNodes.slider.focus()");
  await cdp.send('Input.dispatchKeyEvent',{type:'keyDown',key:'Home',code:'Home'});
  await cdp.send('Input.dispatchKeyEvent',{type:'keyUp',key:'Home',code:'Home'});
  await until(`document.querySelector('${mainSelector}').getBoundingClientRect().width===240`,"minimum after fit");
  await evaluate("document.querySelector('[data-export-area=gallery] [role=option]').click()");
  assert(await evaluate("(()=>{const tile=document.querySelector('[data-export-area=gallery] [role=option]');const frame=tile.querySelector('[data-tile-selection-frame]');return frame && Number(getComputedStyle(frame).zIndex)>Math.max(...[...tile.querySelectorAll('[data-tile-bar]')].map(el=>Number(getComputedStyle(el).zIndex)||0));})()"),"selection frame paints above both information strips");
  await evaluate("document.querySelector('[data-export-area=gallery] [role=option]').click()");
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.includes('测试输出')).click()");
  await evaluate("document.querySelector('[data-export-issue=\"issue:7\"] [role=option]').click()");
  await until("[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='送入队列'&&!b.disabled)","selected issue can enqueue");
  await evaluate("document.querySelector('[data-export-issue=\"issue:7\"] [role=option]').focus()");
  await cdp.send("Input.dispatchKeyEvent",{type:"keyDown",key:"Enter",code:"Enter"});
  await cdp.send("Input.dispatchKeyEvent",{type:"keyUp",key:"Enter",code:"Enter"});
  await until("document.querySelector('[data-export-area=queue] [role=option]')","one issue is queued");
  await evaluate("document.querySelector('[data-export-area=queue] [role=option]').click()");
  assert.equal(await evaluate("document.querySelectorAll('[data-export-area=queue] [role=option]').length"),1,"pending click selects without removal");
  await evaluate("document.querySelector('[data-export-area=queue] [role=option]').focus()");
  await cdp.send("Input.dispatchKeyEvent",{type:"keyDown",key:"Delete",code:"Delete"});
  await cdp.send("Input.dispatchKeyEvent",{type:"keyUp",key:"Delete",code:"Delete"});
  await until("!document.querySelector('[data-export-area=queue] [role=option]')","explicit remove");
  // Session switch survives workspace unmount; activity only follows running state.
  await evaluate("document.querySelector('[data-export-run]').click()");
  await until("document.querySelector('[data-export-run]').textContent==='停止当前预设'","empty enabled preset");
  assert.equal(await evaluate("document.querySelectorAll('[data-export-status=running]').length"),0);
  assert.equal(await evaluate("!!document.querySelector('[data-flow-processing]')"),false,"empty enabled has no shimmer");
  await evaluate("document.querySelector('[data-export-run]').click()");
  await until("document.querySelector('[data-export-run]').textContent==='开始导出'","disabled preset");
  await evaluate("document.querySelector('[data-export-issue=\"issue:7\"] [role=option]').dispatchEvent(new MouseEvent('click',{bubbles:true,ctrlKey:true}))");
  await until("[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='送入队列'&&!b.disabled)","refill selection ready");
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='送入队列').click()");
  await until("document.querySelector('[data-export-area=queue] [role=option]')","runtime queue refill");
  await evaluate("document.querySelector('[data-export-run]').click()");
  await until("document.querySelector('[data-export-status=running]')","runtime notification reflected");
  assert(await evaluate("!!document.querySelector('[data-flow-processing]')"),"running shimmer");
  await evaluate("window.__exportQueue.fail()");
  await until("document.querySelector('[data-export-queue-error]')?.textContent.includes('目录无法写入')", "failure reason shown in queue tile");
  assert(await evaluate("document.querySelector('[data-export-area=queue] [role=option]').getAttribute('aria-disabled')!=='true'"),"failed item stays selectable");
  await evaluate("document.querySelector('[data-export-retry]').click()");
  await until("!document.querySelector('[data-export-queue-error]') && document.querySelector('[data-export-status=running]')", "queue retry uses same item");
  await evaluate("[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='浏览').click()");
  await sleep(150);await evaluate("window.__exportQueue.finish()");
  await evaluate("[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导出').click()");
  await until("document.querySelector('[data-export-status=done]')","completion retained across workspaces");
  await until("document.querySelector('[data-export-issue=\"issue:7\"] img')", "all issues return after workspace remount");
  assert.equal(await evaluate("!!document.querySelector('[data-flow-processing]')"),false,"done removes shimmer");
  assert.equal(await evaluate("document.querySelector('[data-export-area=queue] [role=option]').getAttribute('aria-disabled')"),'true');
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='重置所有队列').click()");
  await until("document.querySelector('[role=dialog]')","reset dialog");
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='确定').click()");
  await until("!document.querySelector('[data-export-area=queue] [role=option]')","reset clears runtime queue");
  assert.deepEqual(await evaluate("[...document.querySelectorAll('[data-export-form] [data-scope=segment-group][data-part=item]')].map(el=>el.textContent.trim()).filter(text=>['WEBP','AVIF','JPG','PNG'].includes(text))"),['WEBP','AVIF','JPG','PNG']);
  assert(await evaluate("![...document.querySelectorAll('[data-export-right] button')].some(el=>el.textContent.trim()==='新建')"));
  assert.equal(await evaluate("document.querySelector('[data-export-form-title]').textContent.trim()"),'修改预设');
  assert.equal(await evaluate("document.querySelector('[data-export-save]').textContent.trim()"),'变更');
  assert(await evaluate("!['导入预设','导出预设','导出预览','失败与失效清单'].some(text=>[...document.querySelectorAll('[data-export-right] button')].some(el=>el.textContent.trim()===text))"),'deferred and removed actions are absent');
  assert(await evaluate("document.querySelector('[data-export-save-hint]').textContent.includes('先保存')"),'footer save hint remains below actions');
  const layout=await evaluate(`(()=>{const top=document.querySelector('[data-export-presets]'),list=document.querySelector('[data-export-preset-list]'),settings=document.querySelector('[data-export-settings]'),right=document.querySelector('[data-export-right]');return {topGrow:getComputedStyle(top).flexGrow,listGrow:getComputedStyle(list).flexGrow,settingsGrow:getComputedStyle(settings).flexGrow,settingsShrink:getComputedStyle(settings).flexShrink,settingsBelow:settings.getBoundingClientRect().top>=top.getBoundingClientRect().bottom,footer:document.querySelector('[data-export-save-hint]').getBoundingClientRect().bottom<=right.getBoundingClientRect().bottom};})()`);
  assert.deepEqual(layout,{topGrow:'1',listGrow:'1',settingsGrow:'0',settingsShrink:'0',settingsBelow:true,footer:true},'preset list flexes; compact settings stay at bottom');
  const setName=async value=>evaluate(`(()=>{const input=document.querySelector('[data-export-form] input');input.value=${JSON.stringify(value)};input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await setName('另一个名称');
  assert(await evaluate("document.querySelector('[data-export-run]').disabled"),'unsaved form cannot start queue');
  assert(await evaluate("document.querySelector('[data-export-save]').disabled"),'save waits for name detection');
  await sleep(1300);
  assert.equal(await evaluate("document.querySelector('[data-export-form-title]').textContent.trim()"),'修改预设','name mode does not switch before debounce');
  await until("document.querySelector('[data-export-form-title]').textContent.trim()==='新建预设' && document.querySelector('[data-export-save]').textContent.trim()==='新增'",'new name changes both title and save action');
  await setName('  测试输出  ');
  await until("document.querySelector('[data-export-form-title]').textContent.trim()==='修改预设' && document.querySelector('[data-export-save]').textContent.trim()==='变更'",'normalized existing name matches');
  await setName('测试输出');
  await until("!document.querySelector('[data-export-save]').disabled",'name detection completed before continuing');
  assert(await evaluate("(()=>{const save=document.querySelector('[data-export-save]').getBoundingClientRect(),run=document.querySelector('[data-export-run]').getBoundingClientRect();return save.y===run.y && run.width>save.width*2 && run.x-save.right>=8;})()"),'compact save and expanding run share a row with padding');
  for (const [mode,label,value] of [['percent','百分比缩小',50],['maxEdge','最大边长',2048],['original','保持不变',0]]) {
    await evaluate(`document.querySelector('[data-export-size] [data-part=item][data-choice-value="${mode}"]').click()`);
    await until(`document.querySelector('[data-export-size] [data-part=item][data-choice-value="${mode}"]')?.getAttribute('data-state')==='checked'`, 'size mode '+mode);
    assert.equal(await evaluate("document.querySelectorAll('[data-export-size] input[type=number]:enabled').length"),mode==='original'?0:1);
    assert(await evaluate("[...document.querySelectorAll('[data-export-size] input[type=number]')].every(el=>el.getBoundingClientRect().width===96)"));
  }
  for(const format of ['WEBP','AVIF','JPG','PNG']){
    await evaluate(`[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='${format}').click()`);
    await until(`[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='${format}')?.getAttribute('data-state')==='checked'`,"format control ready");
    await sleep(50);
    assert.equal(await evaluate("!!document.querySelector('[data-export-quality]')"),format!=='PNG',format+" quality visibility");
  }
  const stability=await evaluate(`(async()=>{
    const left=document.querySelector('[data-export-left]');
    const row=[...left.querySelectorAll('button')].find(b=>b.textContent.includes('2026-08-15'));
    const card=[...left.querySelectorAll('[role=option]')].find(b=>b.textContent.includes('测试库'));
    const top=row.getBoundingClientRect().top;
    let shifts=0; const timer=setInterval(()=>{if(row.getBoundingClientRect().top!==top)shifts++;},5);
    window.__exportTest.delayed=true;
    for(let i=0;i<3;i++){window.__exportTest.change();await new Promise(r=>setTimeout(r,30));}
    await new Promise(r=>setTimeout(r,300));clearInterval(timer);
    return {shifts,row:row.isConnected,card:card.isConnected};
  })()`);
  assert.deepEqual(stability,{shifts:0,row:true,card:true},"background catalog notices preserve directory position and node identity");
  const scopeButton="[...document.querySelectorAll('button')].find(b=>['全部','仅定稿','编辑过'].includes(b.textContent.trim()))";
  await evaluate(scopeButton+".click()");
  await until("document.querySelectorAll('[data-export-issue]').length===2","manual issues scope");
  await evaluate(scopeButton+".click()");
  await until("document.querySelectorAll('[data-export-issue]').length===2","edited scope");
  await evaluate(scopeButton+".click()");
  await until("document.querySelectorAll('[data-export-issue]').length===2","all scope recovery");
  await evaluate("window.__exportTest.more=true;window.__exportTest.change()");
  await until("document.querySelectorAll('[data-export-area=gallery] [data-export-issue]').length===6","six visible small issues");
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='全部定稿').click()");
  await until("document.querySelectorAll('[data-export-all-issues] [data-export-issue]').length===10","modal includes all issues and main");
  const hidden=await evaluate("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]')!==null");assert(hidden);
  await evaluate("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]').click()");
  await until("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]')?.getAttribute('aria-selected')==='true'","modal selection survives update");
  await evaluate("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]').focus()");
  await cdp.send("Input.dispatchKeyEvent",{type:"keyDown",key:"a",code:"KeyA",modifiers:2});
  await cdp.send("Input.dispatchKeyEvent",{type:"keyUp",key:"a",code:"KeyA",modifiers:2});
  await until("document.querySelectorAll('[data-export-all-issues] [role=option][aria-selected=true]').length===10","modal Ctrl+A selects this photo's full issue list");
  await cdp.send("Input.dispatchKeyEvent",{type:"keyDown",key:"Enter",code:"Enter"});
  await cdp.send("Input.dispatchKeyEvent",{type:"keyUp",key:"Enter",code:"Enter"});
  await until("document.querySelectorAll('[data-export-area=queue] [role=option]').length>0","modal Enter enqueue via shared command");
  await cdp.send("Input.dispatchKeyEvent",{type:"keyDown",key:"Delete",code:"Delete"});
  await cdp.send("Input.dispatchKeyEvent",{type:"keyUp",key:"Delete",code:"Delete"});
  await until("!document.querySelector('[data-export-area=queue] [role=option]')","modal Delete removes eligible pending items");
  await evaluate("document.querySelector('[data-scope=dialog][data-part=close-trigger]').click()");
  // Real pointer events exercise drag threshold, destination and post-drop click suppression.
  await evaluate("document.querySelector('[data-export-preset=\"test-preset\"]').click();document.querySelector('[data-export-area=gallery] [role=option]').focus()");
  const key=async(key,code,modifiers=0)=>{
    await cdp.send('Input.dispatchKeyEvent',{type:'keyDown',key,code,modifiers});
    await cdp.send('Input.dispatchKeyEvent',{type:'keyUp',key,code,modifiers});
  };
  await key('a','KeyA',2);
  await until("document.querySelectorAll('[data-export-area=gallery] [role=option][aria-selected=true]').length===7",'top Ctrl+A includes complete hidden issue selection');
  const drag=async(id,source='[data-export-area=gallery] [role=option]',release=true)=>{
    const a=await evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(source)}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
    const b=await evaluate(`(()=>{const el=document.querySelector('[data-export-preset="${id}"]');el.scrollIntoView();const r=el.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',...a});
    await cdp.send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...a});
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:a.x+3,y:a.y});
    assert(!await evaluate("!!document.querySelector('[data-export-drag-preview]')"),'normal click movement does not drag');
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:a.x+12,y:a.y});
    await until("document.querySelector('[data-export-drag-preview]')",'drag starts');
    const node=await evaluate("(()=>{const el=document.querySelector('[data-export-drag-preview]'),r=el.getBoundingClientRect();window.__dragPreview=el;return {width:r.width,height:r.height,cards:el.querySelectorAll('.rb-export-drag-card').length};})()");
    assert.deepEqual(node,{width:200,height:200,cards:4});
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,...b});
    await sleep(100);
    const hover=await evaluate(`(()=>{const el=document.elementFromPoint(${b.x},${b.y});return {hit:el?.outerHTML.slice(0,200),card:el?.closest('[data-export-preset]')?.dataset.exportPreset,classes:document.querySelector('[data-export-preset="${id}"]')?.className,point:{x:${b.x},y:${b.y}},rect:document.querySelector('[data-export-preset="${id}"]')?.getBoundingClientRect().toJSON(),list:document.querySelector('[data-export-preset-list]')?.getBoundingClientRect().toJSON(),pointer:getComputedStyle(document.querySelector('[data-export-preset="${id}"]')).pointerEvents};})()`);
    assert.equal(hover.card,id,'pointer hits preset: '+JSON.stringify(hover));
    await until(`document.querySelector('[data-export-preset="${id}"]')?.classList.contains('border-brand-2')`,'drop target highlighted');
    assert(await evaluate("window.__dragPreview===document.querySelector('[data-export-drag-preview]')"),'preview identity survives pointer move');
    if(release)await cdp.send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...b});
    else {await evaluate("window.dispatchEvent(new Event('blur'))");await cdp.send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...b});}
    await until("!document.querySelector('[data-export-drag-preview]')",'preview cleared');
  };
  await drag('second-preset');
  await until("window.__exportQueue.state().queues['second-preset']?.length===10",'drag includes all ten selected issues');
  await until("document.querySelector('[data-export-preset-count=\"second-preset\"]').textContent==='10/10'",'animated counter reaches target');
  assert(await evaluate("document.querySelector('[data-export-preset=\"test-preset\"]').getAttribute('aria-pressed')==='true'"),'drag does not switch selected preset');
  assert.equal(await evaluate("document.querySelectorAll('[data-export-area=gallery] [role=option][aria-selected=true]').length"),7,'source selection survives drop');
  await drag('second-preset','[data-export-area=gallery] [data-export-issue] [role=option]');
  assert.equal(await evaluate("window.__exportQueue.state().queues['second-preset'].length"),10,'repeat drop deduplicates');
  assert(!await evaluate("document.querySelector('[data-export-preset-count=\"second-preset\"]').classList.contains('rb-export-count-bump')"),'duplicate drop does not animate');
  await drag('test-preset','[data-export-area=gallery] [data-export-issue] [role=option]',false);
  assert.equal(await evaluate("window.__exportQueue.state().queues['test-preset']?.length??0"),0,'blur cancels drop');
  await drag('test-preset');
  await until("window.__exportQueue.state().queues['test-preset']?.length===10",'same selection drops into another preset');
  await evaluate("document.querySelector('[data-export-run]').click()");
  await until("document.querySelector('[data-export-status=running]')",'dropped issues start normal worker');
  assert.equal(await evaluate("document.querySelectorAll('[data-export-area=gallery] [role=option][aria-selected=true]').length"),7,'selection retained when dropped queue begins processing');
  assert(await evaluate("document.querySelector('[data-export-run] .rb-shimmer-mask svg')!==null"),'button glyph and text share running mask');
  await evaluate("window.__exportQueue.skip()");
  await until("document.querySelector('[data-export-status=skipped]')",'existing file skip status retained');
  assert(!await evaluate("!!document.querySelector('[data-flow-processing]')"),'skipped file is terminal');
  assert(await evaluate("document.querySelector('[data-export-area=queue] [role=option][aria-disabled=true]')"),'skipped item cannot be removed');
  await evaluate("document.querySelector('[data-export-area=queue] [role=option]:not([aria-disabled=true])').focus()");
  await key('a','KeyA',2);
  await until("document.querySelectorAll('[data-export-area=queue] [role=option][aria-selected=true]').length===9",'bottom Ctrl+A selects only unfinished');
  await key('Delete','Delete');
  await until("document.querySelectorAll('[data-export-area=queue] [role=option]').length===1",'bottom Delete preserves skipped issue');
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='重置所有队列').click()");
  await until("document.querySelector('[role=dialog]')",'reset drag queues');
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='确定').click()");
  await until("Object.keys(window.__exportQueue.state().queues).length===0",'reset all destination queues');
  await evaluate("window.__exportTest.edited=false;window.__exportTest.change()");
  await until("document.querySelectorAll('[data-export-area=gallery] [data-export-issue]').length===0","SOOC main has no duplicate child");
  await evaluate(scopeButton+".click()");
  await until("!document.querySelector('[data-export-area=gallery] [role=option]')","manual filter excludes unedited SOOC");
  await evaluate(scopeButton+".click()");
  await until("!document.querySelector('[data-export-area=gallery] [role=option]')","edited filter excludes baseline");
  await evaluate(scopeButton+".click()");
  await until("document.querySelector('[data-export-area=gallery] [role=option]')","all filter restores baseline main");
  const before=await evaluate("window.__exportTest.calls.filter(c=>c==='export_variant_image').length");await sleep(100);
  assert.equal(await evaluate("window.__exportTest.calls.filter(c=>c==='export_variant_image').length"),before,"failed thumbnails never self-retry");
  assert(await evaluate("window.__exportTest.calls.includes('export_variants')"));
  assert(!await evaluate("document.querySelector('[data-export-workspace]').innerText.includes('issue')"),"Chinese UI terminology");
  assert.deepEqual(cdp.exceptions,[]);
  assert.deepEqual(cdp.consoleErrors,[]);
  await evaluate("window.__exportTest.edited=true;window.__exportTest.more=false;window.__exportTest.count=300;window.__exportTest.change();window.dispatchEvent(new Event('focus'))");
  await sleep(500);
  for(let i=0;i<3;i++) {
    await evaluate("(()=>{const scroller=document.querySelector('[data-export-area=gallery] [data-virtual-scroller]');scroller.scrollTop=scroller.scrollHeight;scroller.dispatchEvent(new Event('scroll'));})()");
    await sleep(350);
  }
  assert(await evaluate("window.__exportTest.pages.some(offset=>offset>=256)"),"fixture reaches the second 256-photo page");
  await until("[...document.querySelectorAll('[data-export-area=gallery] [data-export-issue=\"issue:7\"] img')].length>0", "later page renders issues");
  await sleep(500);
  const scrolledRefresh=await evaluate(`(async()=>{
    const tile=document.querySelector('[data-export-area=gallery] [data-export-issue="issue:7"]');
    const img=tile?.querySelector('img');
    let lost=false;const timer=setInterval(()=>{if(!tile?.isConnected || !img?.isConnected)lost=true;},5);
    window.dispatchEvent(new Event('focus'));await new Promise(r=>setTimeout(r,600));clearInterval(timer);
    return {lost,connected:tile?.isConnected,image:img?.isConnected};
  })()`);
  assert.deepEqual(scrolledRefresh,{lost:false,connected:true,image:true},"focus refresh on later pages never drops visible issue cells");
  await cdp.send("Page.navigate",{url:url+(url.includes('?')?'&':'?')+'export-empty=1'});
  await until("[...document.querySelectorAll('[data-part=item]')].some(b=>b.textContent.trim()==='导出')",'empty fixture startup');
  await evaluate("[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导出').click()");
  await until("document.querySelector('[data-export-preset-list]')?.textContent==='当前无预设，请先新建'",'empty preset guidance');
  assert.equal(await evaluate("document.querySelector('[data-export-form-title]').textContent.trim()"),'新建预设');
  assert.equal(await evaluate("document.querySelector('[data-export-save]').textContent.trim()"),'新增');
  assert(await evaluate("document.querySelector('[data-export-save]').classList.contains('bg-brand-2')"),'new preset uses accent');
  assert(await evaluate("document.querySelector('[data-export-run] svg')!==null"),'export icon precedes text');
  assert.equal(await evaluate("document.querySelector('[data-export-existing-file] input:checked').value"),'append');
  assert(await evaluate("document.querySelector('[data-export-run]').disabled"));
  assert.deepEqual(cdp.exceptions,[]);assert.deepEqual(cdp.consoleErrors,[]);
  console.log("✓ populated export: bounds, quality visibility, issue selection/queue removal, modal, photo filters, stable library, failure retries");
} finally { cdp?.close();chrome.kill("SIGKILL"); }
