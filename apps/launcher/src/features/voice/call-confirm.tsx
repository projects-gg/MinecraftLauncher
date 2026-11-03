// "Sesli arama başlat" asks first (VOICE-CALL; the owner: "tıklayınca emin misin diye sorsun"):
// "{name} aransın mı?" with "Vazgeç" and "Ara", the friend's face beside the question. The keyboard
// starts on "Vazgeç" (the in-game overlay's rule, OVL-SOC: a held Enter never rings anyone; Tab,
// then Enter calls); Esc or a press outside lets it be. Wherever a call starts (the friend's card,
// a person menu, the chat's header) it asks the same, unless the player turned "Aramadan önce sor"
// off in Settings → Sesli sohbet. "Tekrar ara" right after "Cevap yok" and "Odaya davet et" do not
// ask: the one repeats a call just asked about, the other starts none.
import { Phone } from "@phosphor-icons/react";
import { Button, Dialog } from "@pl/ui";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { SocialFace } from "@/features/social/social-face";
import { callFriend } from "./actions";
import { useFace, useVoiceUi } from "./store";

const useCallAsk = create<{ friend: string | null }>(() => ({ friend: null }));

/** Starts a call to `friend`, asking first while "Aramadan önce sor" is on (the default). */
export function askToCall(friend: string): void {
  if (useVoiceUi.getState().state.askBeforeCall === false) {
    void callFriend(friend);
    return;
  }
  // After the menu or the card that asked has let go of the focus.
  window.setTimeout(() => useCallAsk.setState({ friend }), 0);
}

/** "Ara" (`yes`) or "Vazgeç": the question goes either way. */
export function answerCallAsk(yes: boolean): void {
  const friend = useCallAsk.getState().friend;
  useCallAsk.setState({ friend: null });
  if (yes && friend) void callFriend(friend);
}

/** The question (mounted once, by VoiceHost). */
export function CallConfirm() {
  const { t } = useTranslation();
  const friend = useCallAsk((s) => s.friend);
  // The dialog keeps its words while it fades out.
  const last = useRef<string | null>(null);
  if (friend) last.current = friend;
  const shown = friend ?? last.current;
  const face = useFace(shown ?? "");

  if (!shown) return null;
  return (
    <Dialog
      open={friend !== null}
      onOpenChange={(open) => {
        if (!open) answerCallAsk(false);
      }}
      title={
        <span className="flex min-w-0 items-center gap-3">
          <SocialFace
            source={face}
            size={32}
            className="shrink-0 overflow-hidden rounded-[var(--radius-head)]"
          />
          <span className="min-w-0 [overflow-wrap:anywhere]">
            {t("voice.call.ask", { name: face.name })}
          </span>
        </span>
      }
      closeLabel={t("common.close")}
      width={400}
      footer={
        <>
          {/* The focus starts here: Enter alone never calls (the dialog keeps a focus it finds). */}
          <Button
            variant="ghost"
            autoFocus
            data-call-cancel=""
            onClick={() => answerCallAsk(false)}
          >
            {t("voice.call.askNo")}
          </Button>
          <Button
            variant="primary"
            icon={<Phone size={16} weight="fill" />}
            onClick={() => answerCallAsk(true)}
          >
            {t("voice.call.askYes")}
          </Button>
        </>
      }
    />
  );
}
