import { createSignal, For, Show } from "solid-js";
import { Dialog, ConfirmDialog } from "../components/ui/Dialog.tsx";
import { Button } from "../components/ui/Button.tsx";
import { Input } from "../components/ui/Form.tsx";
import { t } from "../i18n/index.ts";
import { updateCheck, updateDownload, updateInstall, type AvailableUpdate } from "../api/updates.ts";
import { currentBuildInfo } from "../lib/build-info.ts";
import { UPDATE_KEY, UPDATE_ENDPOINTS, defaultUpdateSource, readUpdateSource, validUpdateSource, type UpdateMode, type UpdateSource } from "../lib/update-source.ts";
export function UpdateDialog(props:{open:boolean;onOpenChange:(open:boolean)=>void;canInstall:()=>boolean}) {
  const fallback=defaultUpdateSource(__RAYBEND_UPDATER_PUBLIC_KEY__,currentBuildInfo().channel);
  let raw:string|null=null;try{raw=localStorage.getItem(UPDATE_KEY);}catch{/* 偏好读失败不影响应用。 */}
  const [source,setSource]=createSignal(readUpdateSource(raw,fallback));
  const [available,setAvailable]=createSignal<AvailableUpdate|null>(null);
  const [state,setState]=createSignal<"idle"|"checking"|"current"|"available"|"downloading"|"downloaded"|"installing"|"error">("idle");
  const [error,setError]=createSignal("");const [confirm,setConfirm]=createSignal(false);
  const busy=()=>["checking","downloading","installing"].includes(state());
  const storeManaged=()=>__RAYBEND_DISTRIBUTION__==="store";
  function change(next:UpdateSource):void{setSource(next);setAvailable(null);setState("idle");setError("");try{localStorage.setItem(UPDATE_KEY,JSON.stringify(next));}catch{setError(t("updates.storage_error"));}}
  function mode(next:UpdateMode):void{change(next==="off"?{mode:"off",endpoint:"",publicKey:""}:next==="custom"?{...source(),mode:"custom"}:{mode:next,endpoint:UPDATE_ENDPOINTS[next],publicKey:__RAYBEND_UPDATER_PUBLIC_KEY__});}
  async function act(action:"check"|"download"|"install"):Promise<void>{
    if(busy())return;
    if(action==="install" && !props.canInstall()){setError(t("updates.tasks_active"));return;}
    const selected=source();setError("");setState(action==="check"?"checking":action==="download"?"downloading":"installing");
    try{if(action==="check"){setAvailable(null);const update=await updateCheck(selected);setAvailable(update);setState(update?"available":"current");}else if(action==="download"){await updateDownload(selected);setState("downloaded");}else {await updateInstall(selected);setState("downloaded");}}
    catch(caught){setError(t("updates.failed",{message:String(caught)}));setState("error");}
  }
  return <><Dialog open={props.open} onOpenChange={open=>{if(!busy())props.onOpenChange(open);}} title={t("updates.title")} footer={<>
    <Button variant="secondary" disabled={busy()} onClick={()=>props.onOpenChange(false)}>{t("common.close")}</Button>
    <Show when={state()==="available"}><Button onClick={()=>void act("download")}>{t("updates.download")}</Button></Show>
    <Show when={state()==="downloaded"}><Button disabled={!props.canInstall()} onClick={()=>setConfirm(true)}>{t("updates.install")}</Button></Show>
    <Button variant="secondary" loading={busy()} disabled={busy()||storeManaged()||!validUpdateSource(source())} onClick={()=>void act("check")}>{t("updates.check")}</Button>
  </>}><div class="flex flex-col gap-3">
    <p class="text-fs-1 text-fg-2">{t(storeManaged()?"updates.store":"updates.privacy")}</p>
    <div class="flex flex-wrap gap-1"><For each={["off","stable","beta","custom"] as const}>{value=><Button variant={source().mode===value?"primary":"secondary"} disabled={busy()||storeManaged()} onClick={()=>mode(value)}>{t(`updates.mode.${value}`)}</Button>}</For></div>
    <Show when={source().mode==="custom"}><label>{t("updates.endpoint")}<Input disabled={busy()} value={source().endpoint} onInput={e=>change({...source(),endpoint:e.currentTarget.value})}/></label><label>{t("updates.public_key")}<Input disabled={busy()} value={source().publicKey} onInput={e=>change({...source(),publicKey:e.currentTarget.value})}/></label></Show>
    <p aria-live="polite" class="text-fs-1">{t(`updates.state.${state()}`)} <Show when={available()}>{update=><span>{update().version}</span>}</Show></p>
    <Show when={source().mode!=="off" && !validUpdateSource(source())}><p class="text-fs-1 text-fg-3">{t("updates.not_configured")}</p></Show>
    <Show when={available()?.notes}><p class="max-h-40 overflow-auto whitespace-pre-wrap text-fs-1 text-fg-2">{available()?.notes}</p></Show>
    <Show when={!props.canInstall()}><p class="text-fs-1 text-fg-3">{t("updates.tasks_active")}</p></Show>
    <Show when={error()}><p role="alert" class="text-fs-1 text-danger">{error()}</p></Show>
  </div></Dialog><ConfirmDialog open={confirm()} scrim={false} title={t("updates.install")} message={t("updates.confirm")} onCancel={()=>setConfirm(false)} onConfirm={()=>{setConfirm(false);void act("install");}}/></>;
}
