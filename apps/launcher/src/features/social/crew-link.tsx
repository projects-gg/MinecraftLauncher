// The crew's invite link and its join setting (TEAM5), under the crew code in the same box. Every
// member sees the link and copies its page's address (that page opens `projects://crew/…`);
// whoever answers requests (`admit`) creates it, renews it ("Eski bağlantı hemen çalışmaz olur.")
// or turns it off, each irreversible step asked once in place. The leader picks "Katılım": "Açık"
// (the code and the link let anyone in at once) or "Onaylı" (they ask, and requests wait); going
// back to "Açık" renews the link, which then swaps in place. A new Discord account hears in place
// why it cannot create a link yet.
import {
  ArrowsClockwise,
  Check,
  Copy,
  LinkSimple,
  LinkSimpleBreak,
  LockSimple,
} from "@phosphor-icons/react";
import { Button, Segmented } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { JoinMode, SocialTeam } from "@/lib/api/social-types";
import { attempt, copyText } from "./actions";
import { managesLink } from "./crew-joining";
import { limitOf, opensIn } from "./limits";
import { rise, swap, useSocialMotion } from "./motion";
import { useSocialUi } from "./store";

/** The link's page as players read it: the address without its scheme. */
const shown = (url: string) => url.replace(/^https?:\/\//, "");

type Asking = "renew" | "off" | null;

function LinkRow({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const features = useSocialUi((s) => s.view.crew);
  const me = useSocialUi((s) => s.view.me);
  const [copied, setCopied] = useState(false);
  const [asking, setAsking] = useState<Asking>(null);
  const [busy, setBusy] = useState(false);
  const manages = managesLink(team, features);
  const limit = limitOf(me, "crewLink", Date.now());
  const url = team.linkUrl ?? (team.link ? `https://social.projects.gg/e/${team.link}` : null);
  if (!url && !manages) return null;

  const change = async (on: boolean) => {
    setBusy(true);
    const link = await attempt(() => api.social.setTeamLink(team.id, on));
    setBusy(false);
    setAsking(null);
    if (link === undefined) return;
    announce(on ? t("crewWave2.link.renewed") : t("crewWave2.link.turnedOff"));
  };
  const copy = () =>
    void copyText(url ?? "").then((ok) => {
      if (!ok) return;
      setCopied(true);
      announce(t("crewWave2.link.copied"));
      window.setTimeout(() => setCopied(false), 1600);
    });

  return (
    <div className="mt-3 border-t border-white/7 pt-3">
      <div className="flex items-center gap-3">
        {url ? (
          <div className="min-w-0 flex-1">
            <p className="text-[12px] text-ash-400">{t("crewWave2.link.label")}</p>
            <p className="relative h-6 overflow-hidden text-[13.5px] leading-6 font-medium text-bone-100 tabular-nums">
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={url}
                  className="absolute inset-0 truncate"
                  data-selectable
                  {...swap(reduce, 0.26)}
                >
                  {shown(url)}
                </motion.span>
              </AnimatePresence>
            </p>
          </div>
        ) : (
          <p className="min-w-0 flex-1 text-[13.5px] text-bone-100">{t("crewWave2.link.label")}</p>
        )}
        {url ? (
          <Button size="sm" variant="secondary" onClick={copy}>
            <span className="relative inline-flex items-center">
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={String(copied)}
                  className="flex items-center gap-1.5"
                  {...swap(reduce, 0.16)}
                >
                  {copied ? (
                    <Check size={14} weight="bold" className="text-jade-400" />
                  ) : (
                    <Copy size={14} />
                  )}
                  {copied ? t("teams.code.copied") : t("teams.code.copy")}
                </motion.span>
              </AnimatePresence>
            </span>
          </Button>
        ) : (
          <Button
            size="sm"
            variant="secondary"
            icon={<LinkSimple size={14} />}
            disabled={busy || Boolean(limit)}
            onClick={() => void change(true)}
          >
            {t("crewWave2.link.create")}
          </Button>
        )}
      </div>
      {manages ? (
        <div className="mt-2">
          {limit ? (
            <p className="mb-1.5 flex items-start gap-2 text-[12.5px] leading-snug text-ash-300">
              <LockSimple size={14} className="mt-px shrink-0 text-ash-400" aria-hidden />
              {t("crewWave2.link.limited", { when: opensIn(t, limit.until, Date.now()) })}
            </p>
          ) : null}
          {url ? (
            <AnimatePresence mode="popLayout" initial={false}>
              {asking ? (
                <motion.div
                  key={asking}
                  className="flex items-center gap-2"
                  {...rise(reduce, 4, 0.2)}
                >
                  <p className="min-w-0 flex-1 text-[13px] text-bone-100">
                    {asking === "renew" ? t("crewWave2.link.renewAsk") : t("crewWave2.link.offAsk")}
                  </p>
                  <Button size="sm" variant="ghost" onClick={() => setAsking(null)}>
                    {t("teams.cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy}
                    onClick={() => void change(asking === "renew")}
                  >
                    {asking === "renew" ? t("crewWave2.link.renew") : t("crewWave2.link.off")}
                  </Button>
                </motion.div>
              ) : (
                <motion.div key="idle" className="flex gap-1" {...rise(reduce, 4, 0.2)}>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<ArrowsClockwise size={14} />}
                    disabled={Boolean(limit)}
                    onClick={() => setAsking("renew")}
                  >
                    {t("crewWave2.link.renew")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<LinkSimpleBreak size={14} />}
                    onClick={() => setAsking("off")}
                  >
                    {t("crewWave2.link.off")}
                  </Button>
                </motion.div>
              )}
            </AnimatePresence>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** "Katılım": the leader's choice between "Açık" and "Onaylı", what it means under it. */
function JoinSetting({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const lineId = useId();
  const [pending, setPending] = useState<JoinMode | null>(null);
  const current = team.join ?? "open";
  const mode = pending ?? current;
  const pick = async (join: JoinMode) => {
    if (join === current) return;
    setPending(join);
    const done = await attempt(() => api.social.setJoinMode(team.id, join));
    setPending(null);
    if (done !== undefined) announce(t("crewWave2.joinMode.saved"));
  };
  return (
    <div className="mt-3 border-t border-white/7 pt-3">
      <div className="flex items-center gap-3">
        <p className="min-w-0 flex-1 text-[13.5px] text-bone-100">
          {t("crewWave2.joinMode.label")}
        </p>
        <Segmented<JoinMode>
          value={mode}
          onValueChange={(join) => void pick(join)}
          label={t("crewWave2.joinMode.label")}
          describedBy={lineId}
          options={[
            { value: "open", label: t("crewWave2.joinMode.open") },
            { value: "approval", label: t("crewWave2.joinMode.approval") },
          ]}
        />
      </div>
      <p
        id={lineId}
        className="relative mt-1.5 min-h-[18px] text-[12.5px] leading-snug text-ash-400"
      >
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span key={mode} className="block" {...swap(reduce, 0.2)}>
            {mode === "approval"
              ? t("crewWave2.joinMode.approvalLine")
              : t("crewWave2.joinMode.openLine")}
          </motion.span>
        </AnimatePresence>
      </p>
    </div>
  );
}

/** The rows under the crew code: the invite link (with links) and "Katılım" (the leader). */
export function CrewLinkRows({ team }: { team: SocialTeam }) {
  const features = useSocialUi((s) => s.view.crew);
  return (
    <>
      {features.link ? <LinkRow team={team} /> : null}
      {features.requests && team.role === "founder" ? <JoinSetting team={team} /> : null}
    </>
  );
}
