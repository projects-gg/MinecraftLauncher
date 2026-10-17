// The card on top of the overview's play column: "Devam et" back into the last world or server,
// or, while there is none, the one empty state with "Oyna". The whole card is the button.
import { GameController, Globe, HardDrives, Play } from "@phosphor-icons/react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { ResumeTarget } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import type { LeadCard as LeadKind } from "./model";
import { FauxButton, IconTile, ServerPlayers } from "./parts";

const cardClass =
  "group glass relative flex w-full cursor-pointer items-center gap-4 rounded-[var(--radius-panel)] py-3 pr-3.5 pl-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-offset-2 focus-visible:ring-offset-obsidian-950 disabled:cursor-default";

const tileSize = "size-[52px] [--cut:7px]";

/** Lightens the glass under the pointer (a layer, so the glass keeps its own tint). */
function HoverWash() {
  return (
    <span
      aria-hidden
      className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/0 transition-colors duration-150 ease-out group-hover:bg-white/[0.035] group-disabled:bg-transparent"
    />
  );
}

/** `host` or `host:port` (`[v6]:port`), as a server list would write it. */
function serverAddress(host: string, port: number | null): string {
  if (port === null) return host;
  return host.includes(":") ? `[${host}]:${port}` : `${host}:${port}`;
}

export function LeadCard({
  kind,
  instanceId,
  target,
  locked,
}: {
  kind: LeadKind;
  instanceId: string;
  /** Where "Devam et" goes (`kind` "resume"). */
  target: ResumeTarget | null;
  /** This instance's game is starting or running. */
  locked: boolean;
}) {
  const { t } = useTranslation();
  const play = useLaunch((s) => s.play);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const titleId = useId();
  const resume = kind === "resume" ? target : null;

  const start = () => {
    // The command bar follows the instance the game starts from, as everywhere else.
    selectInstance(instanceId);
    void play(instanceId, resume ? { quickPlay: resume.quickPlay } : undefined);
  };

  if (resume) {
    const quick = resume.quickPlay;
    const server = quick.kind === "multiplayer" ? serverAddress(quick.host, quick.port) : null;
    const Mark = resume.kind === "world" ? Globe : HardDrives;
    return (
      <button
        type="button"
        className={cardClass}
        disabled={locked}
        onClick={start}
        aria-label={t("resume.aria", { name: resume.name })}
      >
        <HoverWash />
        <IconTile
          src={resume.iconUrl}
          pixelated={resume.kind === "server"}
          size={tileSize}
          fallback={<Mark size={24} weight="duotone" className="text-gold-400/85" />}
        >
          <span
            aria-hidden
            className="absolute inset-0 flex items-center justify-center bg-obsidian-950/60 opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 group-focus-visible:opacity-100 group-disabled:opacity-0"
          >
            <Play size={18} weight="fill" className="text-bone-50" />
          </span>
        </IconTile>
        <span className="relative min-w-0 flex-1">
          <span className="block text-[12px] text-ash-400">{t("overview.resumeLabel")}</span>
          <span className="block truncate font-display text-[17px] leading-snug font-semibold text-bone-50">
            {resume.name}
          </span>
          <span className="flex min-w-0 items-center gap-x-3 text-[12.5px] text-ash-300">
            {resume.lastPlayedAt ? (
              <span className="shrink-0">
                {t("play.lastPlayed", { when: formatRelative(resume.lastPlayedAt) })}
              </span>
            ) : null}
            {server ? <ServerPlayers address={server} /> : null}
          </span>
        </span>
        <FauxButton icon={<Play size={14} weight="fill" />}>{t("resume.label")}</FauxButton>
      </button>
    );
  }

  return (
    <button
      type="button"
      className={cardClass}
      disabled={locked}
      onClick={start}
      aria-label={t("instance.play")}
      aria-describedby={titleId}
    >
      <HoverWash />
      <IconTile
        size={tileSize}
        fallback={<GameController size={26} weight="duotone" className="text-gold-400/85" />}
      />
      <span
        id={titleId}
        className="relative min-w-0 flex-1 truncate font-display text-[17px] font-semibold text-bone-50"
      >
        {kind === "neverPlayed" ? t("play.neverPlayed") : t("overview.nothing")}
      </span>
      <FauxButton icon={<Play size={14} weight="fill" />}>{t("instance.play")}</FauxButton>
    </button>
  );
}
