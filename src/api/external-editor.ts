import {isTauriRuntime} from './tauri-env.ts';
import {onTauriEvent} from './events.ts';
import type {ExternalApplication,ExternalTask} from '../lib/external-editor.ts';
import {emptyExternalTask} from '../lib/external-editor.ts';
import type {VariantSnapshot} from '../lib/export-model.ts';
let core:Promise<typeof import('@tauri-apps/api/core')>|undefined;
const call=<T>(command:string,args:Record<string,unknown>):Promise<T>=>{core??=import('@tauri-apps/api/core');return core.then(module=>module.invoke<T>(command,args));};
export async function externalApplications(action:'discover'|'check',applications?:readonly ExternalApplication[]):Promise<ExternalApplication[]>{return isTauriRuntime()?call('external_applications',{action,applications:applications??null}):[];}
export interface ExternalRequest {repositoryId:string;captured:VariantSnapshot;directory:string;application:ExternalApplication}
export async function externalTask(action:'status'|'start'|'cancel',request?:ExternalRequest):Promise<ExternalTask>{if(!isTauriRuntime()){if(action==='start')throw Error('External editing requires the desktop app');return emptyExternalTask();}return call('external_task',{action,request:request??null});}
export const onExternalTask=(handler:(task:ExternalTask)=>void)=>onTauriEvent('external-editor://state',handler);
