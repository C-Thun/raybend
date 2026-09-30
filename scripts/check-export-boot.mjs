/** Repeatable DOM/API smoke with populated export fixtures. No visual/color/photo E2E claims. */
import assert from "node:assert/strict";
import { launchChrome, connectCdp, sleep } from "./lib/cdp.mjs";
const url = process.argv[2] ?? "http://localhost:1420/";
const port = Number(process.env.CDP_PORT ?? 9517);
const response = await fetch(url);
assert(response.ok, "Start pnpm dev before running check:export");
function backend() {
  const repo = {id:"repro0000000000",name:"测试库",importTemplate:":FILENAME",createdAt:1700000000000,lastOpenedAt:null,online:true,root:"C:/Photos/demo",displayPath:"C:/Photos/demo",paths:[],photosCount:1,imagesCount:1,triedPaths:0};
  repo.connection = {repositoryId:repo.id,state:"online",reason:null,root:repo.root,generation:"1",revision:"1",observedAt:1700000000000};
  repo.paths = [{path:repo.root,status:"online",lastSeenAt:1700000000000}];
  const photo = {id:1,relPath:"photos/2026-08-15/测试.JPG",fileName:"测试.JPG",ext:"jpg",isRaw:false,hasRaw:false,width:4000,height:3000,orientation:1,sizeBytes:1000,missing:false,createdMs:1789000000000,takenAt:1789000000000,takenAtOffsetMin:null,rating:0,colorLabel:null,likeState:null,lockLevel:0,cameraMake:null,cameraModel:null,lens:null,focalMm:null,fNumber:null,exposureMs:null,iso:null,author:null,description:null,gpsLat:null,gpsLon:null,country:null,provinceState:null,city:null,sublocation:null};
  const handlers = new Map(); let next=0;
  let queue={revision:0,generation:0,queues:{},enabled:[]},sequence=0;
  function notify(){for(const args of handlers.values())if(args.event==="export://state")args.handler({payload:structuredClone(queue)});}
  window.__exportQueue={state:()=>queue,skip:()=>{for(const items of Object.values(queue.queues))for(const item of items)if(item.status==="running"){item.status="skipped";item.output="C:/Output/existing.jpg";}queue.revision++;notify();},fail:()=>{for(const items of Object.values(queue.queues))for(const item of items)if(item.status==="running"){item.status="failed";item.error="目录无法写入：C:/输出";}queue.revision++;notify();},finish:()=>{for(const items of Object.values(queue.queues))for(const item of items)if(item.status==="running")item.status="done";queue.revision++;notify();}};
  window.__exportTest = {calls:[],pages:[], edited:true, delayed:false, more:false, count:1,
    pickedPath:null, pickerDelay:false, pickerResolve:null, view:()=>structuredClone(repo),
    connection(online) {
      repo.online=online;repo.root=online?repo.displayPath:null;
      repo.connection={...repo.connection,state:online?"online":"offline",root:repo.root,
        revision:String(BigInt(repo.connection.revision)+1n),generation:online?String(BigInt(repo.connection.generation)+1n):repo.connection.generation};
      for(const args of handlers.values())if(args.event==="repository://connection")args.handler({payload:structuredClone(repo.connection)});
    },
    change(empty=false) { for (const [id, h] of handlers) if(h.event === "catalog://changed") h.handler({id,event:h.event,payload:{repositoryId:repo.id,root:repo.root,scopePath:"photos/2026-08-15",assetIds:empty?[]:[1],relativePaths:empty?[]:[photo.relPath]}}); }
  };
  localStorage.clear();
  localStorage.setItem("raybend.locale", "zh-CN");
  localStorage.setItem("raybend.browse-session.v1", JSON.stringify({repositoryId:repo.id,scopePath:"photos/2026-08-15"}));
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:"main"},currentWebview:{label:"main"}},transformCallback:cb=>cb,unregisterCallback:()=>{},convertFileSrc:p=>p,
    invoke:async(cmd,args={})=>{
      const state=window.__exportTest;state.calls.push(cmd);
      if(cmd==="plugin:event|listen") { const id=++next;handlers.set(id,args);return id; }
      if(cmd==="plugin:event|unlisten") {handlers.delete(args.eventId);return;}
      if(cmd==="migration_snapshot")return {revision:"1",active:[]};
      if(cmd==="organization_reconcile")return {processedBatches:0,failedRepositories:[]};
      if(cmd==="organization_buckets")return [];
      if(cmd==="repository_remount") return structuredClone(repo);
      if(cmd==="plugin:dialog|open") {
        if(state.pickerDelay)return new Promise(resolve=>{state.pickerResolve=resolve;});
        return state.pickedPath;
      }
      if(cmd==="repository_counts")return [1,1];
      if(cmd==="repository_settings") {
        assertOnline();return {importTemplate:repo.importTemplate};
      }
      if(cmd==="repository_template_preview")return {ok:true,paths:["photos/示例.JPG"],warnings:[],error:null};
      if(cmd==="repository_add_location") {
        if(args.repositoryId!==repo.id || args.path!=="G:/中文照片库")throw {code:"identity_mismatch"};
        if(!repo.paths.some(p=>p.path===args.path))repo.paths.push({path:args.path,status:"online",lastSeenAt:1700000001000});
        if(!repo.online){repo.displayPath=args.path;state.connection(true);}
        return structuredClone(repo);
      }
      if(cmd==="repository_remove_location") {
        if(args.path===repo.root)throw {code:"active_location"};
        repo.paths=repo.paths.filter(p=>p.path!==args.path);return structuredClone(repo);
      }
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
      if(cmd==="export_variants")return args.assetIds.map(assetId=>({assetId,variants:(state.edited?["sooc","raw","latest",...(state.more?Array.from({length:8},(_,i)=>"issue:"+(i+7)):["issue:7"])]:["sooc"]).map(variant=>({reference:{assetId,variant},relPath:photo.relPath,name:variant.startsWith("issue:")?"中文定稿 "+variant.split(":")[1]:variant,sourceBase:variant==="raw"?"raw":"sooc",profileHash:variant,main:variant==="latest" || (!state.edited && variant==="sooc"),edited:state.edited,createdAt:1,ordinal:variant.startsWith("issue:")?Number(variant.split(":")[1]):null}))}));
      if(["thumb_get","view_image","export_variant_image"].includes(cmd))return Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII="),c=>c.charCodeAt(0)).buffer;
      return null;
    }};
  function assertOnline(){if(!repo.online)throw {code:"connection_lost"};}
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
    throw new Error(label+": "+JSON.stringify({text:await evaluate("document.body.innerText.slice(0,1200)"),
      calls:await evaluate("window.__exportTest?.calls.slice(-40)"),errors:cdp.consoleErrors,exceptions:cdp.exceptions,
      tile:await evaluate("document.querySelector('[data-export-issue]')?.outerHTML.slice(0,1800)")}));
  };
  await until("[...document.querySelectorAll('[data-part=item]')].some(b=>b.textContent.trim()==='导出')","flowbar startup");
  await until("[...document.querySelectorAll('button')].some(b=>b.textContent.includes('2026-08-15'))","startup repository restored before flow switch");
  const point=await evaluate("(()=>{const el=[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导出');const r=el.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()");
  await cdp.send("Input.dispatchMouseEvent",{type:"mousePressed",button:"left",buttons:1,clickCount:1,...point});
  await cdp.send("Input.dispatchMouseEvent",{type:"mouseReleased",button:"left",buttons:0,clickCount:1,...point});
  await until("document.querySelector('[data-export-issue=\"issue:7\"]')","populated grid requests variants");
  assert(await evaluate("document.querySelectorAll('[data-export-issue]').length===3"),"one photo exposes SOOC, RAW, latest and named issue");
  assert(await evaluate("document.querySelector('[data-export-issue=\"issue:7\"] [data-export-suffix]')?.textContent==='I07'"),"issue tile carries its ordinal suffix badge");
  assert(await evaluate("document.querySelector('[data-export-issue=raw] [data-export-suffix]')?.textContent==='IRA'"),"RAW-marked issue tile carries IRA badge");
  assert(await evaluate("document.querySelector('[data-export-issue=sooc] [data-export-suffix]')?.textContent==='ISO'"),"SOOC tile carries ISO badge");
  await until("[...document.querySelectorAll('[data-export-left] button')].some(b=>b.textContent.includes('2026-08-15'))","directory tree");
  await until("document.querySelector('[data-export-issue=\"issue:7\"] img')", "issue thumbnail ready");
  const keptOffline = await evaluate(`(async()=>{
    const tile=document.querySelector('[data-export-issue="issue:7"]');
    window.__exportTest.connection(false);await new Promise(r=>setTimeout(r,100));
    return tile===document.querySelector('[data-export-issue="issue:7"]');
  })()`);
  assert(keptOffline,"offline event keeps the current export issue DOM and registered library");
  // 同库换盘符：离线齿轮可达，选错不登记，正确位置恢复后仍为同一库。
  const beforeSettings=await evaluate("window.__exportTest.calls.filter(c=>c==='repository_settings').length");
  await evaluate("document.querySelector('[data-export-left] button[aria-label=\"库设置\"]').click()");
  await until("document.querySelector('[data-repository-locations]')", "offline gear opens positions without catalog");
  assert.equal(await evaluate("window.__exportTest.calls.filter(c=>c==='repository_settings').length"),beforeSettings,"known offline settings skip catalog reads");
  assert(await evaluate("document.querySelector('[role=dialog] input')?.disabled && [...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='保存')?.disabled"),"only catalog writes are disabled offline");
  assert(await evaluate("document.querySelector('[data-repository-position]')?.textContent.includes('C:/Photos/demo')"),"offline registration remains visible");
  const addsBefore=await evaluate("window.__exportTest.calls.filter(c=>c==='repository_add_location').length");
  await evaluate("document.querySelector('[data-repository-locate]').click()");
  await until("window.__exportTest.calls.includes('plugin:dialog|open') && !document.querySelector('[data-repository-locate]').disabled", "cancelled picker releases busy state");
  assert.equal(await evaluate("window.__exportTest.calls.filter(c=>c==='repository_add_location').length"),addsBefore,"cancel never adds a position");
  await evaluate("window.__exportTest.pickedPath='F:/另一个库';document.querySelector('[data-repository-locate]').click()");
  await until("document.querySelector('[data-repository-locations] [role=status]')?.textContent.includes('另一个库')", "wrong library has a localized identity reason");
  assert.equal(await evaluate("window.__exportTest.view().paths.length"),1,"identity mismatch leaves registration untouched");
  // 原生选择器晚于关闭返回，不能再发新增命令。
  await evaluate("window.__exportTest.pickerDelay=true;document.querySelector('[data-repository-locate]').click()");
  await until("window.__exportTest.pickerResolve!==null", "delayed native picker started");
  const lateAdds=await evaluate("window.__exportTest.calls.filter(c=>c==='repository_add_location').length");
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='取消').click()");
  await until("!document.querySelector('[data-repository-locations]')", "settings closed before picker result");
  await evaluate("window.__exportTest.pickerResolve('G:/中文照片库');window.__exportTest.pickerDelay=false;window.__exportTest.pickerResolve=null");
  await sleep(100);
  assert.equal(await evaluate("window.__exportTest.calls.filter(c=>c==='repository_add_location').length"),lateAdds,"late picker does not mutate the closed library settings");
  await evaluate("document.querySelector('[data-export-left] button[aria-label=\"库设置\"]').click()");
  await until("document.querySelector('[data-repository-locate]')", "offline settings can reopen");
  await evaluate("window.__exportTest.pickedPath='G:/中文照片库';document.querySelector('[data-repository-locate]').click()");
  await until("document.querySelectorAll('[data-repository-position]').length===2 && document.querySelector('[role=dialog] input')?.disabled===false", "verified alternate location reconnects same library");
  assert(await evaluate("window.__exportTest.view().id==='repro0000000000' && window.__exportTest.view().photosCount===1 && window.__exportTest.view().root==='G:/中文照片库'"),"library identity and last count survive the new root");
  assert(await evaluate("document.querySelector('[data-repository-position=\"G:/中文照片库\"] button').disabled"),"current position cannot be removed");
  await evaluate("document.querySelector('[data-repository-position=\"C:/Photos/demo\"] button[aria-label=\"移除位置\"]').click()");
  await until("[...document.querySelectorAll('[role=dialog]')].some(d=>d.textContent.includes('移除库位置'))", "position removal uses existing confirmation");
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='移除位置').click()");
  await until("document.querySelectorAll('[data-repository-position]').length===1", "removes registration only");
  assert.equal(await evaluate("window.__exportTest.view().root"),'G:/中文照片库');
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='取消').click()");
  await until("!document.querySelector('[data-repository-locations]')", "settings close after recovery");
  assert(!await evaluate("window.__exportTest.calls.includes('repository_create')"),"locating never invokes library creation");
  await until("document.querySelector('[data-export-issue=\"issue:7\"] img')", "new connection generation refreshes the current preview");
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
  for (const [mode] of [['percent','百分比缩小',50],['maxEdge','最大边长',2048],['original','保持不变',0]]) {
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
  await until("document.querySelectorAll('[data-export-issue]').length===3","manual issues scope");
  await evaluate(scopeButton+".click()");
  await until("document.querySelectorAll('[data-export-issue]').length===3","edited scope");
  await evaluate(scopeButton+".click()");
  await until("document.querySelectorAll('[data-export-issue]').length===3","all scope recovery");
  await evaluate("window.__exportTest.more=true;window.__exportTest.change()");
  await until("document.querySelectorAll('[data-export-area=gallery] [data-export-issue]').length===6","six visible small issues");
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='全部定稿').click()");
  await until("document.querySelectorAll('[data-export-all-issues] [data-export-issue]').length===11","modal includes all issues and main");
  const hidden=await evaluate("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]')!==null");assert(hidden);
  await evaluate("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]').click()");
  await until("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]')?.getAttribute('aria-selected')==='true'","modal selection survives update");
  await evaluate("document.querySelector('[data-export-all-issues] [data-export-issue=\"issue:14\"] [role=option]').focus()");
  await cdp.send("Input.dispatchKeyEvent",{type:"keyDown",key:"a",code:"KeyA",modifiers:2});
  await cdp.send("Input.dispatchKeyEvent",{type:"keyUp",key:"a",code:"KeyA",modifiers:2});
  await until("document.querySelectorAll('[data-export-all-issues] [role=option][aria-selected=true]').length===11","modal Ctrl+A selects this photo's full issue list");
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
  const drag=async(dest,source='[data-export-area=gallery] [role=option]',release=true)=>{
    const a=await evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(source)}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',...a});
    await evaluate("window.__pid=null;window.addEventListener('pointerdown',e=>{window.__pid=e.pointerId;},{capture:true,once:true})");
    await cdp.send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...a});
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:a.x+3,y:a.y});
    assert(!await evaluate("!!document.querySelector('[data-export-drag-preview]')"),'normal click movement does not drag');
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:a.x+12,y:a.y});
    await until("document.querySelector('[data-export-drag-preview]')",'drag starts');
    // 幽灵中心就落在指针上（只差 8px 手感偏移）——旧版把卡片摆在右下 100px 开外。
    await until(`(()=>{const el=document.querySelector('[data-export-drag-preview]');if(!el)return false;const r=el.getBoundingClientRect();return Math.abs(r.x+r.width/2-(${a.x+20}))<6&&Math.abs(r.y+r.height/2-(${a.y+8}))<6;})()`,'ghost centred on the pointer');
    const node=await evaluate("(()=>{const el=document.querySelector('[data-export-drag-preview]'),r=el.getBoundingClientRect();window.__dragPreview=el;window.__dragCount=Number(el.querySelector('[data-export-drag-count]').textContent);return {width:r.width,height:r.height,cards:el.querySelectorAll('.rb-export-drag-card').length};})()");
    assert.deepEqual(node,{width:132,height:132,cards:4},'ghost is one card wide with four cards');
    assert(await evaluate("Number.isInteger(window.__dragCount)&&window.__dragCount>0"),'drag badge counts the captured batch');
    if(dest==='outside'){
      // CDP 的合成鼠标事件出不了视口（浏览器直接丢弃越界坐标），所以用一条同 pointerId 的
      // 合成 pointermove 走**真实的那段代码路径** —— 与下面 blur 取消同一个做法。
      await evaluate("window.dispatchEvent(new PointerEvent('pointermove',{pointerId:window.__pid,clientX:-24,clientY:120}))");
      await until("!document.querySelector('[data-export-drag-preview]')",'leaving the window aborts the drag');
      await cdp.send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...a});
      assert(!await evaluate("!!document.querySelector('[data-export-drag-preview]')"),'aborted drag does not come back');
      return;
    }
    const b=await evaluate(`(()=>{
      if(${JSON.stringify(dest)}==='queue'){const r=document.querySelector('[data-export-area=queue]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};}
      const el=document.querySelector('[data-export-preset="${dest}"]');el.scrollIntoView();const r=el.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
    await cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,...b});
    await sleep(100);
    if(dest==='queue'){
      const hover=await evaluate(`(()=>{const el=document.elementFromPoint(${b.x},${b.y});const zone=document.querySelector('[data-export-area=queue]');return {inside:!!el?.closest('[data-export-area=queue]'),drop:zone?.dataset.exportDrop,classes:zone?.className};})()`);
      assert(hover.inside,'pointer hits the queue area: '+JSON.stringify(hover));
      await until("document.querySelector('[data-export-area=queue]')?.dataset.exportDrop==='queue'",'queue lights up as drop target');
      assert(await evaluate("(()=>{const el=document.querySelector('[data-export-area=queue]');return el.classList.contains('border-brand')&&!el.classList.contains('border-surface-layer');})()"),'queue frame uses the primary colour: '+JSON.stringify(hover));
    } else {
      const hover=await evaluate(`(()=>{const el=document.elementFromPoint(${b.x},${b.y});return {hit:el?.outerHTML.slice(0,200),card:el?.closest('[data-export-preset]')?.dataset.exportPreset,classes:document.querySelector('[data-export-preset="${dest}"]')?.className,point:{x:${b.x},y:${b.y}},rect:document.querySelector('[data-export-preset="${dest}"]')?.getBoundingClientRect().toJSON(),list:document.querySelector('[data-export-preset-list]')?.getBoundingClientRect().toJSON(),pointer:getComputedStyle(document.querySelector('[data-export-preset="${dest}"]')).pointerEvents};})()`);
      assert.equal(hover.card,dest,'pointer hits preset: '+JSON.stringify(hover));
      await until(`document.querySelector('[data-export-preset="${dest}"]')?.classList.contains('border-brand-2')`,'drop target highlighted');
    }
    assert(await evaluate("window.__dragPreview===document.querySelector('[data-export-drag-preview]')"),'preview identity survives pointer move');
    if(release)await cdp.send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...b});
    else {await evaluate("window.dispatchEvent(new Event('blur'))");await cdp.send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,...b});}
    await until("!document.querySelector('[data-export-drag-preview]')",'preview cleared');
  };
  await drag('second-preset');
  await until("window.__exportQueue.state().queues['second-preset']?.length===11",'drag includes all eleven selected issues');
  await until("document.querySelector('[data-export-preset-count=\"second-preset\"]').textContent==='11/11'",'animated counter reaches target');
  assert(await evaluate("document.querySelector('[data-export-preset=\"test-preset\"] > button').getAttribute('aria-pressed')==='true'"),'drag does not switch selected preset');
  assert.equal(await evaluate("document.querySelectorAll('[data-export-area=gallery] [role=option][aria-selected=true]').length"),7,'source selection survives drop');
  await drag('second-preset','[data-export-area=gallery] [data-export-issue] [role=option]');
  assert.equal(await evaluate("window.__exportQueue.state().queues['second-preset'].length"),11,'repeat drop deduplicates');
  assert(!await evaluate("document.querySelector('[data-export-preset-count=\"second-preset\"]').classList.contains('rb-export-count-bump')"),'duplicate drop does not animate');
  await drag('test-preset','[data-export-area=gallery] [data-export-issue] [role=option]',false);
  assert.equal(await evaluate("window.__exportQueue.state().queues['test-preset']?.length??0"),0,'blur cancels drop');
  await drag('test-preset');
  await until("window.__exportQueue.state().queues['test-preset']?.length===11",'same selection drops into another preset');
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
  await until("(()=>{const scope=document.querySelector('[data-export-area=queue]');const unfinished=scope.querySelectorAll('[role=option]:not([aria-disabled=true])');return unfinished.length>0 && scope.querySelectorAll('[role=option][aria-selected=true]').length===unfinished.length && [...unfinished].every(el=>el.getAttribute('aria-selected')==='true') && !scope.querySelector('[role=option][aria-disabled=true][aria-selected=true]');})()",'bottom Ctrl+A selects only unfinished');
  await key('Delete','Delete');
  await until("document.querySelectorAll('[data-export-area=queue] [role=option]').length===1",'bottom Delete preserves skipped issue');
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='重置所有队列').click()");
  await until("document.querySelector('[role=dialog]')",'reset drag queues');
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='确定').click()");
  await until("Object.keys(window.__exportQueue.state().queues).length===0",'reset all destination queues');
  // 拖到下方队列区：与 toolsbar「送入队列」同一个落点（当前选中预设），进范围时队列区亮主色框。
  await evaluate("document.querySelector('[data-export-area=gallery] [role=option]').focus()");
  await key('a','KeyA',2);
  await until("document.querySelectorAll('[data-export-area=gallery] [role=option][aria-selected=true]').length===7",'reselect gallery before queue drop');
  await drag('queue');
  await until("Number.isInteger(window.__dragCount)&&window.__exportQueue.state().queues['test-preset']?.length===window.__dragCount",'queue drop enqueues the dragged batch into the selected preset');
  await until("document.querySelector('[data-export-preset-count=\"test-preset\"]').textContent===window.__dragCount+'/'+window.__dragCount",'queue drop animates the same counter');
  assert(!await evaluate("document.querySelector('[data-export-area=queue]').dataset.exportDrop"),'queue highlight clears after drop');
  // 拖动中把指针移出窗口：中止，不投放、不留幽灵。
  await drag('outside');
  assert.equal(await evaluate("window.__exportQueue.state().queues['test-preset'].length"),await evaluate("window.__dragCount"),'window exit drops nothing');
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
  // 预设删除（2026-09-28，卡片右下角的 easy destroy）：队列没干完时拦下；清了队列才放行。
  const presetCount = () => evaluate("document.querySelectorAll('[data-export-preset]').length");
  const deletePresetCard = async (id) => {
    await evaluate(`document.querySelector('[data-export-preset="${id}"] [aria-label="删除预设"]')?.click()`);
    await until("document.querySelector('[role=dialog]')", '删除预设要先确认（easy destroy）');
    await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='删除')?.click()");
  };
  const beforeDelete = await presetCount();
  await deletePresetCard('test-preset');
  await sleep(400);
  assert.equal(await presetCount(), beforeDelete, '队列里还有未完成的条目时删除应当被拦下');
  assert(await evaluate("document.querySelector('[data-export-mid] [role=alert]')?.textContent.includes('队列') ?? document.body.innerText.includes('未完成的条目')"), '拦截要说清原因');
  await evaluate("document.querySelector('[role=dialog] [data-part=close-trigger]')?.click() ?? [...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='取消')?.click()");
  // 清空队列后再删：这次真的少一张，且选中态被清掉
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='重置所有队列').click()");
  await until("document.querySelector('[role=dialog]')",'reset before delete');
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='确定').click()");
  await until("Object.keys(window.__exportQueue.state().queues).length===0",'reset before preset delete');
  await deletePresetCard('test-preset');
  await until(`document.querySelectorAll('[data-export-preset]').length===${beforeDelete - 1}`, '确认后预设从列表消失');
  assert.equal(
    await evaluate("document.querySelector('[data-export-preset=\"second-preset\"] > button')?.getAttribute('aria-pressed')"),
    "false",
    '删掉选中的预设后选中态要清掉',
  );
  await cdp.send("Page.navigate",{url:url+(url.includes('?')?'&':'?')+'export-empty=1'});
  await until("[...document.querySelectorAll('[data-part=item]')].some(b=>b.textContent.trim()==='导出')",'empty fixture startup');
  await until("(()=>{[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导出')?.click();return !!document.querySelector('[data-export-preset-list]');})()",'empty export workflow mounted after startup');
  await until("document.querySelector('[data-export-preset-list]')?.textContent==='当前无预设，请先新建'",'empty preset guidance');
  await until("document.querySelector('[data-export-form-title]') && document.querySelector('[data-export-save]') && document.querySelector('[data-export-run]')",'empty preset form finishes mounting');
  assert.equal(await evaluate("document.querySelector('[data-export-form-title]').textContent.trim()"),'新建预设');
  assert.equal(await evaluate("document.querySelector('[data-export-save]').textContent.trim()"),'新增');
  assert(await evaluate("document.querySelector('[data-export-save]').classList.contains('bg-brand-2')"),'new preset uses accent');
  assert(await evaluate("document.querySelector('[data-export-run] svg')!==null"),'export icon precedes text');
  assert.equal(await evaluate("document.querySelector('[data-export-existing-file] input:checked').value"),'append');
  assert(await evaluate("document.querySelector('[data-export-run]').disabled"));
  assert.deepEqual(cdp.exceptions,[]);assert.deepEqual(cdp.consoleErrors,[]);
  console.log("✓ populated export: bounds, quality visibility, issue selection/queue removal, modal, photo filters, stable library, failure retries");
} finally { cdp?.close();chrome.kill("SIGKILL"); }
