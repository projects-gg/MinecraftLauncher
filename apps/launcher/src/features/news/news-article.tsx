import { ArrowSquareOut, X } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  cn,
  dialogMotion,
  EntranceGate,
  GatedMotion,
  IconButton,
  scrimMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence } from "motion/react";
import { Dialog } from "radix-ui";
import { type ReactNode, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import Markdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { DiscordTile } from "@/features/social/discord-button";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { canOpenLink, isWebLink, openLink, openWebLink } from "@/lib/links";
import { minecraftText } from "@/lib/minecraft";
import { useNewsSeen } from "@/stores/news-seen";
import { CoverImage } from "./cover-image";
import { MinecraftChip } from "./minecraft";

/** The Discord message a feed post summarizes, opened in the browser. */
function DiscordSourceLink({ url }: { url: string }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => openWebLink(url)}
      className="group ml-auto inline-flex cursor-pointer items-center gap-2 rounded-md py-1 pr-1.5 pl-1 text-[12.5px] font-medium text-ash-300 transition-colors duration-150 hover:text-bone-50 focus-visible:outline-2"
    >
      <DiscordTile
        size={20}
        className="rounded-[5px] transition-colors duration-150 group-hover:bg-[#4752C4]"
      />
      {t("home.openInDiscord")}
    </button>
  );
}

/** Where a Minecraft update comes from: the official article, opened in the browser. */
function MinecraftSourceLink({ url }: { url: string }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => openWebLink(url)}
      aria-label={t("minecraft.readOn")}
      className="ml-auto inline-flex cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-1 text-[12.5px] font-medium text-ash-300 transition-colors duration-150 hover:text-bone-50 focus-visible:outline-2"
    >
      minecraft.net
      <ArrowSquareOut size={14} aria-hidden />
    </button>
  );
}

/** What the reader shows of a Projects news item or a Minecraft update. */
interface Article {
  title: string;
  body: string;
  coverUrl?: string;
  coverBlurhash?: string;
  /** The category chip, or Minecraft's source mark. */
  label: ReactNode;
  publishedAt: string;
  /** Written by the news feed: only https:// and projects:// links open, no remote images. */
  fromFeed: boolean;
  /** Top right: the original, where there is one. */
  source?: ReactNode;
  /** Under the text. */
  action?: ReactNode;
}

/**
 * The full news article over the current page (loaded with its Markdown renderer the first time
 * one opens, see news-reader.tsx). Markdown is sanitized; links open in the system browser, never
 * inside the launcher. It opens like every dialog (its entrance waits for the frame that draws
 * it, the cover picture included) and takes the keyboard focus on its text, so the arrow keys,
 * Page Down and Space scroll the article at once. An opened article is no longer new (its gold
 * dot in the lists fades out). Minecraft's updates open here too, with their source instead of
 * Discord and a button to the official article; they are never new, so nothing is marked.
 */
export default function NewsArticle({
  newsId,
  onClose,
}: {
  newsId: string | undefined;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { reduce } = useMotionPrefs();
  const scroller = useRef<HTMLDivElement>(null);
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const news = newsId ? data?.news.find((n) => n.id === newsId) : undefined;
  const minecraft =
    newsId && !news ? data?.minecraft.find((entry) => entry.id === newsId) : undefined;
  const open = Boolean(news || minecraft);
  const openedNews = news?.id;
  useEffect(() => {
    if (openedNews) useNewsSeen.getState().markRead(openedNews);
  }, [openedNews]);

  let article: Article | undefined;
  if (news) {
    const cta = news.cta;
    article = {
      title: news.title,
      body: news.body,
      coverUrl: news.coverUrl,
      label: <Chip tone="onArt">{t(`categories.${news.category}`)}</Chip>,
      publishedAt: news.publishedAt,
      fromFeed: Boolean(news.fromFeed),
      source: news.sourceUrl ? <DiscordSourceLink url={news.sourceUrl} /> : undefined,
      action: cta ? (
        <Button
          className="mt-6"
          variant="primary"
          iconRight={isWebLink(cta.url) ? <ArrowSquareOut size={16} weight="bold" /> : undefined}
          onClick={() => openLink(cta.url)}
        >
          {cta.label}
        </Button>
      ) : undefined,
    };
  } else if (minecraft) {
    const text = minecraftText(minecraft, i18n.language === "en" ? "en" : "tr");
    article = {
      title: text.title,
      body: text.body,
      coverUrl: minecraft.coverUrl,
      coverBlurhash: minecraft.coverBlurhash,
      label: <MinecraftChip />,
      publishedAt: minecraft.publishedAt,
      fromFeed: true,
      source: <MinecraftSourceLink url={minecraft.url} />,
      action: (
        <Button
          className="mt-6"
          variant="secondary"
          iconRight={<ArrowSquareOut size={16} weight="bold" />}
          onClick={() => openWebLink(minecraft.url)}
        >
          {t("minecraft.readOn")}
        </Button>
      ),
    };
  }
  const hasCover = Boolean(article?.coverUrl || article?.coverBlurhash);

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open && article ? (
          <EntranceGate key="news-reader">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-40 bg-obsidian-950/70 backdrop-blur-sm [html[data-perf=on]_&]:bg-obsidian-950/85 [html[data-perf=on]_&]:backdrop-blur-none"
                  {...scrimMotion(reduce)}
                />
              </Dialog.Overlay>
              <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center p-5">
                <Dialog.Content
                  asChild
                  forceMount
                  aria-describedby={undefined}
                  // The article's text takes the focus, not the close button (its tooltip would
                  // pop open): the keyboard scrolls the article right away.
                  onOpenAutoFocus={(event) => {
                    event.preventDefault();
                    scroller.current?.focus({ preventScroll: true });
                  }}
                >
                  <GatedMotion.div
                    className="pointer-events-auto flex max-h-[86vh] w-full max-w-[820px] flex-col outline-none"
                    {...dialogMotion(reduce)}
                  >
                    <article className="glass-strong relative flex min-h-0 flex-col overflow-hidden rounded-[14px] shadow-[0_40px_120px_rgb(34_6_6/0.6)]">
                      {hasCover ? (
                        <div className="relative h-[clamp(180px,32vh,260px)] shrink-0 overflow-hidden">
                          <CoverImage
                            src={article.coverUrl}
                            blurhash={article.coverBlurhash}
                            className="size-full"
                          />
                          {/* The date and title start over the picture: its lower third is nearly solid. */}
                          <div className="absolute inset-0 bg-[linear-gradient(to_top,var(--color-obsidian-900)_10%,rgb(18_16_22/0.86)_34%,transparent_78%)]" />
                        </div>
                      ) : null}
                      <div className="absolute top-3 right-3">
                        <Dialog.Close asChild>
                          <IconButton
                            label={t("common.close")}
                            icon={<X size={18} />}
                            className="bg-obsidian-950/60 backdrop-blur-md"
                          />
                        </Dialog.Close>
                      </div>
                      {/* With a cover the text starts over its lower part; without one, below
                          the close button. */}
                      <div
                        ref={scroller}
                        tabIndex={-1}
                        className={cn(
                          "overflow-y-auto px-8 pb-8 outline-none",
                          hasCover ? "-mt-14" : "pt-12",
                        )}
                      >
                        <div className="relative flex items-center gap-2.5">
                          {article.label}
                          <span className="text-shadow-art text-[12.5px] text-bone-100/85">
                            {formatRelative(article.publishedAt)}
                          </span>
                          {article.source}
                        </div>
                        <Dialog.Title className="text-shadow-art relative mt-2.5 font-display text-[30px] leading-[1.06] font-semibold text-bone-50 [text-wrap:balance]">
                          {article.title}
                        </Dialog.Title>
                        <div className="prose-news mt-4">
                          <Markdown
                            remarkPlugins={[remarkGfm]}
                            rehypePlugins={[rehypeSanitize]}
                            components={{
                              a: ({ href, children }) =>
                                // Only https:// pages and projects:// links open (the Discord
                                // feed's rule, docs/content-format.md); others stay text.
                                canOpenLink(href) ? (
                                  <a
                                    href={href}
                                    onClick={(e) => {
                                      e.preventDefault();
                                      openLink(href);
                                    }}
                                  >
                                    {children}
                                  </a>
                                ) : (
                                  <span>{children}</span>
                                ),
                              // Pictures in a feed post would load from anywhere: show their text.
                              ...(article.fromFeed
                                ? {
                                    img: ({ alt }: { alt?: string }) =>
                                      alt ? <span>{alt}</span> : null,
                                  }
                                : {}),
                            }}
                          >
                            {article.body}
                          </Markdown>
                        </div>
                        {article.action}
                      </div>
                    </article>
                  </GatedMotion.div>
                </Dialog.Content>
              </div>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
