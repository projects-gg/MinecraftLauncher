package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerMotion;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * One conversation, as the launcher shows it: bubbles (the player's own on the right, gold-tinted; others'
 * on the left, with names over a group in crew chat), the time and where a message stands under a group's
 * last bubble, "düzenlendi", a quiet mark where a message was deleted, and a failed message's reason with
 * "Tekrar dene" or "Düzenle" and "Sil". Laid out only when the conversation changes, so a frame only draws.
 *
 * <p>Chat extras (CHAT2): the message a reply answers as one quiet line over its bubble (pressing it goes
 * there), stickers without a bubble (each moves once when it arrives), reaction counts under a bubble (pressing
 * one gives or takes back the same reaction; the pointer on it names who), the links the launcher lets the
 * player press, a link's card as a line in the bubble, and "Görüldü" under the last message a friend has seen.
 * A message under the pointer, or picked with the arrow keys, shows "Tepki ver" and "Yanıtla" beside it;
 * "Tepki ver" opens six quick reactions and "Yanıtla" in their place.
 *
 * <p>The list sits at its newest message: a new one rises in and the older ones glide up to make room.
 * Scrolled up, it stays where it is and a "Yeni mesajlar" pill takes the player down. A short conversation
 * sits at the bottom, over the field (as in the launcher), and the list's edges fade into the panel while
 * there is more above or below. The newest message keeps clear of the line that floats over the bottom edge
 * ("yazıyor…", a note), so that line never covers a word.
 *
 * <p>Crew tags (TEAM6): in crew chat the name over someone's group carries the tag they wear ("[GK] Mert"); the
 * pointer resting on it opens the tag's bubble. Who came into the crew while the launcher ran shows as a quiet line
 * at its time among the messages ("Ece ekibe katıldı").
 */
public final class MessageList extends PanelWidget {
    private static final int BUBBLE_PAD_X = 6;
    private static final int BUBBLE_PAD_Y = 4;
    private static final int LINE_H = 10;
    private static final int SMALL_H = 9;
    private static final int GROUP_GAP = 7;
    private static final int ROW_GAP = 2;
    private static final int DIVIDER_H = 16;
    /** A sticker in the conversation, GUI pixels (CHAT2 §8.3). */
    public static final int STICKER = 48;
    private static final int QUOTE_H = 10;
    private static final int QUOTE_GAP = 2;
    private static final int CARD_GAP = 3;
    private static final int CHIP_H = 12;
    private static final int CHIP_GAP = 3;
    private static final int CHIPS_TOP = 3;
    private static final int TOOL = 14;
    private static final int TOOL_GAP = 2;
    private static final int TOOL_PAD = 2;
    /** A quick reaction's cell: the 16 px glyph with room on both sides (its focus ring keeps off the next one). */
    private static final int QUICK_CELL = 22;
    /** More room under a time in the middle of a group ("Görüldü"), so it reads as the bubble above's. */
    private static final int META_GAP = 3;
    /** A quiet line among the messages ("Ece ekibe katıldı", TEAM6 §7). */
    private static final int ARRIVAL_H = 14;
    /** The fades at the list's edges; a message picked with the keys is brought out of them. */
    private static final int FADE = ShapeSet.LIST_FADE;
    /** From the newest message's bottom to the end of the content (its gap and the list's own margin). */
    private static final float END_PAD = ROW_GAP + 4f;
    private static final long ENTER_MS = 200;
    private static final long STRIP_MS = 130;
    private static final long FLASH_MS = 1100;
    private static final long TOOLTIP_DELAY_MS = 280;
    private static final float WHEEL_STEP = 24f;
    /** Right after a conversation opens, its history arrives: the list lands on it instead of gliding. */
    private static final long SETTLE_MS = 700;
    /** The quick reactions, as sent (fully qualified) and as the game's font draws them. */
    static final String[] QUICK = {"👍", "❤️", "😂", "😭", "🔥", "😮"};
    private static final String[] QUICK_SHOWN = new String[QUICK.length];

    static {
        for (int i = 0; i < QUICK.length; i++) {
            QUICK_SHOWN[i] = Protocol.plain(QUICK[i], 8);
        }
    }

    /** What the player can do with a message. */
    public interface Actions {
        void retry(OverlayState.ChatLine line);

        void rewrite(OverlayState.ChatLine line);

        void discard(OverlayState.ChatLine line);

        /** "Yanıtla". */
        void reply(OverlayState.ChatLine line);

        /** Gives ({@code on}) or takes back a reaction ({@code emoji} as sent, {@code shown} as drawn). */
        void react(OverlayState.ChatLine line, String emoji, String shown, boolean on);

        /** A link the launcher lets the player press. */
        void openLink(String url, boolean trusted);

        /**
         * The sender's person menu (OVL-UX2; FIX2-D): a right click on their name over a group in crew chat, or Shift+F10
         * or the menu key on a message of theirs picked with the keys. {@code id}: their user id (crew chat), null for
         * the friend of a direct conversation; {@code badge}: the tag they wear, or null.
         */
        default void person(String id, String name, String badge, PersonMenu.Anchor at, boolean keys) {
        }
    }

    private static final class Row {
        /** The message, or null for a quiet line among them ("Ece ekibe katıldı"). */
        OverlayState.ChatLine line;
        boolean mine;
        FormattedCharSequence name;
        /** The name's width (a right click on it opens the sender's menu, OVL-UX2). */
        float nameW;
        /** TEAM6: the tag the sender wears, before their name over a group in crew chat. */
        TagChip chip;
        /** TEAM6 §7: someone came into the crew; the words of its quiet line. */
        FormattedCharSequence arrival;
        float arrivalW;
        Component arrivalWords;
        List<FormattedCharSequence> text;
        float y;
        float height;
        float bubbleTop;
        float bubbleW;
        float bubbleH;
        boolean meta;
        // Every word a frame draws keeps its layout (no text is laid out again while nothing changes).
        FormattedCharSequence time;
        float timeW;
        FormattedCharSequence edited;
        boolean seen;
        float metaTop;
        float metaWidth;
        OverlayState.Delivery delivery;
        FormattedCharSequence reason;
        float reasonW;
        boolean rewrite;
        FormattedCharSequence firstAction;
        FormattedCharSequence secondAction;
        float firstActionX;
        float firstActionW;
        float secondActionX;
        float secondActionW;
        float actionsY;
        boolean divider;
        long appearedAt;
        // CHAT2
        boolean quote;
        float quoteTop;
        FormattedCharSequence quoteName;
        float quoteNameW;
        FormattedCharSequence quoteText;
        float quoteW;
        boolean quoteQuiet;
        String quoteTarget;
        StickerSet.Sticker sticker;
        boolean unknownSticker;
        FormattedCharSequence cardSite;
        float cardTitleX;
        FormattedCharSequence cardTitle;
        float chipsTop;
        int chips;
        float[] chipX;
        float[] chipY;
        float[] chipW;
        FormattedCharSequence[] chipEmoji;
        FormattedCharSequence[] chipCount;
        float[] chipEmojiW;
        List<Texts.LinkSpan> links = List.of();
        boolean actionable;
    }

    private final Actions actions;
    private final List<Row> rows = new ArrayList<>();
    private final Spring scroll = Spring.snappy(0f);
    private final Set<String> known = new HashSet<>();
    private final float[] pose = new float[StickerMotion.SIZE];
    /** TEAM6: the bubble of the tag before a sender's name (the pointer only: messages take no focus one by one). */
    private final BadgeBubble bubble = new BadgeBubble();
    private String conversation;
    private boolean crew;
    private int laidOutRevision = -1;
    private int laidOutBadges = -1;
    private int laidOutWidth = -1;
    private int laidOutHeight = -1;
    private long laidOutSeen = -1;
    private long nextTimeout = Long.MAX_VALUE;
    private int unreadAtOpen;
    /** The first message that was unread when the conversation opened: "Yeni mesajlar" stays over it. */
    private OverlayState.ChatLine dividerLine;
    private float contentHeight;
    /** Where the rows end (the content's height without the room below them). */
    private float rowsEnd;
    /** Room kept under the newest message for the line floating over the list's bottom edge. */
    private float floatRoom;
    private boolean pinned = true;
    private long settleUntil;
    private boolean newBelow;
    /** The last layout added a message after the newest one the list had (only that brings the pill). */
    private boolean arrivedBelow;
    /** The floating line's height (0 when none can show) and how much of it shows this frame (0 to 1). */
    private int floatLineH;
    private float floatShown;
    /** Where the drawn messages end this frame (above the floating line while scrolled up). */
    private float visibleBottom = Float.MAX_VALUE;
    private boolean reducedMotion;
    private long lastFrame;
    private Component lastNarration = Component.empty();
    private String deletedText = "";
    private FormattedCharSequence editedWord = FormattedCharSequence.EMPTY;
    private float editedW;
    private FormattedCharSequence seenWord = FormattedCharSequence.EMPTY;
    private float seenW;
    private FormattedCharSequence dividerWord = FormattedCharSequence.EMPTY;
    private float dividerW;
    private FormattedCharSequence newBelowWord = FormattedCharSequence.EMPTY;
    private float newBelowWidth;
    private FormattedCharSequence emptyWord = FormattedCharSequence.EMPTY;
    private float emptyW;
    private final FormattedCharSequence[] quickLines = new FormattedCharSequence[QUICK.length];
    // What the player may do now (CHAT2): set on every refresh.
    private boolean canReply;
    private boolean canReact;
    private boolean turkish = true;
    // Pointer and keyboard (row indexes; -1 none).
    private int hoverRow = -1;
    private int hoverChip = -1;
    private int hoverTool = -1;
    private int hoverQuick = -1;
    /** The pointer rests on a sender's tag: its bubble is the one popover there (no toolbar beside the message). */
    private boolean hoverTag;
    private boolean hoverQuote;
    private boolean hoverLink;
    private int selected = -1;
    private int stripRow = -1;
    private int stripFocus;
    private final float[] quickW = new float[QUICK.length];
    private long stripOpenedAt;
    private OverlayState.ChatLine stripLine;
    private OverlayState.ChatLine flashLine;
    private long flashAt;
    private int tooltipRow = -1;
    private int tooltipChip = -1;
    private long tooltipSince;
    private long tooltipClosedAt;
    private List<FormattedCharSequence> tooltip = List.of();
    private float tooltipW;

    public MessageList(Actions actions) {
        super(0, 0, 100, 100, Texts.tr("chat.list"));
        this.actions = actions;
        for (int i = 0; i < QUICK.length; i++) {
            quickLines[i] = Texts.visual(QUICK_SHOWN[i]);
        }
    }

    private void setEmpty(String words, Font small) {
        emptyWord = Texts.visual(words);
        emptyW = small.width(words);
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
        bubble.setReducedMotion(value);
    }

    /** Closes the tag's bubble (another conversation, another page). */
    public void dismissBubble() {
        bubble.dismiss(now());
    }

    /** Esc: closes the tag's bubble if one is open; whether it was. */
    public boolean closeBubble() {
        return bubble.close(now());
    }

    /** The middle of the tag chip before {@code from}'s name (their newest group), or null (for the scenario driver). */
    public float[] chipCenter(String from) {
        for (int i = rows.size() - 1; i >= 0; i--) {
            Row row = rows.get(i);
            if (row.chip != null && row.line != null && row.line.from.equals(from)) {
                float y = getY() + row.y - scroll.value() + (row.divider ? DIVIDER_H : 0) + SMALL_H / 2f;
                if (y > getY() + FADE && y < Math.min(getY() + getHeight(), visibleBottom) - FADE) {
                    return new float[] {getX() + 1 + row.chip.width / 2f, y};
                }
            }
        }
        return null;
    }

    /** Draws the bubble of the tag under the pointer beside the panel (or inside the given box). */
    public void drawBubble(GuiGraphicsExtractor g, long now, float panelLeft, float left, float top, float right,
        float bottom) {
        if (visible) {
            bubble.draw(g, now, panelLeft, left, top, right, bottom);
        }
    }

    /** Whether {@code (mx, my)} is on the tag's bubble (a press there stays in the panel). */
    public boolean bubbleContains(double mx, double my) {
        return visible && bubble.contains(mx, my);
    }

    /**
     * A line {@code height} GUI pixels tall floats 2 above the list's bottom edge (the panel's "yazıyor…" and
     * notes; 0 when none can show): the newest message stays 3 above it, so nothing moves when it comes or goes
     * and it never covers a word. The launcher keeps the same room under its messages for its typing pill.
     */
    public void setFloatingLine(int height) {
        floatLineH = Math.max(0, height);
        float room = height <= 0 ? 0f : Math.max(0f, height + 2 + 3 - END_PAD);
        if (room == floatRoom) {
            return;
        }
        floatRoom = room;
        if (!rows.isEmpty()) {
            contentHeight = rowsEnd + 4f + floatRoom;
            if (pinned) {
                scroll.snap(maxScroll());
            }
        }
    }

    /**
     * How much the floating line shows this frame (the screen sets it before the list draws). Scrolled up, the
     * messages fade out above it instead of passing under it, so it covers no word there either.
     */
    public void setFloatingShown(float alpha) {
        floatShown = alpha;
    }

    /** The player went elsewhere (the field, a reply, the stickers): the pick and the quick reactions let go. */
    @Override
    public void setFocused(boolean focused) {
        super.setFocused(focused);
        if (!focused) {
            closeStrip();
            selected = -1;
        }
    }

    public String conversation() {
        return conversation;
    }

    /** The quick reactions are open (Esc closes them first). */
    public boolean stripOpen() {
        return stripRow >= 0;
    }

    /** A message is picked with the keyboard (Esc lets it go first). */
    public boolean hasSelection() {
        return selected >= 0;
    }

    /** Closes the quick reactions, else lets the picked message go. Returns whether there was either. */
    public boolean dismiss() {
        if (stripRow >= 0) {
            closeStrip();
            return true;
        }
        if (selected >= 0) {
            selected = -1;
            return true;
        }
        return false;
    }

    /** Shows another conversation (its newest messages). */
    public void open(String conv, boolean crewChat, int unread) {
        conversation = conv;
        crew = crewChat;
        unreadAtOpen = Math.max(0, unread);
        dividerLine = null;
        laidOutRevision = -1;
        known.clear();
        rows.clear();
        pinned = true;
        newBelow = false;
        selected = -1;
        closeStrip();
        flashLine = null;
        settleUntil = now() + SETTLE_MS;
        bubble.dismiss(now());
    }

    /** Lays out again when the conversation's lines, the width or a waiting message's time changed. */
    public void refresh(OverlayState state, long now) {
        OverlayState.Conversation conv = state.conversation(conversation);
        boolean writable = conv != null && conv.writable() && state.online() && state.has("chat");
        canReply = writable && state.has("replies");
        canReact = writable && state.has("reactions");
        if (conversation == null) {
            rows.clear();
            contentHeight = 0f;
            boolean tr = Texts.turkish();
            if (laidOutRevision != -2 || tr != turkish) {
                // Once, not on every frame: nothing is open, nothing changes.
                turkish = tr;
                setEmpty(Texts.str("chat.none"), Fonts.small());
                laidOutRevision = -2;
            }
            return;
        }
        int revision = state.lineRevision(conversation);
        long seen = conv == null ? 0 : conv.seen();
        if (getHeight() != laidOutHeight) {
            // The reply strip or the quick replies came or went: the list keeps its newest message in view.
            laidOutHeight = getHeight();
            if (pinned) {
                scroll.snap(maxScroll());
            }
        }
        if (revision == laidOutRevision && getWidth() == laidOutWidth && seen == laidOutSeen && now < nextTimeout
            && state.badgeRevision() == laidOutBadges) {
            return;
        }
        laidOutBadges = state.badgeRevision();
        boolean first = laidOutRevision == -1 || now < settleUntil;
        // The first message of an empty conversation appears in its place (it rises in by itself): gliding
        // up from the empty list's bottom would carry it across the whole list.
        boolean wasEmpty = rows.isEmpty();
        float oldTop = scroll.value();
        OverlayState.ChatLine keepSelected = selected >= 0 && selected < rows.size() ? rows.get(selected).line : null;
        layout(state, now, first, seen);
        laidOutRevision = revision;
        laidOutWidth = getWidth();
        laidOutSeen = seen;
        selected = keepSelected == null ? -1 : indexOf(keepSelected);
        if (stripLine != null) {
            stripRow = indexOf(stripLine);
            if (stripRow < 0 || !rows.get(stripRow).actionable) {
                closeStrip();
            }
        }
        float max = maxScroll();
        if (first || wasEmpty) {
            scroll.snap(pinned ? max : Math.min(max, oldTop));
        } else if (pinned) {
            if (reducedMotion) {
                scroll.snap(max);
            } else {
                // The older messages glide up as the new one comes in.
                scroll.snap(Math.min(oldTop, max));
                scroll.setTarget(max);
            }
        } else {
            if (reducedMotion) {
                scroll.snap(Math.min(max, oldTop));
            }
            // Scrolled up, the list stays where it is. A message that came in after the newest one brings the
            // pill (with reduced motion too); a bubble that only grew (a reaction, its card, "Görüldü") does not.
            if (arrivedBelow) {
                newBelow = true;
            }
        }
    }

    private int indexOf(OverlayState.ChatLine line) {
        for (int i = 0; i < rows.size(); i++) {
            if (rows.get(i).line == line) {
                return i;
            }
        }
        return -1;
    }

    /**
     * The scroll that shows the newest message at the bottom. Below zero for a conversation shorter than the
     * list: it then sits at the bottom, over the field, and does not scroll.
     */
    private float maxScroll() {
        return contentHeight - getHeight();
    }

    private float minScroll() {
        return Math.min(0f, maxScroll());
    }

    private float clampScroll(float target) {
        return Math.max(minScroll(), Math.min(maxScroll(), target));
    }

    private void layout(OverlayState state, long now, boolean first, long seen) {
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        turkish = Texts.turkish();
        for (int i = 0; i < QUICK.length; i++) {
            quickW[i] = ui.width(QUICK_SHOWN[i]);
            quickLines[i] = Texts.visual(QUICK_SHOWN[i]);
        }
        deletedText = Texts.str("chat.deleted");
        String edited = Texts.str("chat.edited");
        editedWord = Texts.visual(edited);
        editedW = small.width(edited);
        String seenWords = Texts.str("chat.seen");
        seenWord = Texts.visual(seenWords);
        seenW = small.width(seenWords);
        String divider = Texts.str("chat.divider");
        dividerWord = Texts.visual(divider);
        dividerW = small.width(divider);
        String below = Texts.str("chat.new_below");
        newBelowWord = Texts.visual(below);
        newBelowWidth = small.width(below) + 22f;
        setEmpty(Texts.str("chat.empty"), small);
        rows.clear();
        nextTimeout = Long.MAX_VALUE;
        int width = getWidth();
        int maxBubble = Math.max(60, Math.round(width * 0.78f));
        int textMax = maxBubble - 2 * BUBBLE_PAD_X;
        List<OverlayState.ChatLine> lines = new ArrayList<>(state.lines(conversation));
        OverlayState.ChatLine seenLine = crew ? null : OverlayState.seenLine(lines, seen);
        if (first) {
            // While the history comes in; then the divider stays over the same message (it slid down one row
            // with every message that came after, the player's own too).
            dividerLine = unreadAtOpen > 0 ? OverlayState.firstUnread(lines, unreadAtOpen) : null;
        }
        int dividerAt = dividerLine == null ? -1 : lines.indexOf(dividerLine);
        StickerSet set = StickerSet.get();
        // Who came into the crew, at their times among the messages (TEAM6 §7).
        List<OverlayState.Arrival> joins = crew ? state.arrivals(conversation) : List.of();
        int nextJoin = 0;
        float y = 4f;
        Row previous = null;
        Set<String> seenIds = new HashSet<>();
        int lastKnown = -1;
        int lastArrival = -1;
        for (int i = 0; i < lines.size(); i++) {
            OverlayState.ChatLine line = lines.get(i);
            OverlayState.ChatLine next = i + 1 < lines.size() ? lines.get(i + 1) : null;
            while (nextJoin < joins.size() && joins.get(nextJoin).at() <= line.at) {
                y = addJoin(joins.get(nextJoin++), y, previous != null, small, first, now, seenIds);
                previous = rows.get(rows.size() - 1);
            }
            // A join line that comes before the next message ends this group.
            boolean joinNext = next != null && nextJoin < joins.size() && joins.get(nextJoin).at() <= next.at;
            Row row = new Row();
            row.line = line;
            row.mine = line.mine;
            row.delivery = line.deliveryAt(now);
            if (row.delivery == OverlayState.Delivery.PENDING) {
                nextTimeout = Math.min(nextTimeout, line.createdAt + OverlayState.SEND_TIMEOUT_MS + 1);
            }
            boolean newGroup = previous == null || previous.line == null || previous.mine != line.mine
                || !previous.line.from.equals(line.from) || i == dividerAt;
            boolean lastOfGroup = next == null || joinNext || next.mine != line.mine || !next.from.equals(line.from)
                || i + 1 == dividerAt;
            if (i == dividerAt && i > 0) {
                row.divider = true;
                y += 2;
            }
            if (newGroup && previous != null) {
                y += GROUP_GAP - ROW_GAP;
            }
            row.y = y;
            if (row.divider) {
                y += DIVIDER_H;
            }
            if (crew && !line.mine && newGroup) {
                // "[GK] Mert": the tag they wear before the name (TEAM6); the name gives way, the chip does not.
                row.chip = TagChip.of(state.badge(line.badge), TagChip.CHAT);
                String fitted = Texts.fit(small, line.from, Math.round(maxBubble - TagChip.room(row.chip)));
                row.name = Texts.visual(fitted);
                row.nameW = small.width(fitted);
                y += SMALL_H + 1;
            }
            if (line.reply != null && !line.deleted) {
                layoutQuote(row, line.reply, set, small, maxBubble);
                row.quoteTop = y - row.y;
                y += QUOTE_H + QUOTE_GAP;
            }
            row.bubbleTop = y - row.y;
            if (line.deleted) {
                row.text = List.of(Texts.visual(deletedText));
                row.bubbleW = small.width(deletedText) + 2 * BUBBLE_PAD_X;
                row.bubbleH = SMALL_H + 2 * BUBBLE_PAD_Y;
            } else if (line.sticker != null) {
                row.sticker = set.find(line.sticker);
                row.unknownSticker = row.sticker == null;
                row.text = row.unknownSticker ? List.of(Texts.visual(Texts.str("chat.sticker"))) : List.of();
                row.bubbleW = STICKER;
                row.bubbleH = STICKER;
            } else {
                layoutText(row, line, ui, small, textMax);
            }
            y += row.bubbleH;
            if (!line.reactions.isEmpty() && !line.deleted) {
                y += CHIPS_TOP;
                row.chipsTop = y - row.y;
                y += layoutChips(row, line.reactions, ui, small, maxBubble);
            }
            boolean failed = row.delivery == OverlayState.Delivery.FAILED;
            row.seen = line == seenLine;
            row.meta = lastOfGroup || row.delivery != OverlayState.Delivery.SENT || (line.edited && !line.deleted)
                || row.seen;
            if (row.meta) {
                String time = time(line.at);
                row.time = Texts.visual(time);
                row.timeW = small.width(time);
                row.edited = line.edited && !line.deleted ? editedWord : null;
                row.metaWidth = row.timeW + (row.edited == null ? 0 : editedW + 4)
                    + (row.seen ? seenW + 4 : line.mine ? 11 : 0);
                y += 2;
                row.metaTop = y - row.y;
                y += SMALL_H;
            }
            if (failed) {
                String reason = line.failureAt(now);
                String why = failureText(reason, line.failureRule);
                row.reason = Texts.visual(why);
                row.reasonW = small.width(why);
                row.rewrite = isRewrite(reason) && line.sticker == null;
                String firstWords = Texts.str(row.rewrite ? "chat.rewrite" : "chat.retry");
                String secondWords = Texts.str("chat.discard");
                row.firstAction = Texts.visual(firstWords);
                row.secondAction = Texts.visual(secondWords);
                row.firstActionW = small.width(firstWords) + 8;
                row.secondActionW = small.width(secondWords) + 8;
                row.secondActionX = row.secondActionW;
                row.firstActionX = row.secondActionW + 2 + row.firstActionW;
                y += 2 + SMALL_H + 2;
                row.actionsY = y;
                y += 12;
            }
            row.actionable = !line.deleted && line.sid > 0 && row.delivery == OverlayState.Delivery.SENT;
            row.height = y - row.y;
            // The player's own message keeps its first key: it gets the service's id only when the launcher
            // confirms it, and with that id first it rose in (a sticker moved) a second time.
            String id = line.clientId != null ? line.clientId : line.id;
            if (id != null) {
                seenIds.add(id);
                if (known.contains(id)) {
                    lastKnown = i;
                } else if (!first) {
                    row.appearedAt = now;
                    lastArrival = i;
                }
            }
            rows.add(row);
            y += ROW_GAP + (row.meta && !lastOfGroup ? META_GAP : 0);
            previous = row;
        }
        while (nextJoin < joins.size()) {
            y = addJoin(joins.get(nextJoin++), y, previous != null, small, first, now, seenIds);
            previous = rows.get(rows.size() - 1);
        }
        known.clear();
        known.addAll(seenIds);
        arrivedBelow = lastArrival > lastKnown;
        rowsEnd = y;
        contentHeight = rows.isEmpty() ? 0f : y + 4f + floatRoom;
        lastNarration = rows.isEmpty() ? Texts.tr("chat.empty") : narrationOf(rows.get(rows.size() - 1));
        tooltipRow = -1;
    }

    /**
     * "Ece ekibe katıldı" (the player's own: "Ekibe katıldın") as a quiet line among the messages, the launcher's
     * waving hand before it; returns where the next row starts. A new one fades in where it lands.
     */
    private float addJoin(OverlayState.Arrival join, float y, boolean gapBefore, Font small, boolean first, long now,
        Set<String> seenIds) {
        float top = y + (gapBefore ? GROUP_GAP - ROW_GAP : 0);
        Row row = new Row();
        String words = join.mine() ? Texts.str("note.you")
            : Texts.str("note.joined", join.name().isEmpty() ? Texts.str("toast.someone") : join.name());
        String fitted = Texts.fit(small, words, Math.max(20, getWidth() - 24));
        row.arrival = Texts.visual(fitted);
        row.arrivalW = small.width(fitted);
        row.arrivalWords = Component.literal(words);
        row.y = top;
        row.height = ARRIVAL_H;
        String key = "join:" + join.id();
        seenIds.add(key);
        if (!known.contains(key) && !first) {
            row.appearedAt = now;
        }
        rows.add(row);
        return top + ARRIVAL_H + ROW_GAP;
    }

    /** The quoted message as one quiet line: "Ayşe: metin…", "Önceki bir mesaj", "Mesaj silindi". */
    private void layoutQuote(Row row, OverlayState.Reply reply, StickerSet set, Font small, int maxBubble) {
        row.quote = true;
        int room = maxBubble - 8;
        if (reply.missing() || reply.deleted()) {
            String words = Texts.str(reply.missing() ? "chat.quote.earlier" : "chat.deleted");
            row.quoteQuiet = true;
            row.quoteText = Texts.visual(Texts.fit(small, words, room));
            row.quoteW = 8 + Math.min(room, small.width(words));
            row.quoteTarget = reply.missing() ? null : reply.id();
            return;
        }
        String name = reply.mine() ? Texts.str("chat.quote.you") : reply.from().isEmpty() ? Texts.str("toast.someone")
            : reply.from();
        name = Texts.fit(small, name, Math.max(24, room / 2));
        String said;
        if (reply.sticker() != null) {
            StickerSet.Sticker s = set.find(reply.sticker());
            said = s == null ? Texts.str("chat.sticker") : Texts.str("chat.sticker.named", s.name(turkish));
        } else {
            said = reply.text() == null ? "" : reply.text();
        }
        row.quoteName = Texts.visual(name + ":");
        row.quoteNameW = small.width(name + ":") + 3;
        int textRoom = Math.max(10, room - Math.round(row.quoteNameW));
        String fitted = Texts.fit(small, said, textRoom);
        row.quoteText = Texts.visual(fitted);
        row.quoteW = 8 + row.quoteNameW + small.width(fitted);
        row.quoteTarget = reply.id();
    }

    /** A message's words, its links pressable, and its card line. */
    private void layoutText(Row row, OverlayState.ChatLine line, Font ui, Font small, int textMax) {
        if (line.links.isEmpty()) {
            row.text = Texts.wrap(ui, line.text, textMax, 12);
        } else {
            List<String> urls = new ArrayList<>(line.links.size());
            List<Boolean> trusted = new ArrayList<>(line.links.size());
            for (OverlayState.Link l : line.links) {
                urls.add(l.url());
                trusted.add(l.trusted());
            }
            Texts.Wrapped wrapped = Texts.wrapWithLinks(ui, line.text, urls, trusted, textMax, 12,
                row.mine ? Theme.BONE_50 : Theme.BONE_100);
            row.text = wrapped.lines();
            row.links = wrapped.links();
        }
        float widest = 0f;
        for (FormattedCharSequence s : row.text) {
            widest = Math.max(widest, ui.width(s));
        }
        row.bubbleW = Math.max(18f, widest + 2 * BUBBLE_PAD_X);
        row.bubbleH = row.text.size() * LINE_H + 2 * BUBBLE_PAD_Y;
        if (line.card != null) {
            String site = Texts.fit(small, line.card.site(), textMax / 2);
            row.cardSite = Texts.visual(site);
            row.cardTitleX = small.width(site);
            String title = Texts.fit(small, " · " + line.card.title(), Math.max(20, textMax - small.width(site)));
            row.cardTitle = Texts.visual(title);
            float cardW = small.width(site) + small.width(title);
            row.bubbleW = Math.max(row.bubbleW, Math.min(textMax, cardW) + 2 * BUBBLE_PAD_X);
            row.bubbleH += CARD_GAP + SMALL_H;
        }
    }

    /** The reaction chips, wrapped to the bubble's side; returns their height. */
    private float layoutChips(Row row, List<OverlayState.Reaction> reactions, Font ui, Font small, int maxBubble) {
        int n = reactions.size();
        row.chips = n;
        row.chipX = new float[n];
        row.chipY = new float[n];
        row.chipW = new float[n];
        row.chipEmoji = new FormattedCharSequence[n];
        row.chipCount = new FormattedCharSequence[n];
        row.chipEmojiW = new float[n];
        float x = 0f;
        float y = 0f;
        for (int i = 0; i < n; i++) {
            OverlayState.Reaction r = reactions.get(i);
            String count = r.count() > 99 ? "99+" : Integer.toString(r.count());
            row.chipEmoji[i] = Texts.visual(r.shown());
            row.chipEmojiW[i] = ui.width(r.shown());
            row.chipCount[i] = Texts.visual(count);
            float w = 4 + row.chipEmojiW[i] + 3 + small.width(count) + 5;
            if (x > 0 && x + w > maxBubble) {
                x = 0f;
                y += CHIP_H + 2;
            }
            row.chipX[i] = x;
            row.chipY[i] = y;
            row.chipW[i] = w;
            x += w + CHIP_GAP;
        }
        return y + CHIP_H;
    }

    private static String failureText(String reason, String rule) {
        if ("link".equals(reason) && rule != null) {
            return Texts.str("chat.failure.link." + switch (rule) {
                case "off", "count", "http", "long", "shortener", "lookalike", "file", "invite" -> rule;
                default -> "other";
            });
        }
        return Texts.str("chat.failure." + failureKey(reason));
    }

    private static String failureKey(String reason) {
        if (reason == null) {
            return "connection";
        }
        return switch (reason) {
            case "link", "phone", "words", "rate", "muted", "off", "stickers", "links" -> reason;
            case "daily-limit" -> "daily_limit";
            case "too-long" -> "too_long";
            case "limited-links" -> "limited_links";
            case "feature-off" -> "feature_off";
            case "blocked", "not-allowed" -> "not_allowed";
            default -> "connection";
        };
    }

    private static boolean isRewrite(String reason) {
        return reason != null && switch (reason) {
            case "link", "phone", "words", "too-long", "limited-links" -> true;
            default -> false;
        };
    }

    private static String time(long at) {
        LocalDateTime t = LocalDateTime.ofInstant(Instant.ofEpochMilli(at), ZoneId.systemDefault());
        int h = t.getHour();
        int m = t.getMinute();
        return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m;
    }

    private Component narrationOf(Row row) {
        OverlayState.ChatLine line = row.line;
        if (line == null) {
            return row.arrivalWords == null ? Component.empty() : row.arrivalWords;
        }
        if (line.deleted) {
            return Texts.tr("chat.deleted");
        }
        if (line.sticker != null) {
            String name = row.sticker == null ? Texts.str("chat.sticker") : row.sticker.name(turkish);
            return line.mine ? Texts.tr("narration.sticker.mine", name)
                : Texts.tr("narration.sticker.theirs", line.from, name);
        }
        return line.mine ? Texts.tr("narration.mine", line.text)
            : Texts.tr("narration.theirs", line.from, line.text);
    }

    /** Takes the list to its newest message. */
    public void toBottom() {
        pinned = true;
        newBelow = false;
        if (reducedMotion) {
            scroll.snap(maxScroll());
        } else {
            scroll.setTarget(maxScroll());
        }
    }

    private void scrollBy(float delta) {
        scrollTo(clampScroll(scroll.target() + delta));
    }

    private void scrollTo(float target) {
        pinned = target >= maxScroll() - 1f;
        if (pinned) {
            newBelow = false;
        }
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    /** Brings row {@code i} into view, clear of the edges' fades (keyboard picking: no glide, the keys repeat). */
    private void reveal(int i) {
        Row row = rows.get(i);
        float top = scroll.target();
        float margin = FADE + 2;
        // Clear of the floating line too, while it shows over the scrolled-up list.
        float h = Math.max(3 * margin, Math.min(getHeight(), visibleBottom - getY()));
        float target = top;
        if (row.y - margin < top) {
            target = row.y - margin;
        } else if (i == rows.size() - 1) {
            target = maxScroll();
        } else if (row.y + row.height + margin > top + h) {
            target = row.y + row.height + margin - h;
        }
        target = clampScroll(target);
        pinned = target >= maxScroll() - 1f;
        scroll.snap(target);
    }

    /** Goes to the message a quote answers and lights it for a moment. */
    private void jumpTo(String id) {
        for (int i = 0; i < rows.size(); i++) {
            Row row = rows.get(i);
            if (row.line != null && id.equals(row.line.id)) {
                float target = row.y + row.height / 2f - getHeight() / 2f;
                scrollTo(clampScroll(target));
                flashLine = row.line;
                flashAt = now();
                return;
            }
        }
    }

    // ---- Input ----------------------------------------------------------------------------------------

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY)) {
            return false;
        }
        closeStrip();
        scrollBy((float) -scrollY * WHEEL_STEP);
        return true;
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (stripRow >= 0) {
            int count = stripCount();
            if (event.isLeft()) {
                stripFocus = Math.max(0, stripFocus - 1);
            } else if (event.isRight()) {
                stripFocus = Math.min(count - 1, stripFocus + 1);
            } else if (event.isSelection() || event.isConfirmation()) {
                activateStrip(stripFocus);
            } else if (event.isUp() || event.isDown()) {
                closeStrip();
                return keyPressed(event);
            } else {
                return false;
            }
            PanelInput.usedKeyboard();
            return true;
        }
        if (selected >= 0 && selected < rows.size() && (event.key() == Compat.KEY_MENU
            || (event.key() == InputConstants.KEY_F10 && event.hasShiftDown()))) {
            // Shift+F10 or the menu key on someone else's message: its sender's menu (OVL-UX2), under the message.
            Row row = rows.get(selected);
            if (row.line != null && !row.mine) {
                PanelInput.usedKeyboard();
                float top = getY() + row.y - scroll.value() + row.bubbleTop;
                actions.person(row.line.by, row.line.from, row.line.badge, PersonMenu.Anchor.at(getX() + 1,
                    top + row.bubbleH + 1), true);
                return true;
            }
            return false;
        }
        switch (event.key()) {
            case InputConstants.KEY_UP -> select(selected < 0 ? rows.size() - 1 : selected - 1, -1);
            case InputConstants.KEY_DOWN -> {
                int below = selected < 0 ? -1 : messageFrom(selected + 1, 1);
                if (below >= 0) {
                    select(below, 1);
                } else if (selected >= 0) {
                    selected = -1;
                    toBottom();
                }
            }
            case InputConstants.KEY_PAGEUP -> scrollBy(-getHeight() * 0.8f);
            case InputConstants.KEY_PAGEDOWN -> scrollBy(getHeight() * 0.8f);
            case InputConstants.KEY_HOME -> scrollBy(-contentHeight);
            case InputConstants.KEY_END -> {
                selected = -1;
                toBottom();
            }
            case InputConstants.KEY_RETURN, InputConstants.KEY_SPACE, InputConstants.KEY_NUMPADENTER -> {
                if (selected < 0) {
                    return false;
                }
                Row row = rows.get(selected);
                if (row.quote && row.quoteTarget != null && !row.actionable) {
                    jumpTo(row.quoteTarget);
                } else if (row.actionable && (canReact || canReply)) {
                    openStrip(selected, canReact ? 0 : QUICK.length);
                } else {
                    return false;
                }
            }
            default -> {
                return false;
            }
        }
        PanelInput.usedKeyboard();
        return true;
    }

    /** The first message row from {@code index} on in direction {@code dir} (quiet lines are passed over), or -1. */
    private int messageFrom(int index, int dir) {
        for (int i = index; i >= 0 && i < rows.size(); i += dir) {
            if (rows.get(i).line != null) {
                return i;
            }
        }
        return -1;
    }

    private void select(int index, int dir) {
        if (rows.isEmpty()) {
            return;
        }
        int found = messageFrom(Math.max(0, Math.min(rows.size() - 1, index)), dir);
        if (found < 0) {
            found = messageFrom(Math.max(0, Math.min(rows.size() - 1, index)), -dir);
        }
        if (found < 0) {
            return;
        }
        selected = found;
        reveal(selected);
        lastNarration = narrationOf(rows.get(selected));
        net.minecraft.client.Minecraft.getInstance().getNarrator().saySystemQueued(lastNarration);
    }

    /** A right click opens a sender's menu (OVL-UX2). */
    @Override
    protected boolean isValidClickButton(MouseButtonInfo info) {
        return info.button() == InputConstants.MOUSE_BUTTON_LEFT || info.button() == InputConstants.MOUSE_BUTTON_RIGHT;
    }

    /**
     * A right click on a sender's name (or the tag before it) over a group in crew chat: their person menu at the
     * pointer, as the launcher's name does (OVL-UX2). Anything else a right click lands on does nothing.
     */
    private void rightClick(float mx, float my) {
        if (my >= visibleBottom) {
            return;
        }
        float cy = my - getY() + scroll.value();
        for (Row row : rows) {
            if (row.line == null || row.name == null || row.mine || cy < row.y || cy > row.y + row.height) {
                continue;
            }
            float nameTop = row.y + (row.divider ? DIVIDER_H : 0);
            float nameRight = getX() + 1 + TagChip.room(row.chip) + row.nameW;
            if (cy >= nameTop && cy < nameTop + SMALL_H && mx >= getX() && mx < nameRight + 2) {
                closeStrip();
                actions.person(row.line.by, row.line.from, row.line.badge, PersonMenu.Anchor.at(mx, my), false);
            }
            return;
        }
    }

    /** Where the sender's name over message {@code id} is on screen (the scenario driver points at it), or null. */
    public float[] senderNameAt(String id) {
        for (Row row : rows) {
            if (row.line != null && row.name != null && id.equals(row.line.id)) {
                float y = getY() + row.y - scroll.value() + (row.divider ? DIVIDER_H : 0) + SMALL_H / 2f;
                if (y < getY() || y > getY() + getHeight()) {
                    return null;
                }
                return new float[] {getX() + 1 + TagChip.room(row.chip) + row.nameW / 2f, y};
            }
        }
        return null;
    }

    /** A right click at {@code (mx, my)} as the game would send it (the scenario driver). */
    public void rightClickForDriver(float mx, float my) {
        rightClick(mx, my);
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        float mx = (float) event.x();
        float my = (float) event.y();
        if (event.button() == InputConstants.MOUSE_BUTTON_RIGHT) {
            rightClick(mx, my);
            return;
        }
        if (newBelow && pillContains(mx, my)) {
            toBottom();
            return;
        }
        boolean hadStrip = stripRow >= 0;
        if (hadStrip) {
            int cell = quickAt(mx, my);
            if (cell >= 0) {
                activateStrip(cell);
                return;
            }
            closeStrip();
        }
        if (my >= visibleBottom) {
            return;
        }
        float cy = my - getY() + scroll.value();
        float right = getX() + getWidth();
        for (int i = 0; i < rows.size(); i++) {
            Row row = rows.get(i);
            if (cy < row.y || cy > row.y + row.height) {
                continue;
            }
            float local = cy - row.y;
            if (row.firstAction != null && cy >= row.actionsY && cy <= row.actionsY + 12) {
                if (mx >= right - row.firstActionX && mx < right - row.firstActionX + row.firstActionW) {
                    if (row.rewrite) {
                        actions.rewrite(row.line);
                    } else {
                        actions.retry(row.line);
                    }
                    return;
                }
                if (mx >= right - row.secondActionX && mx < right - row.secondActionX + row.secondActionW) {
                    actions.discard(row.line);
                    return;
                }
            }
            // The toolbar was hidden under the open strip: this press only closed the strip.
            int tool = hadStrip ? -1 : toolAt(row, mx, local);
            if (tool == 0 && canReact) {
                openStrip(i, -1);
                return;
            }
            if (tool == 1 && canReply) {
                actions.reply(row.line);
                return;
            }
            if (row.quote && row.quoteTarget != null && quoteContains(row, mx, local)) {
                jumpTo(row.quoteTarget);
                return;
            }
            int chip = chipAt(row, mx, local);
            if (chip >= 0) {
                pressChip(row, chip);
                return;
            }
            Texts.LinkSpan link = linkAt(row, mx, local);
            if (link != null) {
                actions.openLink(link.url(), link.trusted());
                return;
            }
            return;
        }
    }

    private void pressChip(Row row, int chip) {
        if (!canReact || !reactable(row)) {
            return;
        }
        OverlayState.Reaction r = row.line.reactions.get(chip);
        actions.react(row.line, r.emoji(), r.shown(), !r.mine());
    }

    /** Whether the player may react on this row's message now (sent, here, younger than a week). */
    private boolean reactable(Row row) {
        return row.actionable && System.currentTimeMillis() - row.line.at < OverlayState.REACT_WINDOW_MS;
    }

    private void openStrip(int row, int focus) {
        if (!reactable(rows.get(row)) && !canReply) {
            return;
        }
        stripRow = row;
        stripLine = rows.get(row).line;
        stripFocus = Math.max(0, focus);
        stripOpenedAt = now();
    }

    private void closeStrip() {
        stripRow = -1;
        stripLine = null;
    }

    /** Cells of the strip: six reactions while the message takes them, then "Yanıtla" when replies are on. */
    private int stripCount() {
        Row row = stripRow >= 0 && stripRow < rows.size() ? rows.get(stripRow) : null;
        int n = row != null && canReact && reactable(row) ? QUICK.length : 0;
        // "Yanıtla" too, however it opened: the toolbar steps aside while the strip is open.
        return n + (canReply ? 1 : 0);
    }

    private void activateStrip(int cell) {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return;
        }
        Row row = rows.get(stripRow);
        boolean reactions = canReact && reactable(row);
        int n = reactions ? QUICK.length : 0;
        closeStrip();
        if (cell < n) {
            OverlayState.Reaction given = row.line.reaction(QUICK_SHOWN[cell]);
            actions.react(row.line, given != null ? given.emoji() : QUICK[cell], QUICK_SHOWN[cell],
                given == null || !given.mine());
        } else if (canReply && cell == n) {
            actions.reply(row.line);
        }
    }

    // ---- Geometry --------------------------------------------------------------------------------------

    private float bubbleX(Row row) {
        return row.mine ? getX() + getWidth() - row.bubbleW : getX();
    }

    /** The toolbar's buttons beside a bubble (react, reply); 0 when the row has none. */
    private int toolCount(Row row) {
        if (!row.actionable) {
            return 0;
        }
        return (canReact && reactable(row) ? 1 : 0) + (canReply ? 1 : 0);
    }

    private float toolbarW(int n) {
        return n * TOOL + (n - 1) * TOOL_GAP + 2 * TOOL_PAD;
    }

    private float toolbarX(Row row, int n) {
        float bx = bubbleX(row);
        return row.mine ? bx - 4 - toolbarW(n) : bx + row.bubbleW + 4;
    }

    /** Which toolbar button (0 react, 1 reply) is at {@code mx}, row-local {@code ly}; -1 none. */
    private int toolAt(Row row, float mx, float ly) {
        int n = toolCount(row);
        if (n == 0 || stripRow >= 0 || hoverTag || !(hoverRowIs(row) || rowIndex(row) == selected)) {
            return -1;
        }
        float tx = toolbarX(row, n) + TOOL_PAD;
        float ty = row.bubbleTop + TOOL_PAD - 1;
        if (ly < ty || ly >= ty + TOOL) {
            return -1;
        }
        boolean react = canReact && reactable(row);
        for (int k = 0; k < n; k++) {
            float bx = tx + k * (TOOL + TOOL_GAP);
            if (mx >= bx && mx < bx + TOOL) {
                return react ? k : 1;
            }
        }
        return -1;
    }

    private boolean hoverRowIs(Row row) {
        return hoverRow >= 0 && hoverRow < rows.size() && rows.get(hoverRow) == row;
    }

    private int rowIndex(Row row) {
        return rows.indexOf(row);
    }

    private boolean quoteContains(Row row, float mx, float ly) {
        float qx = row.mine ? getX() + getWidth() - row.quoteW : getX();
        return ly >= row.quoteTop && ly < row.quoteTop + QUOTE_H && mx >= qx && mx < qx + row.quoteW;
    }

    private int chipAt(Row row, float mx, float ly) {
        if (row.chips == 0) {
            return -1;
        }
        float right = getX() + getWidth();
        float rowW = 0f;
        for (int i = 0; i < row.chips; i++) {
            rowW = Math.max(rowW, row.chipX[i] + row.chipW[i]);
        }
        for (int i = 0; i < row.chips; i++) {
            float cx = row.mine ? right - rowW + row.chipX[i] : getX() + row.chipX[i];
            float cy = row.chipsTop + row.chipY[i];
            if (mx >= cx && mx < cx + row.chipW[i] && ly >= cy && ly < cy + CHIP_H) {
                return i;
            }
        }
        return -1;
    }

    private float chipsWidth(Row row) {
        float w = 0f;
        for (int i = 0; i < row.chips; i++) {
            w = Math.max(w, row.chipX[i] + row.chipW[i]);
        }
        return w;
    }

    private Texts.LinkSpan linkAt(Row row, float mx, float ly) {
        if (row.links.isEmpty()) {
            return null;
        }
        float bx = bubbleX(row) + BUBBLE_PAD_X;
        float ty = row.bubbleTop + BUBBLE_PAD_Y;
        for (Texts.LinkSpan link : row.links) {
            float top = ty + link.line() * LINE_H;
            if (ly >= top && ly < top + LINE_H && mx >= bx + link.x0() && mx < bx + link.x1()) {
                return link;
            }
        }
        return null;
    }

    /** The strip's box: above the bubble when there is room, else below it (list coordinates). */
    private float stripX(Row row, int n) {
        float w = stripW(n);
        float x = row.mine ? bubbleX(row) + row.bubbleW - w : bubbleX(row);
        return Math.max(getX(), Math.min(getX() + getWidth() - w, x));
    }

    private float stripW(int n) {
        return n * QUICK_CELL + 6;
    }

    private float stripY(Row row) {
        float top = getY() + row.y + row.bubbleTop - scroll.value();
        float above = top - QUICK_CELL - 6 - 3;
        return above >= getY() ? above : Math.min(getY() + getHeight() - QUICK_CELL - 6,
            top + row.bubbleH + 3);
    }

    private int quickAt(float mx, float my) {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return -1;
        }
        Row row = rows.get(stripRow);
        int n = stripCount();
        float x = stripX(row, n);
        float y = stripY(row);
        if (my < y + 3 || my >= y + 3 + QUICK_CELL) {
            return -1;
        }
        int cell = (int) Math.floor((mx - x - 3) / QUICK_CELL);
        return cell >= 0 && cell < n ? cell : -1;
    }

    private float pillLeft() {
        return getX() + getWidth() / 2f - pillWidth() / 2f;
    }

    private float pillWidth() {
        return newBelowWidth;
    }

    private boolean pillContains(float mx, float my) {
        float top = getY() + getHeight() - 18f;
        return mx >= pillLeft() && mx < pillLeft() + pillWidth() && my >= top && my < top + 14f;
    }

    /** Whether the "Yeni mesajlar" pill shows (the typing line moves above it). */
    public boolean pillShowing() {
        return newBelow;
    }

    // ---- Drawing ---------------------------------------------------------------------------------------

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        int h = getHeight();
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        if (rows.isEmpty()) {
            Draw.text(g, small, emptyWord, x0 + (w - emptyW) / 2f, Fonts.centeredY(Fonts.Face.SMALL, y0 + h / 2f),
                Theme.ASH_400);
            bubble.track(null, false, now);
            return;
        }
        float top = scroll.value();
        // Scrolled up while the floating line shows ("yazıyor…", a note): the messages end above it, with the
        // edge's fade, instead of passing under it. The edge rises with the line's own fade, so nothing pops; at
        // the newest message its room is empty, so nothing changes there.
        float band = (floatLineH > 0 ? floatShown : 0f) * Math.min(1f, Math.max(0f, (maxScroll() - top) / FADE));
        float bottom = y0 + h - (band > 0.01f ? band * (floatLineH + 4 + (newBelow ? 18 : 0)) : 0f);
        visibleBottom = bottom;
        trackPointer(mouseX, mouseY, top, now);
        g.enableScissor(x0 - 3, y0, x0 + w + 3, Math.round(bottom));
        float right = x0 + w;
        for (int i = 0; i < rows.size(); i++) {
            Row row = rows.get(i);
            float ry = y0 + row.y - top;
            if (ry > y0 + h || ry + row.height < y0) {
                continue;
            }
            float enter = row.appearedAt == 0 ? 1f : Math.min(1f, (now - row.appearedAt) / (float) ENTER_MS);
            float eased = Ease.OUT.at(enter);
            // A quiet line only fades in where it lands (as the launcher's).
            float rise = reducedMotion || row.sticker != null || row.line == null ? 0f : (1f - eased) * 4f;
            Draw.pushAlpha(eased);
            drawRow(g, i, row, ry + rise, x0, w, right, mouseX, mouseY, now, ui, small);
            Draw.popAlpha();
        }
        // The edges fade into the panel while there is more beyond them (the strength follows the scroll, so
        // nothing pops): no hard cut through a bubble right under the conversation chips.
        float above = Math.min(1f, Math.max(0f, top / FADE));
        float below = Math.max(band, Math.min(1f, Math.max(0f, (maxScroll() - top - floatRoom - 2f) / FADE)));
        if (above > 0.01f) {
            Draw.pushAlpha(above);
            Draw.column(g, ShapeSet.FADE_TOP, x0 - 3, y0, w + 6, FADE);
            Draw.popAlpha();
        }
        if (below > 0.01f) {
            Draw.pushAlpha(below);
            Draw.column(g, ShapeSet.FADE_BOTTOM, x0 - 3, bottom - FADE, w + 6, FADE);
            Draw.popAlpha();
        }
        g.disableScissor();
        Draw.stratum(g);
        drawToolbar(g, now);
        drawStrip(g, now, small);
        drawTooltip(g, now, small);
        if (newBelow) {
            float pw = pillWidth();
            float px = pillLeft();
            float py = y0 + h - 18f;
            Draw.frame(g, Frames.CHIP_SELECTED, px, py, pw, 14);
            Draw.icon(g, Icon.DOWN, 8, px + 6, py + 3, Theme.GOLD_300);
            Draw.text(g, small, newBelowWord, px + 16, Fonts.centeredY(Fonts.Face.SMALL, py + 7), Theme.BONE_100);
        }
    }

    /**
     * Works out what is under the pointer (no allocation; the tooltip's words only when its chip changes), and tells
     * the tag's bubble whether the pointer rests on a sender's tag.
     */
    private void trackPointer(int mouseX, int mouseY, float top, long now) {
        hoverRow = -1;
        hoverChip = -1;
        hoverTool = -1;
        hoverQuick = -1;
        hoverQuote = false;
        hoverLink = false;
        BadgeBubble.Target tag = null;
        hoverTag = false;
        boolean onBubble = bubble.contains(mouseX, mouseY);
        if (!isHovered()) {
            updateTooltip(-1, -1);
            bubble.track(null, onBubble, now);
            return;
        }
        if (stripRow >= 0) {
            hoverQuick = quickAt(mouseX, mouseY);
        }
        if (mouseY >= visibleBottom) {
            // Under the floating line: nothing of the list shows there.
            updateTooltip(-1, -1);
            bubble.track(null, onBubble, now);
            return;
        }
        float cy = mouseY - getY() + top;
        for (int i = 0; i < rows.size(); i++) {
            Row row = rows.get(i);
            if (cy < row.y - 1 || cy > row.y + row.height + ROW_GAP) {
                continue;
            }
            hoverRow = i;
            float local = cy - row.y;
            hoverTool = toolAt(row, mouseX, local);
            hoverChip = chipAt(row, mouseX, local);
            hoverQuote = row.quote && row.quoteTarget != null && quoteContains(row, mouseX, local);
            hoverLink = linkAt(row, mouseX, local) != null;
            if (row.chip != null && stripRow < 0) {
                float chipY = row.chip.top(getY() + row.y - top + (row.divider ? DIVIDER_H : 0) + SMALL_H / 2f);
                float chipX = getX() + 1;
                if (mouseX >= chipX && mouseX < chipX + row.chip.width && mouseY >= chipY
                    && mouseY < chipY + row.chip.height) {
                    float groupTop = getY() + row.y - top;
                    tag = new BadgeBubble.Target(row.line.id != null ? row.line.id : row.line.clientId,
                        row.chip.badge, chipX, chipY, row.chip.width, row.chip.height, groupTop,
                        groupTop + row.height, false);
                }
            }
            break;
        }
        hoverTag = tag != null || onBubble;
        if (hoverTag) {
            hoverTool = -1;
        }
        updateTooltip(hoverChip >= 0 ? hoverRow : -1, hoverChip);
        bubble.track(tag, onBubble, now);
    }

    private void updateTooltip(int row, int chip) {
        if (row == tooltipRow && chip == tooltipChip) {
            return;
        }
        long now = now();
        if (tooltipRow >= 0 && row < 0) {
            tooltipClosedAt = now;
        }
        // A tooltip right after another one shows at once; the first waits a moment.
        boolean quick = tooltipRow >= 0 || now - tooltipClosedAt < 400;
        tooltipRow = row;
        tooltipChip = chip;
        tooltipSince = quick ? now - TOOLTIP_DELAY_MS : now;
        if (row < 0) {
            tooltip = List.of();
            return;
        }
        OverlayState.Reaction r = rows.get(row).line.reactions.get(chip);
        Font small = Fonts.small();
        List<String> names = new ArrayList<>(4);
        if (r.mine()) {
            names.add(Texts.str("chat.reactions.you"));
        }
        names.addAll(r.who());
        int others = r.count() - names.size();
        String who;
        if (names.isEmpty()) {
            who = Texts.str("chat.reactions.people", r.count());
        } else if (others > 0) {
            who = Texts.str("chat.reactions.more", String.join(", ", names), others);
        } else if (names.size() >= 2) {
            who = Texts.str("chat.reactions.two", String.join(", ", names.subList(0, names.size() - 1)),
                names.get(names.size() - 1));
        } else {
            who = names.get(0);
        }
        List<FormattedCharSequence> lines = new ArrayList<>(2);
        lines.add(Texts.visual(Texts.fit(small, who, 150)));
        float width = small.width(Texts.fit(small, who, 150));
        if (canReact && reactable(rows.get(row))) {
            String act = r.mine() ? Texts.str("chat.reactions.remove") : Texts.str("chat.reactions.me_too", r.shown());
            lines.add(Texts.visual(act));
            width = Math.max(width, Fonts.ui().width(act));
        }
        tooltip = lines;
        tooltipW = width + 12;
    }

    private void drawRow(GuiGraphicsExtractor g, int index, Row row, float y, int x0, int w, float right, int mouseX,
        int mouseY, long now, Font ui, Font small) {
        float rowTop = y;
        if (row.line == null) {
            // "Ece ekibe katıldı": the launcher's quiet line, its waving hand in gold.
            float lineW = 8 + 4 + row.arrivalW;
            float lx = Draw.snap(x0 + (w - lineW) / 2f);
            float mid = y + ARRIVAL_H / 2f;
            Draw.icon(g, Icon.WAVE, 8, lx, Draw.snap(mid - 4f), Theme.GOLD_300);
            Draw.text(g, small, row.arrival, lx + 12, Fonts.centeredY(Fonts.Face.SMALL, mid), Theme.ASH_400);
            return;
        }
        if (row.divider) {
            float lw = dividerW;
            float mid = y + DIVIDER_H / 2f - 1;
            Draw.hairline(g, x0, x0 + (w - lw) / 2f - 6, mid, Theme.withAlpha(Theme.GOLD_400, 0.35f));
            Draw.hairline(g, x0 + (w + lw) / 2f + 6, right, mid, Theme.withAlpha(Theme.GOLD_400, 0.35f));
            Draw.text(g, small, dividerWord, x0 + (w - lw) / 2f, Fonts.centeredY(Fonts.Face.SMALL, mid),
                Theme.GOLD_300);
        }
        if (row.name != null) {
            float ny = rowTop + (row.divider ? DIVIDER_H : 0);
            if (row.chip != null) {
                row.chip.draw(g, x0 + 1, ny + SMALL_H / 2f);
            }
            Draw.text(g, small, row.name, x0 + 1 + TagChip.room(row.chip), Fonts.centeredY(Fonts.Face.SMALL,
                ny + SMALL_H / 2f), Theme.ASH_300);
        }
        if (row.quote) {
            drawQuote(g, index, row, rowTop + row.quoteTop, x0, right, small);
        }
        float by = rowTop + row.bubbleTop;
        float bx = row.mine ? right - row.bubbleW : x0;
        boolean pending = row.delivery == OverlayState.Delivery.PENDING;
        // The message a quote led to, lit for a moment.
        if (flashLine == row.line) {
            float t = Math.min(1f, (now - flashAt) / (float) FLASH_MS);
            if (t >= 1f) {
                flashLine = null;
            } else if (reducedMotion) {
                Draw.frame(g, Frames.FOCUS_BUBBLE, bx, by, row.bubbleW, row.bubbleH);
            } else {
                Draw.pushAlpha(1f - Ease.OUT.at(t));
                Draw.frame(g, Frames.FLASH, bx - 2, by - 2, row.bubbleW + 4, row.bubbleH + 4);
                Draw.popAlpha();
            }
        }
        if (row.sticker != null) {
            drawSticker(g, row, bx, by, pending, now);
        } else if (row.unknownSticker) {
            Draw.pushAlpha(pending ? 0.7f : 1f);
            Draw.frame(g, Frames.STICKER_TILE, bx, by, STICKER, STICKER);
            Draw.icon(g, Icon.STICKER, 12, bx + STICKER / 2f - 6, by + STICKER / 2f - 10, Theme.ASH_400);
            FormattedCharSequence label = row.text.get(0);
            Draw.text(g, small, label, bx + (STICKER - small.width(label)) / 2f,
                Fonts.centeredY(Fonts.Face.SMALL, by + STICKER / 2f + 9), Theme.ASH_400);
            Draw.popAlpha();
        } else {
            drawBubble(g, row, bx, by, pending, ui, small);
        }
        if (index == selected && keyboardFocus() && stripRow < 0) {
            // While the quick reactions are open the keys are there: one gold ring, on their cell.
            Draw.frame(g, Frames.FOCUS_BUBBLE, bx, by, row.bubbleW, row.bubbleH);
        }
        if (hoverLink && index == hoverRow) {
            g.requestCursor(CursorTypes.POINTING_HAND);
        }
        if (row.chips > 0) {
            drawChips(g, index, row, rowTop + row.chipsTop, x0, right, small, ui);
        }
        if (row.meta) {
            drawMeta(g, row, rowTop + row.metaTop, x0, right, small);
        }
        if (row.reason != null) {
            float ry = rowTop + row.metaTop + (row.meta ? SMALL_H : 0) + 2;
            Draw.text(g, small, row.reason, right - row.reasonW - 1,
                Fonts.centeredY(Fonts.Face.SMALL, ry + SMALL_H / 2f), Theme.FLARE_300);
            float ay = y - row.y + row.actionsY;
            drawAction(g, row.firstAction, right - row.firstActionX, ay, row.firstActionW, mouseX, mouseY, small);
            drawAction(g, row.secondAction, right - row.secondActionX, ay, row.secondActionW, mouseX, mouseY,
                small);
        }
    }

    private void drawQuote(GuiGraphicsExtractor g, int index, Row row, float qy, int x0, float right, Font small) {
        float qx = row.mine ? right - row.quoteW : x0;
        boolean lit = hoverQuote && index == hoverRow;
        // A thin bar, then the quoted words: quiet, so the message itself stays the thing to read.
        Draw.fill(g, qx + 1, qy + 1, qx + 2.5f, qy + QUOTE_H - 1, lit ? Theme.ASH_300 : Theme.withAlpha(Theme.ASH_500,
            0.8f));
        float tx = qx + 6;
        float textY = Fonts.centeredY(Fonts.Face.SMALL, qy + QUOTE_H / 2f);
        if (row.quoteName != null) {
            Draw.text(g, small, row.quoteName, tx, textY, lit ? Theme.BONE_100 : Theme.ASH_300);
            tx += row.quoteNameW;
        }
        // "Önceki bir mesaj" and "Mesaj silindi" as quiet as the rest (ash-500 is never text: 4:1 there).
        Draw.text(g, small, row.quoteText, tx, textY, lit && !row.quoteQuiet ? Theme.ASH_300 : Theme.ASH_400);
        if (lit) {
            g.requestCursor(CursorTypes.POINTING_HAND);
        }
    }

    private void drawBubble(GuiGraphicsExtractor g, Row row, float bx, float by, boolean pending, Font ui,
        Font small) {
        Frames frame = row.line.deleted ? Frames.BUBBLE_DELETED
            : row.delivery == OverlayState.Delivery.FAILED ? Frames.BUBBLE_FAILED
            : row.mine ? Frames.BUBBLE_MINE : Frames.BUBBLE_THEIRS;
        Draw.pushAlpha(pending ? 0.7f : 1f);
        Draw.frame(g, frame, bx, by, row.bubbleW, row.bubbleH);
        float ty = by + BUBBLE_PAD_Y;
        if (row.line.deleted) {
            Draw.text(g, small, row.text.get(0), bx + BUBBLE_PAD_X, Fonts.centeredY(Fonts.Face.SMALL, ty + SMALL_H / 2f),
                Theme.ASH_400);
        } else {
            int ink = row.mine ? Theme.BONE_50 : Theme.BONE_100;
            for (int i = 0; i < row.text.size(); i++) {
                Draw.text(g, ui, row.text.get(i), bx + BUBBLE_PAD_X, Fonts.centeredY(Fonts.Face.UI, ty + LINE_H / 2f),
                    ink);
                ty += LINE_H;
            }
            if (row.cardSite != null) {
                ty += CARD_GAP - 1;
                Draw.hairline(g, bx + BUBBLE_PAD_X, bx + row.bubbleW - BUBBLE_PAD_X, ty,
                    Theme.withAlpha(0xFFFFFF, 0.08f));
                float cy = Fonts.centeredY(Fonts.Face.SMALL, ty + 1 + SMALL_H / 2f);
                Draw.text(g, small, row.cardSite, bx + BUBBLE_PAD_X, cy, Theme.ASH_300);
                Draw.text(g, small, row.cardTitle, bx + BUBBLE_PAD_X + row.cardTitleX, cy, Theme.ASH_400);
            }
        }
        Draw.popAlpha();
    }

    private void drawSticker(GuiGraphicsExtractor g, Row row, float bx, float by, boolean pending, long now) {
        float alpha = pending ? 0.7f : 1f;
        long dur = StickerMotion.duration(row.sticker.motion());
        boolean moving = row.appearedAt > 0 && now - row.appearedAt < dur && !reducedMotion;
        Matrix3x2fStack m = g.pose();
        m.pushMatrix();
        if (moving) {
            float t = (now - row.appearedAt) / (float) dur;
            StickerMotion.at(row.sticker.motion(), t, STICKER, pose);
            float px = bx + STICKER / 2f;
            float py = by + (pose[StickerMotion.PIVOT] > 0f ? STICKER : STICKER / 2f);
            m.translate(px + pose[StickerMotion.DX], py + pose[StickerMotion.DY]);
            m.rotate((float) Math.toRadians(pose[StickerMotion.ANGLE]));
            m.scale(pose[StickerMotion.SX], pose[StickerMotion.SY]);
            m.translate(-px, -py);
        }
        Draw.pushAlpha(alpha);
        float sx = moving ? bx : Draw.snap(bx);
        float sy = moving ? by : Draw.snap(by);
        if (!StickerArt.draw(g, row.sticker, STICKER, sx, sy)) {
            Draw.frame(g, Frames.STICKER_TILE, bx, by, STICKER, STICKER);
        }
        Draw.popAlpha();
        m.popMatrix();
    }

    private void drawChips(GuiGraphicsExtractor g, int index, Row row, float cy0, int x0, float right, Font small,
        Font ui) {
        float rowW = chipsWidth(row);
        boolean pressable = canReact && reactable(row);
        for (int i = 0; i < row.chips; i++) {
            OverlayState.Reaction r = row.line.reactions.get(i);
            float cx = row.mine ? right - rowW + row.chipX[i] : x0 + row.chipX[i];
            float cy = cy0 + row.chipY[i];
            boolean lit = pressable && index == hoverRow && i == hoverChip;
            Frames frame = r.mine() ? Frames.REACTION_MINE : lit ? Frames.REACTION_HOVER : Frames.REACTION;
            Draw.frame(g, frame, cx, cy, row.chipW[i], CHIP_H);
            if (lit && r.mine()) {
                Draw.frame(g, Frames.REACTION_HOVER, cx, cy, row.chipW[i], CHIP_H, 0x80FFFFFF);
            }
            float textY = Fonts.centeredY(Fonts.Face.UI, cy + CHIP_H / 2f);
            Draw.text(g, ui, row.chipEmoji[i], cx + 4, textY, r.mine() ? Theme.GOLD_200 : Theme.BONE_100);
            Draw.text(g, small, row.chipCount[i], cx + 4 + row.chipEmojiW[i] + 3,
                Fonts.centeredY(Fonts.Face.SMALL, cy + CHIP_H / 2f), r.mine() ? Theme.GOLD_200 : Theme.ASH_300);
            if (lit) {
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
        }
    }

    private void drawMeta(GuiGraphicsExtractor g, Row row, float my, int x0, float right, Font small) {
        float mx = row.mine ? right - row.metaWidth - 1 : x0 + 1;
        float textY = Fonts.centeredY(Fonts.Face.SMALL, my + SMALL_H / 2f);
        if (row.seen) {
            // "Görüldü" in place of the tick: no time of its own, quiet.
            Draw.text(g, small, seenWord, mx, textY, Theme.ASH_300);
            mx += seenW + 4;
        } else if (row.mine) {
            Icon mark = switch (row.delivery) {
                case PENDING -> Icon.CLOCK;
                case FAILED -> Icon.WARNING;
                case SENT -> Icon.CHECK;
            };
            int markColor = row.delivery == OverlayState.Delivery.FAILED ? Theme.FLARE_400
                : row.delivery == OverlayState.Delivery.SENT ? Theme.GOLD_300 : Theme.ASH_400;
            Draw.icon(g, mark, 8, mx, my + 0.5f, markColor);
            mx += 11;
        }
        Draw.text(g, small, row.time, mx, textY, Theme.ASH_400);
        if (row.edited != null) {
            Draw.text(g, small, row.edited, mx + row.timeW + 4, textY, Theme.ASH_400);
        }
    }

    /** "Tepki ver" and "Yanıtla" beside the message under the pointer (or picked with the keys). */
    private void drawToolbar(GuiGraphicsExtractor g, long now) {
        // The open strip has both actions, a tag's bubble is about the sender: one popover at a time.
        int index = stripRow >= 0 || hoverTag || bubble.showing(now) ? -1 : hoverRow >= 0 ? hoverRow : selected;
        if (index < 0 || index >= rows.size()) {
            return;
        }
        Row row = rows.get(index);
        int n = toolCount(row);
        if (n == 0) {
            return;
        }
        float y = getY() + row.y + row.bubbleTop - scroll.value() + TOOL_PAD - 1 - TOOL_PAD;
        if (y < getY() - 2 || y + TOOL + 2 * TOOL_PAD > Math.min(getY() + getHeight(), visibleBottom) + 2) {
            return;
        }
        float x = toolbarX(row, n);
        Draw.frame(g, Frames.POPOVER, x, y, toolbarW(n), TOOL + 2 * TOOL_PAD);
        boolean react = canReact && reactable(row);
        float bx = x + TOOL_PAD;
        for (int k = 0; k < n; k++) {
            int kind = react ? k : 1;
            boolean lit = index == hoverRow && hoverTool == kind;
            if (lit) {
                Draw.frame(g, Frames.CHIP_HOVER, bx, y + TOOL_PAD, TOOL, TOOL);
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            Draw.icon(g, kind == 0 ? Icon.SMILEY : Icon.REPLY, 10, bx + 2, y + TOOL_PAD + 2,
                lit ? Theme.BONE_50 : Theme.ASH_300);
            bx += TOOL + TOOL_GAP;
        }
    }

    /** The six quick reactions and "Yanıtla", over the message (its toolbar steps aside meanwhile). */
    private void drawStrip(GuiGraphicsExtractor g, long now, Font small) {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return;
        }
        Row row = rows.get(stripRow);
        int n = stripCount();
        if (n == 0) {
            closeStrip();
            return;
        }
        float t = Math.min(1f, (now - stripOpenedAt) / (float) STRIP_MS);
        float eased = Ease.OUT_EXPO.at(t);
        float x = stripX(row, n);
        float y = stripY(row);
        float w = stripW(n);
        float h = QUICK_CELL + 6;
        Matrix3x2fStack m = g.pose();
        m.pushMatrix();
        if (!reducedMotion && eased < 1f) {
            // From the toolbar it came out of: a little growth from that corner.
            float ox = row.mine ? x + w : x;
            float oy = y + h;
            float s = 0.94f + 0.06f * eased;
            m.translate(ox, oy);
            m.scale(s, s);
            m.translate(-ox, -oy);
        }
        Draw.pushAlpha(Ease.OUT.at(t));
        Draw.frame(g, Frames.POPOVER, x, y, w, h);
        boolean reactions = canReact && reactable(row);
        int quick = reactions ? QUICK.length : 0;
        for (int i = 0; i < n; i++) {
            float cx = x + 3 + i * QUICK_CELL;
            float cy = y + 3;
            boolean lit = i == hoverQuick || (keyboardFocus() && i == stripFocus);
            if (lit) {
                Draw.frame(g, Frames.CELL_HOVER, cx, cy, QUICK_CELL, QUICK_CELL);
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            if (keyboardFocus() && i == stripFocus) {
                Draw.frame(g, Frames.CELL_FOCUS, cx, cy, QUICK_CELL, QUICK_CELL);
            }
            if (i < quick) {
                OverlayState.Reaction given = row.line.reaction(QUICK_SHOWN[i]);
                if (given != null && given.mine()) {
                    // Already given: a gold dot under it (pressing it takes it back).
                    Draw.disc(g, 3, cx + QUICK_CELL / 2f, cy + QUICK_CELL - 1f, Theme.GOLD_400);
                }
                // The game's emoji glyphs are 16 texels: at twice the text size they stay whole pixels.
                m.pushMatrix();
                m.translate(cx + (QUICK_CELL - 2f * quickW[i]) / 2f + 1f, cy + 1f);
                m.scale(2f, 2f);
                Draw.text(g, Fonts.ui(), quickLines[i], 0, -0.5f, Theme.BONE_50);
                m.popMatrix();
            } else {
                Draw.icon(g, Icon.REPLY, 10, cx + (QUICK_CELL - 10) / 2f, cy + (QUICK_CELL - 10) / 2f,
                    lit ? Theme.BONE_50 : Theme.ASH_300);
            }
        }
        Draw.popAlpha();
        m.popMatrix();
    }

    private void drawTooltip(GuiGraphicsExtractor g, long now, Font small) {
        if (tooltipRow < 0 || tooltipRow >= rows.size() || tooltip.isEmpty() || now - tooltipSince < TOOLTIP_DELAY_MS
            || stripRow >= 0) {
            return;
        }
        Row row = rows.get(tooltipRow);
        if (tooltipChip >= row.chips) {
            return;
        }
        float rowW = chipsWidth(row);
        float right = getX() + getWidth();
        float cx = row.mine ? right - rowW + row.chipX[tooltipChip] : getX() + row.chipX[tooltipChip];
        float cy = getY() + row.y + row.chipsTop + row.chipY[tooltipChip] - scroll.value();
        float h = 6 + tooltip.size() * 10;
        float x = Math.max(getX(), Math.min(right - tooltipW, cx + row.chipW[tooltipChip] / 2f - tooltipW / 2f));
        float y = cy - h - 3;
        if (y < getY()) {
            y = cy + CHIP_H + 3;
        }
        float t = Math.min(1f, (now - tooltipSince - TOOLTIP_DELAY_MS) / 110f);
        Draw.pushAlpha(Ease.OUT.at(t));
        Draw.frame(g, Frames.TOOLTIP, x, y, tooltipW, h);
        float ty = y + 3;
        for (int i = 0; i < tooltip.size(); i++) {
            Draw.text(g, i == 0 ? small : Fonts.ui(), tooltip.get(i), x + 6,
                Fonts.centeredY(i == 0 ? Fonts.Face.SMALL : Fonts.Face.UI, ty + 5), i == 0 ? Theme.BONE_100
                    : Theme.GOLD_300);
            ty += 10;
        }
        Draw.popAlpha();
    }

    private void drawAction(GuiGraphicsExtractor g, FormattedCharSequence label, float x, float y, float w, int mouseX,
        int mouseY, Font small) {
        boolean lit = isHovered() && mouseX >= x && mouseX < x + w && mouseY >= y && mouseY < y + 12;
        if (lit) {
            Draw.frame(g, Frames.CHIP_HOVER, x, y - 1, w, 14);
        }
        Draw.text(g, small, label, x + 4, Fonts.centeredY(Fonts.Face.SMALL, y + 6), lit ? Theme.BONE_100
            : Theme.ASH_300);
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("chat.list"));
        output.add(NarratedElementType.HINT, lastNarration);
    }
}
