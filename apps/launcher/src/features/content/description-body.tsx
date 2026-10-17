import Markdown, { type Components } from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { openWebLink } from "@/lib/links";
import { allowedImage, descriptionSchema } from "./description-schema";

const components: Components = {
  a: ({ href, children }) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        if (href) openWebLink(href);
      }}
    >
      {children}
    </a>
  ),
  img: ({ src, alt, width, height }) =>
    allowedImage(src) ? (
      <img
        src={src}
        alt={alt ?? ""}
        width={width}
        height={height}
        loading="lazy"
        draggable={false}
        className="h-auto max-w-full"
      />
    ) : null,
};

/**
 * A Modrinth project description: Markdown with some HTML, rendered sanitized. Loaded by
 * `description.tsx` the first time a description shows.
 */
export default function DescriptionBody({ body }: { body: string }) {
  return (
    <div
      data-selectable
      className="prose-news max-w-none text-[14.5px] break-words [&_[align=center]]:text-center [&_details]:my-3 [&_h1]:mt-7 [&_h1]:mb-2 [&_h1]:font-display [&_h1]:text-[21px] [&_h1]:font-semibold [&_h1]:text-bone-50 [&_h4]:mt-4 [&_h4]:font-semibold [&_hr]:my-6 [&_hr]:border-white/10 [&_img]:my-1 [&_img]:inline-block [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:bg-obsidian-800 [&_pre]:p-3 [&_summary]:cursor-pointer [&_summary]:font-semibold [&_table]:my-4 [&_table]:w-full [&_table]:text-[13.5px] [&_td]:border-b [&_td]:border-white/6 [&_td]:px-2 [&_td]:py-1.5 [&_th]:border-b [&_th]:border-white/12 [&_th]:px-2 [&_th]:py-1.5 [&_th]:text-left"
    >
      <Markdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, descriptionSchema]]}
        components={components}
      >
        {body}
      </Markdown>
    </div>
  );
}
