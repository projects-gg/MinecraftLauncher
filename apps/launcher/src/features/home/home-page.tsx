import { ArrowSquareOut, BookOpenText, WifiSlash } from "@phosphor-icons/react";
import { Button, Chip, Skeleton } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { api, type HomeContent } from "@/lib/api";
import { NewsColumn } from "./news-column";

function HeroCopy({ content }: { content: HomeContent }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const featured = content.news.find((n) => n.id === content.hero.newsId);

  return (
    <motion.div
      initial={{ opacity: 0, y: 18 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.7, ease: [0.16, 1, 0.3, 1], delay: 0.1 }}
      className="max-w-[560px]"
    >
      {featured ? (
        <Chip tone="gold" className="mb-4">
          {t(`categories.${featured.category}`)}
        </Chip>
      ) : null}
      <h1 className="text-shadow-art font-display text-[60px] leading-[0.98] font-[640] tracking-[-0.01em] text-bone-50 [text-wrap:balance]">
        {content.hero.title}
      </h1>
      <p className="text-shadow-art mt-4 max-w-[46ch] text-[16px] leading-relaxed text-bone-100/90">
        {content.hero.subtitle}
      </p>
      {featured ? (
        <div className="mt-7 flex gap-3">
          <Button
            variant="secondary"
            size="lg"
            icon={<BookOpenText size={18} weight="duotone" />}
            onClick={() => void navigate({ to: "/", search: { news: featured.id } })}
          >
            {t("home.readNews")}
          </Button>
          {featured.cta ? (
            <Button
              variant="ghost"
              size="lg"
              iconRight={<ArrowSquareOut size={16} />}
              onClick={() => featured.cta && void api.openUrl(featured.cta.url)}
            >
              {featured.cta.label}
            </Button>
          ) : null}
        </div>
      ) : null}
    </motion.div>
  );
}

export function HomePage() {
  const { t } = useTranslation();
  const { data, isPending } = useQuery({
    queryKey: ["home-content"],
    queryFn: () => api.homeContent(),
  });

  return (
    <div className="flex min-h-0 flex-1 gap-6 px-10 pt-2 pb-6">
      <section className="flex min-w-0 flex-1 flex-col justify-end pb-4">
        {data?.offline ? (
          <div className="glass mb-auto flex w-fit items-center gap-2.5 rounded-full px-4 py-2 text-[13px] text-bone-100">
            <WifiSlash size={16} className="text-gold-300" />
            {t("home.offlineBanner")}
          </div>
        ) : null}
        {isPending || !data ? (
          <div className="max-w-[520px] space-y-4">
            <Skeleton className="h-6 w-24" />
            <Skeleton className="h-14 w-full" />
            <Skeleton className="h-5 w-4/5" />
          </div>
        ) : (
          <HeroCopy content={data} />
        )}
      </section>
      <NewsColumn content={data} loading={isPending} />
    </div>
  );
}
