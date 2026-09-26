/** App-owned dialog state. The actual worker belongs to Rust and survives workflow changes. */
import {createSignal,batch} from 'solid-js';
import {applicationKey,emptyExternalPreferences,readExternalPreferences,EXTERNAL_PREFERENCES_KEY,emptyExternalTask,externalRunning,type ExternalApplication,type ExternalTarget,type ExternalTask} from '../../lib/external-editor.ts';
import type {VariantSnapshot} from '../../lib/export-model.ts';
import type {ExternalRequest} from '../../api/external-editor.ts';
export interface ExternalEditorDeps {
 getSetting(key:string):Promise<string|null>;setSetting(key:string,value:string):Promise<void>;
 applications(action:'discover'|'check',applications?:readonly ExternalApplication[]):Promise<ExternalApplication[]>;
 task(action:'status'|'start'|'cancel',request?:ExternalRequest):Promise<ExternalTask>;
 subscribe(handler:(task:ExternalTask)=>void):Promise<()=>void>;
 snapshot(target:ExternalTarget):Promise<VariantSnapshot>;
}
export function createExternalEditorStore(deps:ExternalEditorDeps){
 const [preferences,setPreferences]=createSignal(emptyExternalPreferences());
 const [open,setOpen]=createSignal(false),[adding,setAdding]=createSignal(false);
 const [target,setTarget]=createSignal<ExternalTarget|null>(null);
 const [task,setTask]=createSignal(emptyExternalTask());
 const [busy,setBusy]=createSignal(false),[loading,setLoading]=createSignal(true),[discovering,setDiscovering]=createSignal(false);
 const [saving,setSaving]=createSignal(false);let pendingWrites=0;
 const [error,setError]=createSignal<string|null>(null),[candidates,setCandidates]=createSignal<ExternalApplication[]>([]);
 const [picked,setPicked]=createSignal<ReadonlySet<string>>(new Set());
 let settingsValid=true;
 let disposed=false,off:(()=>void)|undefined,writes=Promise.resolve(),discoveryTicket=0,openTicket=0;
 const selectedApplication=()=>preferences().applications.find(app=>applicationKey(app.path)===applicationKey(preferences().lastApplication??''))??preferences().applications[0]??null;
 const running=()=>externalRunning(task());
 async function persist(next=preferences()){
  const old=preferences();setPreferences(next);pendingWrites++;setSaving(true);
  const write=writes.catch(()=>{}).then(()=>deps.setSetting(EXTERNAL_PREFERENCES_KEY,JSON.stringify(next)));writes=write;
  try{await write;}catch(error){if(!disposed&&preferences()===next)setPreferences(old);throw error;}
  finally{pendingWrites--;if(!disposed)setSaving(pendingWrites>0);}
 }
 async function prune(path:string){const prefs=preferences();const apps=prefs.applications.filter(app=>applicationKey(app.path)!==applicationKey(path));await persist({...prefs,applications:apps,lastApplication:apps.some(app=>applicationKey(app.path)===applicationKey(prefs.lastApplication??''))?prefs.lastApplication:apps[0]?.path??null});}
 function applyTask(next:ExternalTask){
  const old=task();if(disposed||next.id<old.id||(next.id===old.id&&next.revision<old.revision))return;
  setTask(next);
  if(next.missingApplication)void prune(next.missingApplication).catch(e=>{if(!disposed)setError(String(e));});
 }
 const ready=(async()=>{
  try{const prefs=readExternalPreferences(await deps.getSetting(EXTERNAL_PREFERENCES_KEY));if(disposed)return;setPreferences(prefs);
   const apps=await deps.applications('check',prefs.applications);if(disposed)return;
   if(JSON.stringify(apps)!==JSON.stringify(prefs.applications))await persist({...prefs,applications:apps,lastApplication:apps.find(app=>applicationKey(app.path)===applicationKey(prefs.lastApplication??''))?.path??apps[0]?.path??null});
  }catch(e){settingsValid=false;if(!disposed)setError(String(e));}
  finally{if(!disposed)setLoading(false);}
 })();
 void deps.subscribe(applyTask).then(unsub=>{if(disposed)unsub();else off=unsub;}).then(()=>deps.task('status')).then(applyTask).catch(e=>{if(!disposed)setError(String(e));});
 const canStart=()=>settingsValid&&!loading()&&!saving()&&!busy()&&!running()&&target()!==null&&selectedApplication()!==null&&preferences().directory.trim()!=='';
 async function add(apps:readonly ExternalApplication[]){if(!settingsValid||busy()||loading())return;setBusy(true);setError(null);try{
   const checked=await deps.applications('check',[...preferences().applications,...apps]);
   await persist({...preferences(),applications:checked,lastApplication:selectedApplication()?.path??checked[0]?.path??null});
   if(apps.length>0&&!checked.some(app=>apps.some(input=>applicationKey(input.path)===applicationKey(app.path))))throw Error('EXTERNAL_APPLICATION_UNAVAILABLE');
   setAdding(false);
  }catch(e){setError(String(e));}finally{setBusy(false);}}
 return {preferences,open,adding,setAdding,target,task,busy,loading,discovering,error,candidates,picked,selectedApplication,running,ready,canStart,
  async show(value:ExternalTarget){const ticket=++openTicket;setOpen(true);setError(null);if(!running())setTarget(value);try{const view=await deps.task('status');if(ticket===openTicket)applyTask(view);}catch(e){setError(String(e));}},
  close(){setOpen(false);setAdding(false);},
  async chooseApplication(path:string){if(!settingsValid||loading()||disposed)return;try{await persist({...preferences(),lastApplication:path});}catch(e){setError(String(e));}},
  async directory(value:string){if(!settingsValid||loading()||disposed)return;try{await persist({...preferences(),directory:value});}catch(e){setError(String(e));}},
  async discover(){const ticket=++discoveryTicket;batch(()=>{setAdding(true);setDiscovering(true);setPicked(new Set<string>());setError(null);});try{const result=await deps.applications('discover');if(!disposed&&ticket===discoveryTicket)setCandidates(result);}catch(e){if(ticket===discoveryTicket)setError(String(e));}finally{if(ticket===discoveryTicket)setDiscovering(false);}},
  registered(path:string){return preferences().applications.some(app=>applicationKey(app.path)===applicationKey(path));},
  toggle(path:string){setPicked(old=>{const next=new Set(old),key=applicationKey(path);if(next.has(key))next.delete(key);else next.add(key);return next;});},
  add,
  async addPicked(){await add(candidates().filter(app=>picked().has(applicationKey(app.path))));},
  async start(){if(!canStart())return;const destination={...target()!},app={...selectedApplication()!},directory=preferences().directory;setBusy(true);setError(null);
   try{await writes;const captured=destination.captured??await deps.snapshot(destination);applyTask(await deps.task('start',{repositoryId:destination.repositoryId,captured,directory,application:app}));}
   catch(e){setError(String(e));}finally{setBusy(false);}},
  async cancel(){try{applyTask(await deps.task('cancel'));}catch(e){setError(String(e));}},
  reportError(e:unknown){setError(String(e));},
  dispose(){disposed=true;openTicket++;discoveryTicket++;off?.();},
 };
}
export type ExternalEditorStore=ReturnType<typeof createExternalEditorStore>;
