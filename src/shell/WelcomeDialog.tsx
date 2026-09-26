import { Dialog } from "../components/ui/Dialog.tsx";
import { Button } from "../components/ui/Button.tsx";
import { t } from "../i18n/index.ts";
export function WelcomeDialog(props: {open:boolean; onClose:()=>void; onLibrary:()=>void; onHelp:()=>void}) {
  return <Dialog open={props.open} onOpenChange={open=>{if(!open)props.onClose();}} title={t("welcome.title")} footer={<>
    <Button variant="secondary" onClick={props.onClose}>{t("welcome.later")}</Button>
    <Button variant="secondary" onClick={props.onHelp}>{t("welcome.help")}</Button>
    <Button variant="primary" onClick={props.onLibrary}>{t("welcome.library")}</Button>
  </>}><div class="flex flex-col gap-3">
    <p>{t("welcome.intro")}</p><p class="text-fs-1 text-fg-2">{t("welcome.local")}</p>
    <p class="rounded-ui bg-surface-track px-2 py-1.5 text-fs-1 text-fg-2">{t("about.privacy")}</p>
  </div></Dialog>;
}
