// SOC-NEXT §3.7 on screen: a run of an ignored person's messages folded into one quiet line in
// crew chat ("Yok saydığın birinin 3 mesajı" and "Göster"), the direct conversation's header line
// ("Yok sayıyorsun" and "Bırak"), and the friends settings' "Yok saydıkların" with "Bırak" per
// person (beside "Engellenenler").
import { EyeSlash } from "@phosphor-icons/react";
import { Button, cn } from "@pl/ui";
import { motion } from "motion/react";
import { memo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { TEXT_LINK } from "@/lib/text-link";
import { enter, glide, leave } from "../motion";
import { NameWithMark } from "../person-mark";
import { SocialAvatar } from "../social-face";
import { useSocialUi } from "../store";
import { useSocNext } from "./flag";
import { setIgnoring } from "./person-rows";
import { useSocNextPrefs } from "./prefs";

/** A folded run in the conversation's list (its day line over it when it starts a day). */
export const FoldItem = memo(function FoldItem({
  count,
  dayLine,
  divider,
  layout,
  onShow,
}: {
  count: number;
  dayLine: ReactNode;
  /** "Yeni mesajlar" over it (its words), as over a message. */
  divider: string | null;
  layout: boolean;
  onShow: () => void;
}) {
  const { t } = useTranslation();
  return (
    <motion.li
      layout={layout ? "position" : false}
      transition={{ layout: glide }}
      initial={false}
      animate={{ opacity: 1, transition: enter(0.2) }}
      exit={{ opacity: 0, transition: leave(0.16) }}
    >
      {dayLine}
      {divider ? (
        <p
          data-divider
          className="mx-2 my-2 flex items-center gap-2.5 text-[11.5px] font-semibold text-bone-100"
        >
          <span aria-hidden className="h-px flex-1 bg-ash-500/45" />
          {divider}
          <span aria-hidden className="h-px flex-1 bg-ash-500/45" />
        </p>
      ) : null}
      <p className="flex min-h-7 items-center gap-1.5 px-2.5 pt-1 pb-3 text-[12px] text-ash-400">
        <EyeSlash size={13} className="shrink-0" aria-hidden />
        <span>{t("socNext.ignore.fold", { count })}</span>
        <button type="button" onClick={onShow} className={cn(TEXT_LINK, "ml-1 text-[12px]")}>
          {t("socNext.ignore.show")}
        </button>
      </p>
    </motion.li>
  );
});

/** The direct conversation's header line while the player ignores the friend. */
export function IgnoredLine({ friend }: { friend: { id: string; name: string } }) {
  const { t } = useTranslation();
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <EyeSlash size={12} className="shrink-0 text-ash-400" aria-hidden />
      <span className="truncate">{t("socNext.ignore.banner")}</span>
      <button
        type="button"
        onClick={() => setIgnoring(friend, false, t)}
        className={cn(TEXT_LINK, "shrink-0 text-[12px]")}
      >
        {t("socNext.ignore.stopShort")}
      </button>
    </span>
  );
}

/** Whether the player ignores this person now (SOC-NEXT on). */
export function useIgnoring(id: string | undefined): boolean {
  const next = useSocNext();
  return useSocNextPrefs((s) => next && id !== undefined && s.ignored.includes(id));
}

/** "Yok saydıkların": friends and crewmates as the lists know them; someone no longer in any
 * list shows by their id's place as "Biri" (they can still be let go). */
export function IgnoredSection({
  Section,
}: {
  /** The settings page's own section (its title and spacing). */
  Section: (props: { title: string; children: ReactNode }) => ReactNode;
}) {
  const { t } = useTranslation();
  const next = useSocNext();
  const ignored = useSocNextPrefs((s) => s.ignored);
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  if (!next) return null;
  const known = (id: string) =>
    friends.find((f) => f.id === id) ??
    crews.flatMap((crew) => crew.members).find((member) => member.id === id) ?? {
      id,
      name: t("chat.list.someone"),
      avatar: "discord",
    };
  return (
    <Section title={t("socNext.ignore.settingsTitle")}>
      {ignored.length ? (
        <div className="divide-y divide-white/6">
          {ignored.map((id) => {
            const person = known(id);
            return (
              <div key={id} className="flex items-center gap-3 py-2">
                <SocialAvatar source={person} size={32} />
                <span className="min-w-0 flex-1">
                  <NameWithMark
                    name={person.name}
                    marks={"marks" in person ? person.marks : undefined}
                    className="text-[14px] text-bone-100"
                  />
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setIgnoring({ id, name: person.name }, false, t)}
                >
                  {t("socNext.ignore.stop")}
                </Button>
              </div>
            );
          })}
        </div>
      ) : (
        <p className="text-[13px] text-ash-400">{t("socNext.ignore.empty")}</p>
      )}
    </Section>
  );
}
