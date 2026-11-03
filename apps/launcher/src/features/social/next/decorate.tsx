// How a message's words carry SOC-NEXT's marks (MessageText, link-ui.tsx): a mention ("@Kaan_07",
// semi-bold on a quiet plate) and a search's matches (a quiet plate on every match, the gold one on
// the result shown: the screen's one gold while the search is open). The search reaches the rows
// through a context, so only the words draw again as it changes, never the rows.
import { createContext, type ReactNode, useContext } from "react";
import { matchRanges } from "./chat-search";

/** The open search of a conversation: its folded query and the result shown (a message key). */
export interface SearchMarks {
  query: string | null;
  current: string | null;
}

export const SearchMarksContext = createContext<SearchMarks>({ query: null, current: null });

export function useSearchMarks(): SearchMarks {
  return useContext(SearchMarksContext);
}

/** Where a message's mentions are (UTF-16 offsets). */
export type Spans = ReadonlyArray<{ start: number; end: number }>;

const MENTION = "rounded-[3px] bg-white/[0.08] px-0.5 font-semibold text-bone-50";
const MATCH = "rounded-[3px] bg-bone-50/18 text-bone-50";
const CURRENT = "rounded-[3px] bg-gold-300 text-gold-ink";

/**
 * A plain piece of a message's text (from `offset` in the whole text) with its marks: the
 * boundaries of every mention and match split it, each part styled by what covers it.
 */
export function decorate(
  text: string,
  offset: number,
  mentions: Spans,
  search: ReadonlyArray<readonly [number, number]>,
  current: boolean,
): ReactNode {
  const end = offset + text.length;
  const cuts = new Set<number>([offset, end]);
  const inside = (start: number, stop: number) => stop > offset && start < end;
  for (const span of mentions) {
    if (!inside(span.start, span.end)) continue;
    cuts.add(Math.max(span.start, offset));
    cuts.add(Math.min(span.end, end));
  }
  for (const [start, stop] of search) {
    if (!inside(start, stop)) continue;
    cuts.add(Math.max(start, offset));
    cuts.add(Math.min(stop, end));
  }
  if (cuts.size === 2) return text;
  const points = [...cuts].sort((a, b) => a - b);
  const parts: ReactNode[] = [];
  for (let i = 0; i + 1 < points.length; i++) {
    const from = points[i] ?? offset;
    const to = points[i + 1] ?? end;
    const piece = text.slice(from - offset, to - offset);
    const mention = mentions.some((span) => span.start <= from && span.end >= to);
    const match = search.some(([start, stop]) => start <= from && stop >= to);
    let node: ReactNode = piece;
    if (match) {
      node = (
        <mark key={`m${from}`} className={current ? CURRENT : MATCH}>
          {piece}
        </mark>
      );
    }
    if (mention) {
      node = (
        <span key={`a${from}`} className={MENTION}>
          {node}
        </span>
      );
    }
    parts.push(typeof node === "string" ? <span key={`t${from}`}>{node}</span> : node);
  }
  return parts;
}

/** The search's matches in a message's words (none while no search is open). */
export function searchRangesOf(
  text: string,
  marks: SearchMarks,
): ReadonlyArray<readonly [number, number]> {
  return marks.query ? matchRanges(text, marks.query) : [];
}
