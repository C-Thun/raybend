import {For,Show} from 'solid-js';
import {Dialog} from '../../components/ui/Dialog.tsx';
import {Button} from '../../components/ui/Button.tsx';
import {Checkbox,Input} from '../../components/ui/Form.tsx';
import {pickDirectory,pickOpenFile} from '../../api/dialog.ts';
import {applicationKey} from '../../lib/external-editor.ts';
import {t} from '../../i18n/index.ts';
import type {ExternalEditorStore} from './store.ts';
export function ExternalEditorDialog(props:{store:ExternalEditorStore}){
 const store=props.store;
 const error=()=>{const value=store.error()??store.task().error;if(value?.includes('EXTERNAL_INVALID_PREFERENCES'))return t('external.invalidPreferences');if(value?.includes('EXTERNAL_APPLICATION_UNAVAILABLE'))return t('external.unavailable');return value;};
 const phase=()=>{const status=store.task().status;return ['preparing','rendering','writing','opening','done','cancelled','failed'].includes(status)?t(`external.status.${status}` as 'external.status.preparing'):'';};
 const manual=async()=>{try{const path=await pickOpenFile({title:t('external.manual'),filters:[{name:t('external.executable'),extensions:['exe']}]});if(path)await store.add([{name:path.split(/[\\/]/).slice(-1)[0]?.replace(/\.exe$/i,'')??path,path}]);}catch(e){store.reportError(e);}};
 return <>
  <Dialog open={store.open()} title={t('external.title')} onOpenChange={open=>{if(!open)store.close();}}>
   <div data-external-editor class="flex flex-col gap-3">
    <p class="truncate text-fs-1 text-fg-2">{store.target()?.name}</p>
    <div class="flex items-center gap-2">
     <select data-external-application class="h-row-h min-w-0 flex-1 rounded-ui bg-surface-track px-2 text-fs-1 text-fg-1" aria-label={t('external.application')} disabled={store.loading()||store.running()||store.busy()}
       value={store.selectedApplication()?.path??''} onChange={event=>void store.chooseApplication(event.currentTarget.value)}>
      <Show when={store.preferences().applications.length===0}><option value="">{t('external.noApplications')}</option></Show>
      <For each={store.preferences().applications}>{app=><option value={app.path}>{app.name}</option>}</For>
     </select>
     <Button data-external-add disabled={store.loading()||store.running()||store.busy()} onClick={()=>void store.discover()}>{t('external.addApplication')}</Button>
    </div>
    <label class="flex flex-col gap-1 text-fs-1 text-fg-2">{t('external.directory')}
     <span class="flex gap-2"><Input data-external-directory class="min-w-0 flex-1" value={store.preferences().directory} disabled={store.loading()||store.running()||store.busy()} onInput={event=>void store.directory(event.currentTarget.value)}/>
      <Button disabled={store.loading()||store.running()||store.busy()} onClick={()=>void pickDirectory({title:t('external.directory')}).then(dir=>{if(dir!==null)return store.directory(dir);}).catch(store.reportError)}>{t('external.browseDirectory')}</Button>
     </span>
    </label>
    <Show when={phase()}><p data-external-status role="status" class="text-fs-1 text-fg-2">{phase()}</p></Show>
    <Show when={store.task().output}><p data-external-output class="break-all text-fs-1 text-fg-2">{t('external.saved',{path:store.task().output!})}</p></Show>
    <Show when={error()}><p role="alert" class="break-words text-fs-1 text-danger">{error()}</p></Show>
    <p class="text-fs-0 text-fg-3">{t('external.hint')}</p>
    <div class="flex justify-end gap-2">
     <Button disabled={store.busy()} onClick={()=>{if(store.running())void store.cancel();else store.close();}}>{t(store.running()?'common.cancel':'common.close')}</Button>
     <Button data-external-start variant="primary" disabled={!store.canStart()} loading={store.busy()} onClick={()=>void store.start()}>{t('external.confirm')}</Button>
    </div>
   </div>
  </Dialog>
  <Dialog open={store.adding()} title={t('external.addTitle')} scrim={false} onOpenChange={store.setAdding}>
   <div class="flex flex-col gap-3">
    <Show when={store.discovering()} fallback={<div data-external-candidates class="flex max-h-72 flex-col gap-2 overflow-auto">
     <For each={store.candidates()} fallback={<p class="text-fs-1 text-fg-3">{t('external.notFound')}</p>}>{app=><div class="flex min-w-0 items-start gap-2">
      <Checkbox checked={store.registered(app.path)||store.picked().has(applicationKey(app.path))} disabled={store.registered(app.path)||store.busy()} label={app.name} onCheckedChange={()=>store.toggle(app.path)}/>
      <span class="min-w-0 flex-1 truncate text-fs-0 text-fg-3" title={app.path}>{store.registered(app.path)?t('external.registered'):app.path}</span>
     </div>}</For>
    </div>}><p role="status" class="text-fs-1 text-fg-3">{t('common.loading')}</p></Show>
    <Show when={error()}><p role="alert" class="text-fs-1 text-danger">{error()}</p></Show>
    <div class="flex justify-end gap-2"><Button disabled={store.discovering()||store.busy()} onClick={()=>void store.discover()}>{t('external.rediscover')}</Button><Button disabled={store.busy()} onClick={()=>void manual()}>{t('external.manual')}</Button><Button variant="primary" disabled={store.picked().size===0||store.busy()} onClick={()=>void store.addPicked()}>{t('external.addSelected')}</Button></div>
   </div>
  </Dialog>
 </>;
}
