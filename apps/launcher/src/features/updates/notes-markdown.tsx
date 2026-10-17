import Markdown, { type Components } from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import { canOpenLink, openLink } from "@/lib/links";

/**
 * Links in release notes and pack changelogs open in the system browser (or inside the launcher
 * for projects://). A plain link would load the page in the launcher's own window, with no way
 * back but a restart.
 */
const noteComponents: Components = {
  a: ({ href, children }) =>
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
};

/**
 * Release notes and pack changelogs: sanitized Markdown. Loaded with its renderer only when notes
 * are about to show (`notes.ts`), not with the launcher.
 */
export default function NotesMarkdown({ text }: { text: string }) {
  return (
    <Markdown rehypePlugins={[rehypeSanitize]} components={noteComponents}>
      {text}
    </Markdown>
  );
}
