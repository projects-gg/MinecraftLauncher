import { ArrowSquareOut, X } from "@phosphor-icons/react";
import { Button, Chip, IconButton } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { useTranslation } from "react-i18next";
import Markdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/format";

/**
 * Full news article over the current page. Markdown is sanitized; links open in the
 * system browser, never inside the launcher.
 */
export function NewsReader({
  newsId,
  onClose,
}: {
  newsId: string | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const item = data?.news.find((n) => n.id === newsId);
  const open = Boolean(newsId && item);

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open && item ? (
          <Dialog.Portal forceMount>
            <Dialog.Overlay asChild forceMount>
              <motion.div
                className="fixed inset-0 z-40 bg-obsidian-950/70 backdrop-blur-sm"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0, transition: { duration: 0.18 } }}
              />
            </Dialog.Overlay>
            <Dialog.Content asChild forceMount aria-describedby={undefined}>
              <motion.article
                className="glass-strong fixed top-1/2 left-1/2 z-50 flex max-h-[86vh] w-[min(820px,92vw)] flex-col overflow-hidden rounded-[14px] shadow-[0_40px_120px_rgb(34_6_6/0.6)] outline-none"
                initial={{ opacity: 0, y: 24, x: "-50%", scale: 0.98 }}
                animate={{ opacity: 1, y: "-50%", x: "-50%", scale: 1 }}
                exit={{ opacity: 0, y: "-46%", x: "-50%", transition: { duration: 0.2 } }}
                transition={{ duration: 0.42, ease: [0.16, 1, 0.3, 1] }}
                style={{ top: "50%" }}
              >
                {item.coverUrl ? (
                  <div className="relative h-[280px] shrink-0 overflow-hidden">
                    <img
                      src={item.coverUrl}
                      alt=""
                      className="size-full object-cover"
                      draggable={false}
                    />
                    <div className="absolute inset-0 bg-[linear-gradient(to_top,var(--color-obsidian-900),transparent_65%)]" />
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
                <div className="-mt-16 overflow-y-auto px-10 pb-10">
                  <div className="relative flex items-center gap-2.5">
                    <Chip tone="gold">{t(`categories.${item.category}`)}</Chip>
                    <span className="text-[12.5px] text-ash-300">
                      {formatRelative(item.publishedAt)}
                    </span>
                  </div>
                  <Dialog.Title className="relative mt-3 font-display text-[34px] leading-[1.05] font-semibold text-bone-50 [text-wrap:balance]">
                    {item.title}
                  </Dialog.Title>
                  <div className="prose-news mt-5">
                    <Markdown
                      remarkPlugins={[remarkGfm]}
                      rehypePlugins={[rehypeSanitize]}
                      components={{
                        a: ({ href, children }) => (
                          <a
                            href={href}
                            onClick={(e) => {
                              e.preventDefault();
                              if (href) void api.openUrl(href);
                            }}
                          >
                            {children}
                          </a>
                        ),
                      }}
                    >
                      {item.body}
                    </Markdown>
                  </div>
                  {item.cta ? (
                    <Button
                      className="mt-6"
                      variant="primary"
                      iconRight={<ArrowSquareOut size={16} weight="bold" />}
                      onClick={() => item.cta && void api.openUrl(item.cta.url)}
                    >
                      {item.cta.label}
                    </Button>
                  ) : null}
                </div>
              </motion.article>
            </Dialog.Content>
          </Dialog.Portal>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
