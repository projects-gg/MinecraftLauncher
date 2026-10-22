// Links in a message and their cards (CHAT2 §9; the owner: every https link the rules let
// through, a one-line warning before an unknown site). A link is bone and underlined (gold is
// OYNA's), its full address in the tooltip; a trusted site opens at once, any other after one line
// that names its domain ("Bu bağlantı ornek.com sitesine gidiyor.", "Vazgeç" / "Aç"). The card
// sits in the bubble under the text: a 10 px container, the site's own launcher icon and name, the
// title and the description; a wide picture (a video, a stream, a projects.gg page) over the text,
// a squarer one (a mod's icon, a channel's face) beside it. Its place is held from the start at its
// likely height; the content fades in (180 ms), a card that does not come turns into the plain one
// (icon, site, address) at the same size. The computer never fetches anything from a linked site:
// the hub makes the card, pictures only from the trusted sites' own picture hosts.
import { Cube, Globe, Package, Play } from "@phosphor-icons/react";
import { type Brand, BrandMark, Button, easeOutExpo, Tooltip, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { Popover } from "radix-ui";
import { type ReactElement, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatCard, ChatCardIcon } from "@/lib/api/social-types";
import { openWebLink } from "@/lib/links";
import type { MessageLink } from "./text";

/** A link as the drawing needs it (text.ts's `messageLinks` makes them). */
type FoundLink = Pick<MessageLink, "start" | "end" | "url" | "host" | "ok" | "trusted">;

/** The rules whose refusal has words of its own (the contract's table; `formHint` is the
 * composer's "https:// ile yaz." for a bare domain). */
const WORDED = new Set([
  "off",
  "form",
  "formHint",
  "count",
  "http",
  "long",
  "shortener",
  "lookalike",
  "file",
  "invite",
]);

/** A refusal's words: its own, or "Bu bağlantı gönderilemez." (`chat2.links.rules.<key>`). */
export const ruleWords = (rule: string): string => (WORDED.has(rule) ? rule : "other");

/** The host or one of its parent domains is on the list. */
function hostIn(host: string, list: readonly string[]): boolean {
  return list.some((entry) => {
    const site = entry.toLowerCase();
    return host === site || host.endsWith(`.${site}`);
  });
}

/** The text in pieces: plain runs and links (for drawing). */
export type TextPiece =
  | { kind: "text"; text: string }
  | { kind: "link"; link: FoundLink; text: string };

export function piecesOf(text: string, links: readonly FoundLink[]): TextPiece[] {
  const pieces: TextPiece[] = [];
  let at = 0;
  for (const link of links) {
    if (link.start > at) pieces.push({ kind: "text", text: text.slice(at, link.start) });
    pieces.push({ kind: "link", link, text: text.slice(link.start, link.end) });
    at = link.end;
  }
  if (at < text.length) pieces.push({ kind: "text", text: text.slice(at) });
  return pieces;
}

/** Second-level labels under a country's domain that are nobody's name ("example.com.tr"). */
const GENERIC_SECOND = new Set(
  "com net org gov edu co ac gen info biz name web bel tv av dr pol k12 tel nom ltd plc me sch mil or ne go".split(
    " ",
  ),
);

/**
 * The registrable part of a host, as the warning names it: "www.youtube.com" → "youtube.com",
 * "forum.example.com.tr" → "example.com.tr" (whose site it is, not the subdomain in front).
 */
export function domainOf(host: string): string {
  const labels = host.replace(/^www\./, "").split(".");
  if (labels.length <= 2) return labels.join(".");
  const top = labels.at(-1) ?? "";
  const second = labels.at(-2) ?? "";
  const keep = top.length === 2 && (GENERIC_SECOND.has(second) || second.length <= 2) ? 3 : 2;
  return labels.slice(-keep).join(".");
}

/** One link's opening: at once for a trusted site, else after the line that names its domain. */
function LinkOpener({
  link,
  children,
}: {
  link: Pick<FoundLink, "url" | "host" | "trusted">;
  /** The element the warning opens from (the link, the card). */
  children: (open: () => void) => ReactElement;
}) {
  const { t } = useTranslation();
  const [asking, setAsking] = useState(false);
  const open = () => {
    if (link.trusted) openWebLink(link.url);
    else setAsking(true);
  };
  return (
    <Popover.Root open={asking} onOpenChange={setAsking}>
      <Popover.Anchor asChild>{children(open)}</Popover.Anchor>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="start"
          sideOffset={6}
          collisionPadding={12}
          className="pop-motion z-50 w-[288px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900 p-3 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] outline-none"
        >
          <p className="text-[13px] leading-snug text-bone-100">
            {t("chat2.links.warning", { domain: domainOf(link.host) })}
          </p>
          <div className="mt-2.5 flex justify-end gap-1.5">
            <Button size="sm" variant="ghost" autoFocus onClick={() => setAsking(false)}>
              {t("chat2.links.cancel")}
            </Button>
            <Button
              size="sm"
              variant="primary"
              onClick={() => {
                setAsking(false);
                openWebLink(link.url);
              }}
            >
              {t("chat2.links.open")}
            </Button>
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

/** A message's text with its links (only those the rules let through can be pressed). */
export function MessageText({ text, links }: { text: string; links: readonly FoundLink[] }) {
  if (!links.some((l) => l.ok)) return <>{text}</>;
  return (
    <>
      {piecesOf(text, links).map((piece, index) => {
        if (piece.kind === "text" || !piece.link.ok) {
          // biome-ignore lint/suspicious/noArrayIndexKey: pieces of one text, in place
          return <span key={index}>{piece.text}</span>;
        }
        const link = piece.link;
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: pieces of one text, in place
          <LinkOpener key={index} link={link}>
            {(open) => (
              // The warning's anchor is this span: the tooltip's trigger takes the link itself.
              <span>
                <Tooltip content={<span className="break-all">{link.url}</span>} side="top">
                  <a
                    href={link.url}
                    onClick={(event) => {
                      event.preventDefault();
                      open();
                    }}
                    className="cursor-pointer text-bone-50 underline decoration-bone-100/45 underline-offset-[3px] transition-[text-decoration-color,color] duration-150 hover:decoration-bone-50 focus-visible:rounded-[2px] focus-visible:outline-2 focus-visible:outline-gold-300"
                  >
                    {piece.text}
                  </a>
                </Tooltip>
              </span>
            )}
          </LinkOpener>
        );
      })}
    </>
  );
}

const BRANDS: Partial<Record<ChatCardIcon, Brand>> = {
  youtube: "youtube",
  twitch: "twitch",
  kick: "kick",
  instagram: "instagram",
  tiktok: "tiktok",
  x: "x",
};

/** The site's mark: the launcher's own monochrome icons, never the site's favicon. */
export function SiteIcon({ icon }: { icon: ChatCardIcon }) {
  const brand = BRANDS[icon];
  if (brand) return <BrandMark brand={brand} size={13} />;
  if (icon === "projects") return <Cube size={14} weight="fill" aria-hidden />;
  if (icon === "modrinth") return <Package size={14} weight="fill" aria-hidden />;
  return <Globe size={14} aria-hidden />;
}

/** The launcher's own icon for a site the card could not be asked for. */
function iconOf(host: string): ChatCardIcon {
  if (hostIn(host, ["youtube.com", "youtu.be"])) return "youtube";
  if (hostIn(host, ["twitch.tv"])) return "twitch";
  if (hostIn(host, ["kick.com"])) return "kick";
  if (hostIn(host, ["modrinth.com"])) return "modrinth";
  if (hostIn(host, ["projects.gg"])) return "projects";
  if (hostIn(host, ["discord.com", "discord.gg"])) return "discord";
  return "globe";
}

/** The plain card (icon, site, address) of a link whose card could not be asked for. */
export function plainCardOf(link: Pick<FoundLink, "url" | "host">): ChatCard {
  return {
    state: "plain",
    url: link.url,
    site: domainOf(link.host),
    icon: iconOf(link.host),
    kind: "page",
  };
}

/** How a card is drawn: a wide picture over its text, or one row (a squarer picture beside it). */
export type CardShape = "banner" | "row";

/** A picture at least this much wider than tall sits over the text. */
const WIDE = 1.3;

const pathOf = (url: string): string => {
  try {
    return new URL(url).pathname;
  } catch {
    return "";
  }
};

/**
 * The shape a link's card will most likely take, so that its place is held at that height before
 * it comes: a video, a clip and a projects.gg page carry a wide picture; anything else one row.
 */
export function cardShapeFor(url: string, host: string): CardShape {
  const path = pathOf(url);
  if (host === "youtu.be") return "banner";
  if (hostIn(host, ["youtube.com"])) {
    return /^\/(?:watch|shorts\/|live\/)/i.test(path) ? "banner" : "row";
  }
  if (host === "clips.twitch.tv") return "banner";
  if (hostIn(host, ["twitch.tv"]))
    return /^\/(?:videos\/|[^/]+\/clip\/)/i.test(path) ? "banner" : "row";
  if (host === "projects.gg" || host === "www.projects.gg") return "banner";
  return "row";
}

/** The shape a card that came takes: by its picture's own proportions. */
export function shapeOf(card: ChatCard): CardShape {
  if (card.state !== "ready" || !card.image) return "row";
  if (card.w && card.h) return card.w / card.h >= WIDE ? "banner" : "row";
  return card.kind === "video" || card.kind === "stream" ? "banner" : "row";
}

/** The address as a card shows it without a title: "ornek.com/adalar". */
function addressOf(link: Pick<FoundLink, "url" | "host">): string {
  const path = pathOf(link.url).replace(/\/$/, "");
  return `${link.host.replace(/^www\./, "")}${path}`;
}

function CardFrame({
  link,
  label,
  children,
}: {
  link: Pick<FoundLink, "url" | "host" | "trusted">;
  label: string;
  children: ReactNode;
}) {
  return (
    <LinkOpener link={link}>
      {(open) => (
        <button
          type="button"
          aria-label={label}
          onClick={open}
          className="group/card relative mt-1.5 mb-0.5 block w-full max-w-[280px] cursor-pointer overflow-hidden rounded-[var(--radius-panel)] border border-obsidian-700 bg-obsidian-800 text-left outline-none transition-[border-color,scale] duration-150 hover:border-white/14 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.99]"
        >
          {children}
        </button>
      )}
    </LinkOpener>
  );
}

/**
 * A link's card under the text. `card` absent: its place held (the card is on its way), or
 * nothing at all when it will not be asked for (`waiting` false: the message is a week old).
 */
export function LinkCard({
  link,
  card,
  waiting,
}: {
  link: FoundLink;
  card: ChatCard | undefined;
  /** Its card was asked for (or will be): hold its place. */
  waiting: boolean;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  if (card?.state === "refused") {
    return <p className="mt-1 text-[12px] text-ash-400">{t("chat2.links.refused")}</p>;
  }
  if (!card && !waiting) return null;
  const shape = card ? shapeOf(card) : cardShapeFor(link.url, link.host);
  const ready = card?.state === "ready" ? card : undefined;
  const site = card?.site ?? domainOf(link.host);
  const title = ready?.title;
  const label = title
    ? t("chat2.links.card", { site, title })
    : t("chat2.links.cardPlain", { site });
  const fade = {
    initial: reduce ? false : ({ opacity: 0 } as const),
    animate: { opacity: 1, transition: { duration: 0.18, ease: easeOutExpo } },
  };
  const siteLine = (
    <span className="flex min-w-0 items-center gap-1.5 text-[12px] leading-4 text-ash-400">
      <SiteIcon icon={card?.icon ?? "globe"} />
      <span className="shrink-0">{site}</span>
      {ready?.author ? (
        <>
          <span aria-hidden className="text-ash-500">
            ·
          </span>
          <span className="truncate">{ready.author}</span>
        </>
      ) : null}
    </span>
  );
  const picture = ready?.image;
  const playable = ready?.kind === "video" || ready?.kind === "stream";

  if (shape === "banner") {
    return (
      <CardFrame link={link} label={label}>
        <span className="relative block aspect-video w-full bg-white/[0.03]">
          {picture ? (
            <motion.img
              src={picture}
              alt=""
              draggable={false}
              className="absolute inset-0 size-full object-cover"
              {...fade}
            />
          ) : null}
          {picture && playable ? (
            <span className="absolute inset-0 flex items-center justify-center">
              <span className="flex size-10 items-center justify-center rounded-full bg-obsidian-950/70 text-bone-50 ring-1 ring-white/15 transition-transform duration-150 group-hover/card:scale-105">
                <Play size={16} weight="fill" aria-hidden />
              </span>
            </span>
          ) : null}
        </span>
        <span className="block h-[70px] px-2.5 py-2">
          {card ? (
            <motion.span className="block" {...fade}>
              {siteLine}
              <span className="mt-1 line-clamp-2 text-[13px] leading-[1.25] font-semibold text-bone-50">
                {title ?? addressOf(link)}
              </span>
            </motion.span>
          ) : (
            <CardSkeleton />
          )}
        </span>
      </CardFrame>
    );
  }
  return (
    <CardFrame link={link} label={label}>
      <span className="flex h-[72px] items-center gap-2.5 px-2.5 py-2">
        {card ? (
          <motion.span className="flex min-w-0 flex-1 items-center gap-2.5" {...fade}>
            <span className="min-w-0 flex-1">
              {siteLine}
              <span
                className="mt-1 block truncate text-[13px] leading-[1.3] font-semibold text-bone-50 data-[plain]:font-normal data-[plain]:text-bone-100"
                data-plain={title ? undefined : true}
              >
                {title ?? addressOf(link)}
              </span>
              {ready?.desc ? (
                <span className="block truncate text-[12.5px] leading-[1.3] text-ash-300">
                  {ready.desc}
                </span>
              ) : null}
            </span>
            {picture ? (
              <img
                src={picture}
                alt=""
                draggable={false}
                className="size-[52px] shrink-0 rounded-md object-cover"
              />
            ) : null}
          </motion.span>
        ) : (
          <CardSkeleton />
        )}
      </span>
    </CardFrame>
  );
}

function CardSkeleton() {
  return (
    <span aria-hidden className="flex size-full flex-col justify-center gap-1.5">
      <span className="h-2.5 w-20 rounded-full bg-white/[0.06]" />
      <span className="h-3 w-[85%] rounded-full bg-white/[0.08]" />
      <span className="h-2.5 w-[60%] rounded-full bg-white/[0.05]" />
    </span>
  );
}
