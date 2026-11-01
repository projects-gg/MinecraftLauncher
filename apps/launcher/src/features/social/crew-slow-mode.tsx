// "Yavaş mod" (CHAT-CALM): the crew chat's rule, in its own "Sohbet" box on the crew page for the
// leader and the moderators, who never wait themselves: off, or 5, 10 or 30 seconds between two
// messages of one member. The pick applies at once (back as it was if the service refuses) and
// the line under it says what it means.
import { Segmented } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialTeam } from "@/lib/api/social-types";
import { attempt } from "./actions";
import { SLOW_STEPS, type SlowStep, slowSpan, slowWords } from "./chat/slow";
import { swap, useSocialMotion } from "./motion";

export function CrewSlowMode({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const lineId = useId();
  const [pending, setPending] = useState<SlowStep | null>(null);
  const current = (SLOW_STEPS as readonly number[]).includes(team.slow ?? 0)
    ? ((team.slow ?? 0) as SlowStep)
    : 0;
  const slow = pending ?? current;
  const pick = async (next: SlowStep) => {
    if (next === current) return;
    setPending(next);
    const done = await attempt(() => api.social.setTeamSlow(team.id, next));
    setPending(null);
    if (done !== undefined) {
      announce(
        next > 0 ? t("chatCalm.slow.set", { time: slowWords(t, next) }) : t("chatCalm.slow.setOff"),
      );
    }
  };
  return (
    <div>
      <div className="flex items-center gap-3">
        <p className="min-w-0 flex-1 text-[13.5px] text-bone-100">{t("chatCalm.slow.title")}</p>
        <Segmented<`${SlowStep}`>
          value={`${slow}`}
          onValueChange={(value) => void pick(Number(value) as SlowStep)}
          label={t("chatCalm.slow.title")}
          describedBy={lineId}
          options={SLOW_STEPS.map((step) => ({
            value: `${step}` as const,
            label: slowWords(t, step),
          }))}
        />
      </div>
      <p
        id={lineId}
        className="relative mt-1.5 min-h-[18px] text-[12.5px] leading-snug text-ash-400"
      >
        {/* Each pick cross-fades the line (its words change with the step). */}
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span key={slow} className="block" {...swap(reduce, 0.2)}>
            {slow > 0
              ? t("chatCalm.slow.onLine", { time: slowSpan(t, slow) })
              : t("chatCalm.slow.offLine")}
          </motion.span>
        </AnimatePresence>
      </p>
    </div>
  );
}
