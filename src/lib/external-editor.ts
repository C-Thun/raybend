import type {DisplayVariant} from './display-variant.ts';
export interface ExternalApplication {name:string;path:string}
export interface ExternalTarget extends DisplayVariant {name:string}
export interface ExternalTask {id:number;revision:number;status:string;output:string|null;error:string|null;missingApplication:string|null}
export interface ExternalPreferences {version:1;applications:ExternalApplication[];lastApplication:string|null;directory:string}
export const EXTERNAL_PREFERENCES_KEY='external-editor.preferences.v1';
export const emptyExternalPreferences=():ExternalPreferences=>({version:1,applications:[],lastApplication:null,directory:''});
export function applicationKey(path:string):string{
 const value=path.trim().replace(/^"|"$/g,'');
 return /^[a-z]:[\\/]|^\\\\|^\/\//i.test(value)?value.replace(/\\/g,'/').toLowerCase():value;
}
export function readExternalPreferences(raw:string|null):ExternalPreferences {
 if(raw===null)return emptyExternalPreferences();
 const value=JSON.parse(raw);
 if(value?.version!==1||!Array.isArray(value.applications)||value.applications.length>128||typeof value.directory!=='string'||value.directory.includes('\0')||(value.lastApplication!==null&&typeof value.lastApplication!=='string'))throw Error('EXTERNAL_INVALID_PREFERENCES');
 const seen=new Set<string>();
 const applications:ExternalApplication[]=value.applications.filter((app:ExternalApplication)=>{
  if(typeof app?.name!=='string'||!app.name.trim()||app.name.length>120||typeof app?.path!=='string'||!app.path||app.path.includes('\0'))return false;
  const key=applicationKey(app.path);if(seen.has(key))return false;seen.add(key);return true;
 });
 return {version:1,applications,lastApplication:value.lastApplication,directory:value.directory};
}
export function externalRunning(task:ExternalTask):boolean{return ['preparing','rendering','writing','opening'].includes(task.status);}
export const emptyExternalTask=():ExternalTask=>({id:0,revision:0,status:'idle',output:null,error:null,missingApplication:null});
