import {createEffect,createSignal,Show,type JSX} from "solid-js";
import {Dialog} from "../../components/ui/Dialog.tsx";
import {Button} from "../../components/ui/Button.tsx";
import {t} from "../../i18n";
import type {ColorBatchReview} from "../../api/color.ts";

/** Review has a fixed backend token; changing selection cannot change the commit scope. */
export function ColorBatchDialog(props:{review:ColorBatchReview|null;onClose:()=>void;onConfirm:(token:string)=>Promise<number>}):JSX.Element {
  const [busy,setBusy]=createSignal(false);const [error,setError]=createSignal<string|null>(null);
  createEffect(()=> {void props.review?.token;setError(null);});
  const commit=async ():Promise<void>=> {
    const review=props.review;if(!review || busy())return;
    setBusy(true);setError(null);
    try {await props.onConfirm(review.token);props.onClose();}catch(error){setError(String(error));}finally{setBusy(false);}
  };
  return <Dialog open={props.review!==null} title={t("editor.colorManagement.batchTitle")} onOpenChange={open=>{if(!open&&!busy()){setError(null);props.onClose();}}}
    footer={<><Button variant="secondary" disabled={busy()} onClick={props.onClose}>{t("common.cancel")}</Button>
      <Button variant="primary" disabled={busy() || !props.review?.applicable || error()!==null} onClick={()=>void commit()}>{t(busy()?"editor.colorManagement.preparing":"editor.colorManagement.batchApply",{count:props.review?.applicable??0})}</Button></>}>
    <Show when={props.review}>{review=><div class="flex flex-col gap-4">
      <p class="text-fg-2">{t("editor.colorManagement.batchScope",{count:review().selected})}</p>
      <div class="rounded-ui bg-surface-bar p-4">
        <p class="mb-3 text-fg-2">{t("editor.colorManagement.batchTarget",{profile:review().profileName??t("editor.colorManagement.automatic")})}</p>
        <p class="font-semibold text-fg-1">{t("editor.colorManagement.batchCompatible",{count:review().applicable,existing:review().existing})}</p>
        <p class="mt-2 text-fg-3">{t("editor.colorManagement.batchSkipped",{count:review().skipped.length})}</p>
      </div>
      <p class="leading-relaxed text-fg-2">{t("editor.colorManagement.batchHint")}</p>
      <Show when={review().skipped.length>0}><div class="max-h-32 overflow-y-auto rounded-ui bg-surface-track p-3 text-fs-0 text-fg-3">{review().skipped.map(reason=><p>{reason}</p>)}</div></Show>
      <Show when={error()}>{message=><p role="alert" class="text-danger">{message()}</p>}</Show>
    </div>}</Show>
  </Dialog>;
}
