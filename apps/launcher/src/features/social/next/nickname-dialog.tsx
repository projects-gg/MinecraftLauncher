// "Takma ad ver" / "Takma adı değiştir" (SOC-NEXT §3.6): a small dialog, mounted once by
// SocNextHost, asked from a friend's menu. The field opens with the nickname (or empty, the real
// name as its hint); Enter saves, Esc or "Vazgeç" leaves it. One sentence says who sees it.
import { Button, Dialog, Field, Input, toast } from "@pl/ui";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { announce } from "@/features/accounts/use-accounts";
import { SocialFace } from "../social-face";
import { cleanNickname, graphemes, NICKNAME_MAX } from "./nickname";
import { useSocNextPrefs } from "./prefs";

interface Asked {
  id: string;
  /** The name the service knows. */
  realName: string;
  /** Their picture, beside the name the dialog is about. */
  avatar?: string;
  avatarUrl?: string;
}

const useNicknameAsk = create<{ asked: Asked | null }>()(() => ({ asked: null }));

/** Opens the dialog for a friend (after the menu that asked has let go of the focus). */
export function askNickname(asked: Asked) {
  window.setTimeout(() => useNicknameAsk.setState({ asked }), 0);
}

/** Takes a friend's nickname away, saying so. */
export function removeNickname(id: string, t: (key: string) => string) {
  useSocNextPrefs.getState().setNickname(id, null);
  announce(t("socNext.nick.removed"));
}

export function NicknameDialog() {
  const { t } = useTranslation();
  const asked = useNicknameAsk((s) => s.asked);
  // The dialog keeps its words while it fades out.
  const last = useRef<Asked | null>(null);
  if (asked) last.current = asked;
  const shown = asked ?? last.current;
  const current = useSocNextPrefs((s) => (shown ? s.nicknames[shown.id] : undefined));
  const [text, setText] = useState("");
  const fieldId = useId();
  // Only when a new ask opens: typing keeps what was typed.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the nickname is read once per ask
  useEffect(() => {
    if (asked) setText(current ?? "");
  }, [asked]);
  const close = () => useNicknameAsk.setState({ asked: null });
  if (!shown) return null;
  const tooLong = graphemes(text.trim()) > NICKNAME_MAX;
  const clean = cleanNickname(text);
  const unchanged = (clean ?? "") === (current ?? "");
  const save = () => {
    if (tooLong || unchanged) return;
    useSocNextPrefs.getState().setNickname(shown.id, clean);
    const words = clean ? t("socNext.nick.saved") : t("socNext.nick.removed");
    announce(words);
    toast.success(words);
    close();
  };
  return (
    <Dialog
      open={asked !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={current ? t("socNext.nick.change") : t("socNext.nick.set")}
      description={t("socNext.nick.hint")}
      closeLabel={t("common.close")}
      width={400}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("social.actions.cancel")}
          </Button>
          <Button variant="primary" disabled={tooLong || unchanged} onClick={save}>
            {t("socNext.nick.save")}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        {/* Whose nickname it is: their picture and the name the service knows. */}
        <p className="mb-3 flex min-w-0 items-center gap-2 text-[13px] text-bone-100">
          <SocialFace
            source={{
              name: shown.realName,
              avatar: shown.avatar ?? "discord",
              avatarUrl: shown.avatarUrl,
            }}
            size={20}
          />
          <span className="truncate">{shown.realName}</span>
        </p>
        <Field
          label={t("socNext.nick.title")}
          htmlFor={fieldId}
          error={tooLong ? t("socNext.nick.tooLong") : null}
        >
          <Input
            id={fieldId}
            autoFocus
            value={text}
            placeholder={shown.realName}
            maxLength={64}
            invalid={tooLong}
            onChange={(event) => setText(event.target.value)}
          />
        </Field>
      </form>
    </Dialog>
  );
}
