import { useQuery } from "@tanstack/react-query";
import { Outlet, useNavigate, useRouterState, useSearch } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import heroFallback from "@/assets/art/hero-dragon-1.webp";
import { NewsReader } from "@/features/news/news-reader";
import { api } from "@/lib/api";
import { Backdrop } from "./backdrop";
import { CommandBar } from "./command-bar";
import { NavRail } from "./nav-rail";
import { TitleBar } from "./title-bar";

export function AppShell() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useSearch({ strict: false }) as { news?: string };
  const navigate = useNavigate();
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const home = pathname === "/";

  return (
    <div className="flex h-dvh w-dvw overflow-hidden text-bone-100">
      <Backdrop
        imageUrl={data?.hero.imageUrl ?? heroFallback}
        variant={home ? "home" : "page"}
        particles={data?.hero.particles ?? "embers"}
      />
      <NavRail />
      <div className="flex min-w-0 flex-1 flex-col">
        <TitleBar />
        <AnimatePresence mode="wait" initial={false}>
          <motion.main
            key={pathname}
            className="flex min-h-0 flex-1 flex-col"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0, transition: { duration: 0.28, ease: [0.16, 1, 0.3, 1] } }}
            exit={{ opacity: 0, transition: { duration: 0.14 } }}
          >
            <Outlet />
          </motion.main>
        </AnimatePresence>
        <CommandBar />
      </div>
      <NewsReader
        newsId={search.news}
        onClose={() => void navigate({ to: ".", search: (prev: Record<string, unknown>) => ({ ...prev, news: undefined }) })}
      />
    </div>
  );
}
