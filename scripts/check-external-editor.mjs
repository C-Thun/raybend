/** Repeatable DOM/API smoke with synthetic payloads, not real-editor/visual E2E. */
import assert from 'node:assert/strict';
import {launchChrome,connectCdp,sleep} from './lib/cdp.mjs';
const url=process.argv[2]??'http://localhost:1420/',port=Number(process.env.CDP_PORT??9518);
assert((await fetch(url)).ok,'Start pnpm dev');
function backend(){
 const repo={id:'repro0000000000',name:'测试库',importTemplate:':FILENAME',createdAt:1,lastOpenedAt:null,online:true,root:'C:/Photos/demo',displayPath:'C:/Photos/demo',paths:[],photosCount:1,imagesCount:1,triedPaths:0};
 const photo={id:1,relPath:'photos/测试.JPG',fileName:'测试.JPG',ext:'jpg',isRaw:false,hasRaw:true,width:4000,height:3000,orientation:1,sizeBytes:1000,missing:false,createdMs:1789000000000,takenAt:1789000000000,takenAtOffsetMin:null,rating:0,colorLabel:null,likeState:null,lockLevel:0,cameraMake:null,cameraModel:null,lens:null,focalMm:null,fNumber:null,exposureMs:null,iso:null,author:null,description:null,gpsLat:null,gpsLon:null,country:null,provinceState:null,city:null,sublocation:null};
 const apps=[{name:'编辑器 A',path:'C:/Apps/a.exe'},{name:'编辑器 B',path:'C:/Apps/b.exe'}];
 const handlers=new Map(),settings=new Map();let next=0,job={id:0,revision:0,status:'idle',output:null,error:null,missingApplication:null};
 const bytes=()=>Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='),c=>c.charCodeAt(0)).buffer;
 const snapshot=reference=>({reference,name:'柔光',relPath:photo.relPath,profileHash:reference.variant,sourceSignature:'original',stack:{sourceBase:'sooc',values:{exposure:1}}});
 const test=window.__externalTest={calls:[],requests:[],finish(){job={...job,revision:job.revision+1,status:'done',output:'D:/Output/测试.tiff'};for(const h of handlers.values())if(h.event==='external-editor://state')h.handler({payload:structuredClone(job)});},settings};
 localStorage.clear();localStorage.setItem('raybend.browse-session.v1',JSON.stringify({repositoryId:repo.id,scopePath:'photos'}));
 window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
 window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:cb=>cb,unregisterCallback:()=>{},convertFileSrc:p=>p,invoke:async(cmd,args={})=>{
  test.calls.push(cmd);
  if(cmd==='plugin:event|listen'){const id=++next;handlers.set(id,args);return id;}if(cmd==='plugin:event|unlisten'){handlers.delete(args.eventId);return;}
  if(cmd==='repositories_list')return [structuredClone(repo)];
  if(cmd==='setting_get')return settings.get(args.key)??null;if(cmd==='setting_set'){settings.set(args.key,args.value);return;}
  if(cmd==='external_applications')return args.action==='discover'?apps:[...new Map((args.applications??[]).map(app=>[app.path.toLowerCase(),app])).values()];
  if(cmd==='external_task'){if(args.action==='start'){test.requests.push(structuredClone(args.request));job={...job,id:job.id+1,revision:0,status:'rendering'};}if(args.action==='cancel'){job={...job,revision:job.revision+1,status:'cancelled'};}return structuredClone(job);}
  if(cmd==='export_queue')return {revision:0,generation:0,queues:{},enabled:[]};if(cmd==='export_preset_validate')return {errors:{},warnings:[]};if(cmd==='export_variants')return [];
  if(cmd==='issue_library')return {issues:[{id:7,name:'柔光',profileHash:'issue:7',sourceBase:'sooc',createdAt:1,stack:{values:{exposure:1}}}],selection:'latest',canFinalize:true,suggestedName:'定稿2',snapshotError:null};
  if(cmd==='export_snapshots')return args.references.map(snapshot);
  if(cmd==='export_variant_details')return {width:2000,height:3000,histogram:{bins:1,r:[1],g:[1],b:[1],luma:[1],max:1}};
  if(cmd==='browse_page')return {total:1,offset:0,items:[structuredClone(photo)]};if(cmd==='browse_timeline')return {total:1,entries:[{id:1,relPath:photo.relPath,takenAt:photo.takenAt}]};
  if(cmd==='browse_facets')return {ratings:[],colors:[],likes:[],locks:[]};if(cmd==='browse_flags_get')return {total:0,picks:[],rejects:[]};
  if(['volumes_list','recent_dirs_list','browse_markings','tag_list'].includes(cmd))return [];
  if(cmd==='dir_list')return args.path.replaceAll('\\','/')===repo.root?[{name:'photos',path:repo.root+'/photos',hasChildren:false}]:[];
  if(cmd==='repository_sync_dir')return [[1,1],[1,1]];
  if(cmd==='dir_meta_ensure')return (args.files??[]).map(file=>({relative:file.relative,width:4000,height:3000,orientation:1}));
  if(cmd==='file_exif')return {...photo,tags:[],software:null,orientation:1};
  if(['thumb_get','view_image','export_variant_image'].includes(cmd))return bytes();
  return null;
 }};
}
const chrome=launchChrome({port});let cdp;
try{
 cdp=await connectCdp(port);await cdp.send('Runtime.enable');await cdp.send('Page.enable');

 await cdp.send('Page.addScriptToEvaluateOnNewDocument',{source:`(${backend.toString()})();`});await cdp.send('Page.navigate',{url});
 const evaluate=async expression=>{const result=await cdp.send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(result.exceptionDetails)throw Error(result.exceptionDetails.exception?.description??result.exceptionDetails.text);return result.result?.value;};
 const until=async(expression,label)=>{const limit=Date.now()+60000;while(Date.now()<limit){if(await evaluate(expression))return;await sleep(100);}throw Error(label+': '+JSON.stringify({page:await evaluate('({url:location.href,ready:document.readyState,html:document.documentElement.outerHTML.slice(0,1000),calls:window.__externalTest?.calls})'),body:await evaluate('document.body.innerText.slice(0,1600)'),exceptions:cdp.exceptions,console:cdp.consoleErrors}));};
 await until("document.querySelector('main [data-virtual-scroller] [role=option]')",'browse ready');
 await evaluate("document.querySelector('main [data-virtual-scroller] [role=option]').click()");
 await until("document.querySelector('[data-browse-issue-choice] select option[value=\"issue:7\"]')",'draft dropdown');
 assert.equal(await evaluate("document.querySelector('[data-browse-issue-choice] select').value"),'latest');
 await evaluate("(()=>{const s=document.querySelector('[data-browse-issue-choice] select');s.value='issue:7';s.dispatchEvent(new Event('change',{bubbles:true}));})()");
 await until("document.querySelector('[data-browse-issue-choice] select').value==='issue:7'&&!document.querySelector('[data-browse-issue-choice] select').disabled",'displayed draft ready');
 assert.equal(await evaluate("window.__externalTest.calls.filter(cmd=>cmd.includes('commit')||cmd==='issue_select').length"),0,'display choice never commits latest');
 assert(await evaluate("window.__externalTest.calls.includes('export_variant_image')"),'grid uses displayed variant');
 await evaluate("document.querySelector('[data-browse-external-editor]').click()");
 await until("document.querySelector('[data-external-editor]')",'external dialog');assert(await evaluate("document.querySelector('[data-external-start]').disabled"),'unregistered apps prevent confirm');
 await evaluate("document.querySelector('[data-external-add]').click()");await until("document.querySelector('[data-external-candidates] input')",'discovered apps');
 await evaluate("document.querySelector('[data-external-candidates] label').click()");
 await until("[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='添加所选'&&!b.disabled)",'candidate checkbox');
 await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='添加所选').click()");
 await until("document.querySelector('[data-external-application]').value==='C:/Apps/a.exe'",'registered app');
 await evaluate("(()=>{const input=document.querySelector('[data-external-directory]');input.value='D:/Output';input.dispatchEvent(new Event('input',{bubbles:true}));})()");await until("!document.querySelector('[data-external-start]').disabled",'directory ready');
 await evaluate("document.querySelector('[data-external-start]').click()");await until("document.querySelector('[data-external-status]').textContent.includes('16-bit TIFF')",'background rendering');
 assert.equal(await evaluate("window.__externalTest.requests[0].captured.reference.variant"),'issue:7');
 assert(await evaluate("document.querySelector('[data-external-start]').disabled"),'cannot duplicate running job');
 await evaluate("[...document.querySelectorAll('button')].find(b=>b.getAttribute('aria-label')==='关闭').click()");
 await evaluate("[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导出').click()");await until("document.querySelector('[data-export-workspace]')",'switch flow');assert.equal(await evaluate("!!document.querySelector('[data-browse-external-editor]')"),false);
 await evaluate('window.__externalTest.finish()');
 await evaluate("[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='浏览').click()");await until("document.querySelector('main [data-virtual-scroller] [role=option]')",'return browse');await evaluate("document.querySelector('main [data-virtual-scroller] [role=option]').click()");await until("document.querySelector('[data-browse-issue-choice] select')",'selector remount');assert.equal(await evaluate("document.querySelector('[data-browse-issue-choice] select').value"),'latest');
 await evaluate("document.querySelector('[data-browse-external-editor]').click()");await until("document.querySelector('[data-external-output]')?.textContent.includes('D:/Output/测试.tiff')",'completed background receipt');
 assert.deepEqual(cdp.exceptions,[]);assert.deepEqual(cdp.consoleErrors,[]);console.log('✓ external editor DOM/API smoke: display-only draft, registration gate, captured TIFF target, background completion, scope reset');
}finally{cdp?.close();chrome.kill("SIGKILL");}
