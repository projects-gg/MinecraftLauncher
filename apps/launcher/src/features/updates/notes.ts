// The Markdown renderer of release notes and pack changelogs (`notes-markdown.tsx`) loads on
// demand: callers start the load as soon as they know notes will show, so the dialog opens with
// its text in place.
export const loadNotes = () => import("./notes-markdown");
