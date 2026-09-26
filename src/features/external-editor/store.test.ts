import test from 'node:test';import assert from 'node:assert/strict';
import {createExternalEditorStore,type ExternalEditorDeps} from './store.ts';
import {applicationKey,emptyExternalTask,EXTERNAL_PREFERENCES_KEY,type ExternalTask} from '../../lib/external-editor.ts';
const app={name:'编辑器',path:'C:/Editor/app.exe'},target={repositoryId:'repo',reference:{assetId:1,variant:'sooc'},name:'SOOC'};
const captured={reference:target.reference,name:'SOOC',relPath:'照片.png',profileHash:'sooc',sourceSignature:'signature',stack:{values:{}}};
function setup(overrides:Partial<ExternalEditorDeps>={}){
 const writes:string[]=[],calls:string[]=[];let handler!:(task:ExternalTask)=>void;
 const store=createExternalEditorStore({getSetting:async()=>JSON.stringify({version:1,applications:[app],lastApplication:app.path,directory:'D:/照片'}),setSetting:async(key,value)=>{assert.equal(key,EXTERNAL_PREFERENCES_KEY);writes.push(value);},
 applications:async(action,apps)=>action==='check'?[...new Map((apps??[]).map(app=>[applicationKey(app.path),app])).values()]:[app],task:async(action,request)=>{calls.push(action);if(action==='start'){assert.equal(request?.captured,captured);return {...emptyExternalTask(),id:1,status:'rendering'};}return emptyExternalTask();},
 snapshot:async()=>captured,subscribe:async(callback)=>{handler=callback;return()=>calls.push('unsub');},...overrides});
 return{store,writes,calls,notify:(view:ExternalTask)=>handler(view)};
}
test('confirm requires registration/directory, writes only preferences and starts one captured draft',async()=>{
 const {store,writes,calls}=setup();await store.ready;assert(!store.canStart());await store.show(target);assert(store.canStart());await store.start();assert(store.running());assert(!store.canStart());
 store.close();assert(store.running());await store.start();assert.equal(calls.filter(call=>call==='start').length,1);assert.equal(writes.length,0);store.dispose();
 const empty=setup({getSetting:async()=>null});await empty.store.ready;await empty.store.show(target);assert(!empty.store.canStart());await empty.store.start();assert(!empty.calls.includes('start'));empty.store.dispose();
});
test('old responses do not regress task phase, missing app is pruned and next app selected',async()=>{
 const second={name:'下一项',path:'C:/Other/app.exe'};const {store,notify,writes}=setup({getSetting:async()=>JSON.stringify({version:1,applications:[app,second],lastApplication:app.path,directory:'D:/照片'})});await store.ready;
 notify({...emptyExternalTask(),id:3,revision:4,status:'failed',output:'D:/照片/1.tiff',missingApplication:app.path,error:'missing'});
 notify({...emptyExternalTask(),id:3,revision:3,status:'rendering'});assert.equal(store.task().status,'failed');assert.equal(store.task().output,'D:/照片/1.tiff');
 await new Promise(resolve=>setImmediate(resolve));assert.equal(store.selectedApplication()?.path,second.path);assert.equal(JSON.parse(writes[0]).applications.length,1);store.dispose();
});
test('concurrent preference edits serialize current values; failed persistence prevents fake confirmation',async()=>{
 const {store,writes}=setup();await store.ready;const one=store.directory('D:/新目录'),two=store.chooseApplication(app.path);assert(!store.canStart());await Promise.all([one,two]);assert.equal(JSON.parse(writes[writes.length-1]).directory,'D:/新目录');store.dispose();
 const broken=setup({setSetting:async()=>{throw Error('disk full');}});await broken.store.ready;await broken.store.directory('D:/new');assert.equal(broken.store.preferences().directory,'D:/照片');assert.match(broken.store.error()!,/disk full/);broken.store.dispose();
});
test('discovery and add use one checked path registration; restart removes missing registrations',async()=>{
 const {store}=setup();await store.ready;await store.discover();assert.equal(store.candidates().length,1);assert(store.registered(app.path));
 await store.add([{name:'Same',path:app.path}]);assert.equal(store.preferences().applications.length,1);store.dispose();
 const pruned=setup({applications:async()=>[]});await pruned.store.ready;assert.equal(pruned.store.preferences().applications.length,0);assert.equal(pruned.store.selectedApplication(),null);pruned.store.dispose();
});

test('pending or invalid settings cannot overwrite registrations or remembered directory',async()=>{
 let release!:(value:string|null)=>void;const pending=setup({getSetting:()=>new Promise(resolve=>{release=resolve;})});
 await pending.store.directory('D:/too-early');await pending.store.chooseApplication('C:/wrong.exe');assert.equal(pending.writes.length,0);
 release(JSON.stringify({version:1,applications:[app],lastApplication:app.path,directory:'D:/saved'}));await pending.store.ready;assert.equal(pending.store.preferences().directory,'D:/saved');pending.store.dispose();
 const future=setup({getSetting:async()=>JSON.stringify({version:99})});await future.store.ready;await future.store.directory('D:/overwrite');await future.store.add([app]);await future.store.show(target);assert(!future.store.canStart());assert.equal(future.writes.length,0);future.store.dispose();
});
