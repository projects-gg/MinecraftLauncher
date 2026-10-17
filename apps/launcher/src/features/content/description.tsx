import { Skeleton } from "@pl/ui";
import { lazy, Suspense } from "react";

// The renderer (react-markdown, remark-gfm, rehype-raw with its HTML parser, rehype-sanitize)
// loads the first time a description shows, not with the launcher.
const DescriptionBody = lazy(() => import("./description-body"));

/** A Modrinth project description: Markdown with some HTML, rendered sanitized. */
export function ProjectDescription({ body }: { body: string }) {
  return (
    <Suspense
      fallback={
        <div className="space-y-2">
          <Skeleton className="h-4 w-11/12" />
          <Skeleton className="h-4 w-10/12" />
        </div>
      }
    >
      <DescriptionBody body={body} />
    </Suspense>
  );
}
