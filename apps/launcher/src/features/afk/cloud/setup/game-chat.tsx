// The game's chat as another player will read it (SETUP-UX-L, the site's game-chat.tsx): the game's
// letters as whole pixels (the inventory's PixelLine, Projects Pixel's bitmaps, at GUI scale 2), each
// line wrapped where the game would wrap it for the strip's width, on the chat's half-dark strip. The
// sender in ash, the words in bone (never pure white), each with the game's quarter-tone drop shadow.
// A line whose words change (another tone, another example) rises 8 px in from below and fades in
// (220 ms), as a new chat line does, and takes the old one's place at once; under reduced motion and
// in Performance mode it only fades. CSS (`@starting-style`), so nothing runs once it is in. The
// strip is a picture for screen readers too: its words are given once as text.
import { cn, useMotionPrefs } from "@pl/ui";
import { PixelLine, textAdvance, wrapText } from "@pl/ui/inventory";
import {
  type CSSProperties,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

/** One message: who wrote it and what (`kind`: the game's public "<name> words" or "name: words"). */
export interface ChatMessage {
  /** Changes when the words do: the line comes in again. */
  key: string;
  who: string;
  text: string;
  kind?: "public" | "private";
}

const SCALE = 2;
/** The strip's padding in GUI pixels on each side. */
const PAD = 4;
const TONES = {
  who: { color: "#bdb6c3", shadow: "#2f2d31" },
  text: { color: "#f3ebdd", shadow: "#3d3b37" },
} as const;

/** The strip's inner width in GUI pixels, as it is drawn (a fallback before the first layout). */
function useGuiWidth(ref: RefObject<HTMLDivElement | null>, fallback: number): number {
  const [width, setWidth] = useState(fallback);
  useLayoutEffect(() => {
    const box = ref.current;
    if (!box || typeof ResizeObserver !== "function") return;
    const read = () =>
      setWidth(Math.max(40, Math.floor(box.getBoundingClientRect().width / SCALE) - PAD * 2));
    read();
    const watch = new ResizeObserver(read);
    watch.observe(box);
    return () => watch.disconnect();
  }, [ref]);
  return width;
}

function Message({ message, width }: { message: ChatMessage; width: number }) {
  const sender = message.kind === "private" ? `${message.who}:` : `<${message.who}>`;
  // Wrapped as one line of the game's chat; the sender is the first line's first word.
  const lines = wrapText(`${sender} ${message.text}`, width);
  return (
    <span className="flex flex-col gap-[2px]">
      {lines.map((line, index) =>
        index === 0 && line.startsWith(sender) ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: a message's wrapped lines, in order
          <span key={index} className="flex min-h-[20px] items-start">
            <PixelLine text={sender} {...TONES.who} />
            <span style={{ width: textAdvance(" ") * SCALE }} />
            <PixelLine text={line.slice(sender.length).trimStart()} {...TONES.text} />
          </span>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: a message's wrapped lines, in order
          <span key={index} className="flex min-h-[20px] items-start">
            <PixelLine text={line} {...TONES.text} />
          </span>
        ),
      )}
    </span>
  );
}

export function GameChat({
  messages,
  className,
  spoken = true,
}: {
  messages: ChatMessage[];
  className?: string;
  /** Off where the caller says the words itself (a live region of its own). */
  spoken?: boolean;
}) {
  const { layout } = useMotionPrefs();
  const box = useRef<HTMLDivElement>(null);
  const width = useGuiWidth(box, 150);
  // Lines on screen from the first frame stay still; only a line that comes later rises in.
  const [live, setLive] = useState(false);
  useEffect(() => {
    setLive(true);
  }, []);
  return (
    <div
      ref={box}
      className={cn(
        "flex flex-col gap-[2px] overflow-hidden rounded-[2px] bg-black/45 p-2",
        className,
      )}
      style={{ "--pl-inv-px": `${SCALE}px` } as CSSProperties}
    >
      {spoken ? (
        <p className="sr-only">{messages.map((m) => `${m.who}: ${m.text}`).join(" ")}</p>
      ) : null}
      {messages.map((message) => (
        <span
          key={message.key}
          aria-hidden
          className={cn(
            "block",
            live &&
              "transition-[opacity,translate] duration-[220ms] ease-[cubic-bezier(0.16,1,0.3,1)] starting:opacity-0",
            live && layout && "starting:translate-y-2",
          )}
        >
          <Message message={message} width={width} />
        </span>
      ))}
    </div>
  );
}
