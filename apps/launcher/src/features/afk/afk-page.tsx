import {
  DotsThreeVertical,
  ForkKnife,
  GearSix,
  Heart,
  Play,
  Stop,
  Terminal,
  Trash,
  UserPlus,
  UsersThree,
} from "@phosphor-icons/react";
import { Button, Chip, cn, EmptyState, Field, IconButton, Input, Menu, PlayerHead, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { type AfkAccount, type AfkState, api } from "@/lib/api";
import { formatDuration } from "@/lib/format";
import { AfkConsole } from "./afk-console";

const NAME = /^[A-Za-z0-9_]{3,16}$/;

function useNow(intervalMs = 1000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}

function stateTone(state: AfkState): "jade" | "gold" | "flare" | "ash" {
  switch (state.kind) {
    case "connected":
      return "jade";
    case "error":
      return "flare";
    case "stopped":
      return "ash";
    default:
      return "gold";
  }
}

function StateLine({ state }: { state: AfkState }) {
  const { t } = useTranslation();
  const now = useNow();
  let text: string;
  switch (state.kind) {
    case "connected":
      text = t("afk.states.connected", { uptime: formatDuration((now - new Date(state.since).getTime()) / 1000) });
      break;
    case "reconnecting":
      text = t("afk.states.reconnecting", {
        secs: Math.max(0, Math.round((new Date(state.nextAttemptAt).getTime() - now) / 1000)),
      });
      break;
    case "queued":
      text = t("afk.states.queued", { secs: state.inSecs });
      break;
    case "error":
      text = `${t("afk.states.error")}: ${state.reason}`;
      break;
    default:
      text = t(`afk.states.${state.kind}`);
  }
  const tone = stateTone(state);
  return (
    <p className="flex items-center gap-2 text-[12.5px]">
      <span
        className={cn(
          "size-2 shrink-0 rounded-full",
          tone === "jade" && "bg-jade-400 shadow-[0_0_8px_var(--color-jade-400)]",
          tone === "gold" && "animate-pulse bg-gold-400",
          tone === "flare" && "bg-flare-400",
          tone === "ash" && "bg-ash-500",
        )}
      />
      <span className={cn("truncate", tone === "flare" ? "text-flare-400" : "text-ash-300")}>{text}</span>
    </p>
  );
}

function AfkCard({ account, onConsole, index }: { account: AfkAccount; onConsole: () => void; index: number }) {
  const { t } = useTranslation();
  const running = account.state.kind !== "stopped" && account.state.kind !== "error";
  const connected = account.state.kind === "connected";

  return (
    <motion.li
      layout
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.35, delay: Math.min(index, 8) * 0.04, ease: [0.16, 1, 0.3, 1] }}
      className={cn(
        "glass relative flex flex-col rounded-[var(--radius-panel)] p-4",
        connected && "shadow-[inset_0_0_0_1px_rgb(95_203_142/0.22)]",
      )}
    >
      <div className="flex items-start gap-3.5">
        <PlayerHead name={account.nickname} src={`https://mc-heads.net/avatar/${account.nickname}/64`} size={48} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="truncate font-display text-[16px] font-semibold text-bone-50">{account.nickname}</h3>
            {account.autoStart ? <Chip>{t("afk.autoStart")}</Chip> : null}
          </div>
          <p className="truncate text-[12.5px] text-ash-400" data-selectable>
            {account.server}
          </p>
        </div>
        <Menu
          trigger={<IconButton label={t("afk.more")} icon={<DotsThreeVertical size={18} weight="bold" />} size="sm" />}
          items={[
            { label: t("afk.settings"), icon: <GearSix size={16} />, onSelect: () => toast(t("common.comingSoon")) },
            null,
            { label: t("afk.remove"), icon: <Trash size={16} />, danger: true, onSelect: () => void api.afkRemove(account.id) },
          ]}
        />
      </div>

      <div className="mt-3.5">
        <StateLine state={account.state} />
      </div>

      {connected ? (
        <div className="mt-3 flex gap-4 text-[12.5px] text-ash-300 tabular-nums">
          <span className="flex items-center gap-1.5" title={t("afk.stats.health")}>
            <Heart size={15} weight="fill" className="text-flare-400" />
            {account.health ?? "-"}
          </span>
          <span className="flex items-center gap-1.5" title={t("afk.stats.food")}>
            <ForkKnife size={15} weight="fill" className="text-gold-400" />
            {account.food ?? "-"}
          </span>
          <span className="flex items-center gap-1.5" title={t("afk.stats.players")}>
            <UsersThree size={15} weight="fill" className="text-ash-300" />
            {account.players ?? "-"}
          </span>
        </div>
      ) : null}

      <div className="mt-4 flex gap-2">
        {running ? (
          <Button size="sm" variant="secondary" icon={<Stop size={14} weight="fill" />} onClick={() => void api.afkStop(account.id)} className="flex-1">
            {t("afk.stop")}
          </Button>
        ) : (
          <Button size="sm" variant="primary" icon={<Play size={14} weight="fill" />} onClick={() => void api.afkConnect(account.id)} className="flex-1">
            {t("afk.connect")}
          </Button>
        )}
        <Button size="sm" variant="ghost" icon={<Terminal size={15} />} onClick={onConsole}>
          {t("afk.console")}
        </Button>
      </div>
    </motion.li>
  );
}

export function AfkPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: accounts = [], isPending } = useQuery({ queryKey: ["afk"], queryFn: () => api.afkAccounts() });
  const [consoleId, setConsoleId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | null>(null);

  useEffect(() => api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk"] })), [queryClient]);

  const anyRunning = accounts.some((a) => a.state.kind !== "stopped" && a.state.kind !== "error");
  const consoleAccount = accounts.find((a) => a.id === consoleId);

  const add = () => {
    const trimmed = name.trim();
    if (!NAME.test(trimmed)) return setNameError(t("afk.invalidName"));
    if (accounts.some((a) => a.nickname.toLowerCase() === trimmed.toLowerCase())) return setNameError(t("afk.duplicate"));
    void api.afkAdd(trimmed).then(() => setName(""));
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col px-10 pt-4">
      <header className="flex flex-wrap items-end justify-between gap-4 pb-6">
        <div>
          <h1 className="font-display text-[32px] leading-tight font-semibold text-bone-50">{t("afk.title")}</h1>
          <p className="mt-1 max-w-[60ch] text-ash-400">{t("afk.subtitle")}</p>
        </div>
        <div className="flex gap-2.5">
          {anyRunning ? (
            <Button variant="ghost" icon={<Stop size={15} weight="fill" />} onClick={() => void api.afkStopAll()}>
              {t("afk.stopAll")}
            </Button>
          ) : null}
          <Button variant="secondary" icon={<Play size={15} weight="fill" className="text-gold-300" />} onClick={() => void api.afkConnectAll()} disabled={!accounts.length}>
            {t("afk.connectAll")}
          </Button>
        </div>
      </header>

      <div className="-mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-6">
        {!isPending && accounts.length === 0 ? (
          <EmptyState title={t("afk.empty.title")} body={t("afk.empty.body")} />
        ) : (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4">
            {accounts.map((a, i) => (
              <AfkCard key={a.id} account={a} index={i} onConsole={() => setConsoleId(a.id)} />
            ))}
          </ul>
        )}

        <form
          className="mt-6 max-w-[520px]"
          onSubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <Field label={t("afk.add")} help={t("afk.nicknameHelp")} error={nameError} htmlFor="afk-name">
            <div className="flex gap-2">
              <Input
                id="afk-name"
                aria-label={t("afk.nickname")}
                value={name}
                maxLength={16}
                invalid={Boolean(nameError)}
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => {
                  setName(e.target.value);
                  setNameError(null);
                }}
              />
              <Button type="submit" variant="secondary" icon={<UserPlus size={16} />}>
                {t("afk.add")}
              </Button>
            </div>
          </Field>
        </form>
      </div>

      <AfkConsole account={consoleAccount} onClose={() => setConsoleId(null)} />
    </div>
  );
}
