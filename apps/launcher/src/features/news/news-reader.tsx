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
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import Markdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { DiscordTile } from "@/features/social/discord-button";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { canOpenLink, isWebLink, openLink, openWebLink } from "@/lib/links";

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

/**
 * Full news article over the current page. Markdown is sanitized; links open in the
 * system browser, never inside the launcher. It opens like every dialog (its entrance waits for
 * the frame that draws it, the cover picture included) and takes the keyboard focus on its text,
 * so the arrow keys, Page Down and Space scroll the article at once.
 */
export function NewsReader({
  newsId,
  onClose,
}: {
  newsId: string | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const scroller = useRef<HTMLDivElement>(null);
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const item = data?.news.find((n) => n.id === newsId);
  const open = Boolean(newsId && item);

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open && item ? (
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
                      {item.coverUrl ? (
                        <div className="relative h-[clamp(180px,32vh,260px)] shrink-0 overflow-hidden">
                          <img
                            src={item.coverUrl}
                            alt=""
                            className="size-full object-cover"
                            draggable={false}
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
                          item.coverUrl ? "-mt-14" : "pt-12",
                        )}
                      >
                        <div className="relative flex items-center gap-2.5">
                          <Chip tone="onArt">{t(`categories.${item.category}`)}</Chip>
                          <span className="text-shadow-art text-[12.5px] text-bone-100/85">
                            {formatRelative(item.publishedAt)}
                          </span>
                          {item.sourceUrl ? <DiscordSourceLink url={item.sourceUrl} /> : null}
                        </div>
                        <Dialog.Title className="text-shadow-art relative mt-2.5 font-display text-[30px] leading-[1.06] font-semibold text-bone-50 [text-wrap:balance]">
                          {item.title}
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
                              ...(item.fromFeed
                                ? {
                                    img: ({ alt }: { alt?: string }) =>
                                      alt ? <span>{alt}</span> : null,
                                  }
                                : {}),
                            }}
                          >
                            {item.body}
                          </Markdown>
                        </div>
                        {item.cta ? (
                          <Button
                            className="mt-6"
                            variant="primary"
                            iconRight={
                              isWebLink(item.cta.url) ? (
                                <ArrowSquareOut size={16} weight="bold" />
                              ) : undefined
                            }
                            onClick={() => item.cta && openLink(item.cta.url)}
                          >
                            {item.cta.label}
                          </Button>
                        ) : null}
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
