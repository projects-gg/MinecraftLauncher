import { lazy, Suspense, useState } from "react";

// The article and its Markdown renderer (react-markdown, remark-gfm, rehype-sanitize) load the
// first time one opens: most starts never open one, and the start parses less script.
const NewsArticle = lazy(() => import("./news-article"));

/**
 * The full news article over the current page (`news-article.tsx`). Nothing renders before the
 * first article opens; after that the article stays mounted, so it closes with its own exit.
 */
export function NewsReader({
  newsId,
  onClose,
}: {
  newsId: string | undefined;
  onClose: () => void;
}) {
  const [wanted, setWanted] = useState(false);
  if (newsId && !wanted) setWanted(true);
  if (!wanted) return null;
  return (
    <Suspense fallback={null}>
      <NewsArticle newsId={newsId} onClose={onClose} />
    </Suspense>
  );
}
