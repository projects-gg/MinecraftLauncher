// Under an auto-reply, while the account may use placeholders (CAFK-FEAT §3.3): chips that put the
// sender's name (`{u}`) and the pattern's parts (`{1}`…) where the caret is, and "Dene", which runs
// the rule against a sample message on this computer and shows what the bot would send.
import { Flask } from "@phosphor-icons/react";
import { Button, cn, enter, Input, leave } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { InsertChip } from "../settings/controls";
import { type CloudReplyTry, cloudApi, type TryRule } from "./api";
import { placeholderChips, placeholderNames } from "./placeholders";

const fade = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: enter(0.16) },
  exit: { opacity: 0, transition: leave(0.12) },
};

export function ReplyTools({
  rule,
  sender,
  blocked,
  onInsert,
}: {
  rule: TryRule;
  /** Who writes the sample: the account's owner, else "Oyuncu". */
  sender: string;
  /** Why the save checks refuse this reply: "Dene" says it instead of a match (contract). */
  blocked: string | null;
  /** Puts a token (`{u}`, `{1}`) where the reply's caret is. */
  onInsert: (token: string) => void;
}) {
  const { t } = useTranslation();
  const sampleId = useId();
  const [trying, setTrying] = useState(false);
  const [sample, setSample] = useState("");
  const [result, setResult] = useState<CloudReplyTry | null>(null);
  const chips = placeholderChips(rule);
  const hasParts = placeholderNames(rule.reply).some((name) => name !== "u");

  // The answer follows the sample and the rule as they are typed (on this computer, no request).
  const { pattern, regex, from, reply } = rule;
  useEffect(() => {
    if (!trying || !sample.trim()) {
      setResult(null);
      return;
    }
    let live = true;
    const id = window.setTimeout(() => {
      void cloudApi
        .tryReply({ pattern, regex, from, reply }, sample, sender)
        .then((next) => live && setResult(next))
        .catch(() => undefined);
    }, 120);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [trying, sample, sender, pattern, regex, from, reply]);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-1.5">
        {/* Nothing to add to a server line's reply (it has no sender): no empty "Yanıta ekle". */}
        <fieldset
          aria-label={t("cloudAfkFeat.settings.chips.label")}
          className={cn("flex min-w-0 flex-wrap items-center gap-1.5", !chips.length && "hidden")}
        >
          <span aria-hidden className="mr-0.5 text-[12px] text-ash-400">
            {t("cloudAfkFeat.settings.chips.label")}
          </span>
          {chips.map((name) => (
            <InsertChip key={name} onClick={() => onInsert(`{${name}}`)}>
              {name === "u"
                ? t("cloudAfkFeat.settings.chips.sender")
                : t("cloudAfkFeat.settings.chips.part", { n: name })}
            </InsertChip>
          ))}
        </fieldset>
        <Button
          size="sm"
          variant="ghost"
          icon={<Flask size={14} />}
          aria-expanded={trying}
          onClick={() => setTrying((now) => !now)}
          className="ml-auto"
        >
          {t("cloudAfkFeat.settings.tryIt")}
        </Button>
      </div>
      <AnimatePresence initial={false}>
        {hasParts ? (
          <motion.p key="hint" className="text-[12px] text-ash-400" {...fade}>
            {t("cloudAfkFeat.settings.partsHint")}
          </motion.p>
        ) : null}
      </AnimatePresence>
      <AnimatePresence initial={false}>
        {trying ? (
          <motion.div key="try" className="flex flex-col gap-1.5" {...fade}>
            {/* Named over the field: once typed in, a bare field said nothing (SHEET-UX). */}
            <label htmlFor={sampleId} className="text-[12px] font-medium text-ash-300">
              {t("cloudAfkFeat.settings.trySample")}
            </label>
            <Input
              id={sampleId}
              value={sample}
              placeholder={t("cloudAfkFeat.settings.trySample")}
              maxLength={256}
              spellCheck={false}
              autoComplete="off"
              autoFocus
              onChange={(event) => setSample(event.target.value)}
              className="h-8 text-[13px]"
            />
            {/* One reserved line: the answer changes in place, nothing below moves. */}
            <p
              role="status"
              className={cn(
                "min-h-[18px] text-[12.5px] leading-[18px] break-words",
                blocked && result
                  ? "text-gold-200"
                  : result?.kind === "matches"
                    ? "text-bone-100"
                    : result?.kind === "value"
                      ? "text-gold-200"
                      : "text-ash-300",
              )}
            >
              {blocked && result
                ? blocked
                : result?.kind === "matches"
                  ? t("cloudAfkFeat.settings.tryMatches", { text: result.reply })
                  : result?.kind === "noMatch"
                    ? t("cloudAfkFeat.settings.tryNoMatch")
                    : result?.kind === "value"
                      ? t("cloudAfkFeat.settings.tryNoReply")
                      : result?.kind === "pattern"
                        ? t("cloudAfkFeat.settings.tryPattern")
                        : null}
            </p>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
