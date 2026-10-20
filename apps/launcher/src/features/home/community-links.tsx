// The community row at the bottom of Home's news column (HOME1): wiki, Discord and forum on the
// left, the social accounts on the right, as quiet icon buttons. The addresses are Studio's
// (`links.custom` with well-known ids, `links.discord`), or the built-in ones while it publishes
// none (`pl_content::Links::community`). Every mark is drawn in the text colour: ash at rest,
// bone under the pointer; no brand colours (gold stays the one accent, docs/design.md).
import { BookOpen, Chats } from "@phosphor-icons/react";
import { BrandMark, IconButton, Skeleton } from "@pl/ui";
import { useTranslation } from "react-i18next";
import discordSymbol from "@/assets/brand/discord-symbol-white.png";
import type { CommunityLink, CommunityPlatform } from "@/lib/api";
import { openLink } from "@/lib/links";

const NAMES: Record<CommunityPlatform, string> = {
  wiki: "Wiki",
  discord: "Discord",
  forum: "Forum",
  instagram: "Instagram",
  tiktok: "TikTok",
  youtube: "YouTube",
  x: "X",
  twitch: "Twitch",
  kick: "Kick",
};

/** The platform's name: the same in every language (brands keep theirs; "Wiki", "Forum"). */
export function platformName(platform: CommunityPlatform): string {
  return NAMES[platform];
}

/** The community's own places; the rest are social accounts. */
const COMMUNITY: readonly CommunityPlatform[] = ["wiki", "discord", "forum"];

/**
 * Discord's official symbol (the launcher's PNG from Discord's brand kit) as a mask, so it takes
 * the text colour like the other marks.
 */
function DiscordGlyph({ height }: { height: number }) {
  const mask = `url(${discordSymbol}) center / contain no-repeat`;
  return (
    <span
      aria-hidden
      className="block shrink-0 bg-current"
      style={{
        width: Math.round(height * 1.32),
        height,
        mask,
        WebkitMask: mask,
      }}
    />
  );
}

/**
 * The platform's mark at the row's size. Wiki and forum use Phosphor's filled marks: the brand
 * marks are solid shapes, and one row reads as one family.
 */
export function CommunityMark({ platform }: { platform: CommunityPlatform }) {
  switch (platform) {
    case "wiki":
      return <BookOpen size={17} weight="fill" aria-hidden />;
    case "forum":
      return <Chats size={17} weight="fill" aria-hidden />;
    case "discord":
      return <DiscordGlyph height={12} />;
    default:
      return <BrandMark brand={platform} size={15} />;
  }
}

function LinkGroup({ links }: { links: CommunityLink[] }) {
  return (
    <ul className="flex items-center gap-0.5">
      {links.map((link) => (
        <li key={link.platform}>
          <IconButton
            size="sm"
            label={platformName(link.platform)}
            tooltipSide="top"
            icon={<CommunityMark platform={link.platform} />}
            onClick={() => openLink(link.url)}
          />
        </li>
      ))}
    </ul>
  );
}

/**
 * The row under the news column's last block. While the home content loads it keeps its height
 * with quiet marks in place, so nothing moves when the links come.
 */
export function CommunityRow({
  links,
  loading,
}: {
  links: CommunityLink[] | undefined;
  loading: boolean;
}) {
  const { t } = useTranslation();
  const community = (links ?? []).filter((link) => COMMUNITY.includes(link.platform));
  const social = (links ?? []).filter((link) => !COMMUNITY.includes(link.platform));
  return (
    <nav
      aria-label={t("homeLinks.label")}
      className="flex h-[42px] shrink-0 items-center justify-between gap-2 border-t border-white/7 px-2"
    >
      {loading ? (
        <>
          <span className="flex gap-2.5 px-1.5">
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} className="size-4" />
            ))}
          </span>
          <span className="flex gap-2.5 px-1.5">
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} className="size-4" />
            ))}
          </span>
        </>
      ) : (
        <>
          <LinkGroup links={community} />
          {social.length ? <LinkGroup links={social} /> : null}
        </>
      )}
    </nav>
  );
}
