// What SKIN1 keeps mounted with the main window (accounts-host.tsx): the Projects skin panel of a
// username account (opened from its card, the palette or a toast) and the toasts of jobs that
// closed while the player looks elsewhere (rules.ts → `toastFor`). Every change also refreshes
// what the panel, the link row and the palette know.
import { toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAccountsUi } from "@/features/accounts/store";
import { useAccounts } from "@/features/accounts/use-accounts";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";
import { ProjectsSkinSheet } from "./projects-skin-sheet";
import { PROJECTS_SKIN_KEY, useProjectsSkinUi } from "./queries";
import { refusalNote, toastFor } from "./rules";
import { NoteText } from "./text";

/** Toasts with "Geri al" stay a little longer (as the key fixes' undo does). */
const UNDO_TOAST_MS = 8000;

function useChangeToasts() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  useEffect(() => {
    const undo = async (accountId: string) => {
      try {
        const sent = await projectsSkinApi.save(accountId, { kind: "undo" });
        if (sent.status === "refused") {
          toast.error(
            <NoteText t={t} note={refusalNote(sent.refusal, Math.floor(Date.now() / 1000))} />,
          );
        }
      } catch {
        toast.error(t("projectsSkin.note.saveFailed"));
      }
    };

    return projectsSkinApi.onChange((change) => {
      void queryClient.invalidateQueries({ queryKey: PROJECTS_SKIN_KEY });
      const plan = toastFor(change, {
        panelFor: useAccountsUi.getState().appearanceFor,
        codeBox: useProjectsSkinUi.getState().codeBoxes > 0,
      });
      if (!plan) return;
      switch (plan.kind) {
        case "saved": {
          const undoFor = plan.undoFor;
          toast.success(
            t(plan.backToDefault ? "projectsSkin.toast.savedDefault" : "projectsSkin.toast.saved"),
            {
              description: t(plan.rejoin ? "projectsSkin.toast.rejoin" : "projectsSkin.toast.join"),
              duration: undoFor ? UNDO_TOAST_MS : undefined,
              action: undoFor
                ? { label: t("projectsSkin.toast.undo"), onClick: () => void undo(undoFor) }
                : undefined,
            },
          );
          break;
        }
        case "skinFailed": {
          const accountId = plan.accountId;
          toast.error(<NoteText t={t} note={plan.note} />, {
            action: accountId
              ? {
                  label: t("projectsSkin.toast.open"),
                  onClick: () => useAccountsUi.getState().openAppearance(accountId),
                }
              : undefined,
          });
          break;
        }
        case "linked":
          toast.success(t("projectsSkin.toast.linked", { name: plan.name }));
          break;
        case "linkFailed":
          toast.error(<NoteText t={t} note={plan.note} />);
          break;
        case "unlinked":
          toast.success(t("projectsSkin.toast.unlinked"));
          break;
      }
    });
  }, [t, queryClient]);
}

export function ProjectsSkinHost() {
  const { data: accounts = [] } = useAccounts();
  const appearanceFor = useAccountsUi((s) => s.appearanceFor);
  const openAppearance = useAccountsUi((s) => s.openAppearance);
  useChangeToasts();
  const account = appearanceFor ? (accounts.find((a) => a.id === appearanceFor) ?? null) : null;
  return <ProjectsSkinSheet account={account} onClose={() => openAppearance(null)} />;
}
