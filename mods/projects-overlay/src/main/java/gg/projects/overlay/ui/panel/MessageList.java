package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.chat.ChatTime;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.emoji.EmojiCatalog;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.state.Presence;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerMotion;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.MarkTip;
import gg.projects.overlay.ui.Marks;
import gg.projects.overlay.ui.PresenceDot;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Function;
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
 * <p>CHAT3 (OVL-SOC2): after the quick reactions a "+" ("Başka tepki") opens the emoji picker to react with any emoji.
 * On the player's own message the toolbar carries a "…" too, and the same strip a second row: "Düzenle" (15 minutes,
 * not a sticker) and "Sil" (a week, asked first), as the launcher's message menu. The arrows walk the strip (↑ and ↓
 * between its rows), Enter does the one picked, and the narrator says each.
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
 *
 * <p>Profile badges (BADGES-UI): the name over someone's group carries their first badge right of it ({@link Marks}),
 * the name giving way; the pointer resting on it names all of them ({@link MarkTip}), and the narrator says them after
 * the sender's name.
 *
 * <p>Days and times (CHAT-UX, as the launcher; {@link ChatTime}): a quiet day line over the first message of each
 * local day ("Bugün", "Dün", "3 gün önce", "28 Ekim"); a group is one sender's messages up to five minutes apart on
 * one day, broken by a day line, "Yeni mesajlar" or a quiet line, its time under its last message; the pointer
 * resting on a bubble shows the message's full time ("28 Ekim 2026 Çarşamba 14:05"). At midnight a frame lays the
 * list out again (no timer: the open screen draws every frame anyway). "Yeni mesajlar" is bone words between ash
 * lines, as the launcher's (one gold per screen).
 */
public final class MessageList extends PanelWidget {
    private static final int BUBBLE_PAD_X = 6;
    private static final int BUBBLE_PAD_Y = 4;
    private static final int LINE_H = 10;
    private static final int SMALL_H = 9;
    /**
     * OVL-NEXT: the column before a sender's name over a group in crew chat where their presence dot stands (the
     * conversation chips' room); kept whether or not they are online, so a name never moves when the dot comes or goes.
     */
    private static final int DOT_LEAD = PresenceDot.DOT + 4;
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
    /** A day line over the first message of a local day (CHAT-UX). */
    private static final int DAY_H = 14;
    /** The tooltip's "chip" while it says a message's full time (CHAT-UX); a reaction chip's index otherwise. */
    private static final int TIME_TIP = -2;
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
    /** CHAT3: the player's own message can be edited this long after it went, and deleted this long. */
    static final long EDIT_WINDOW_MS = 15L * 60_000L;
    static final long DELETE_WINDOW_MS = 7L * 86_400_000L;
    /**
     * The strip's cells after the quick reactions (whose kinds are their indexes): "+", "Yanıtla", "Düzenle", "Sil";
     * SQOL-O, the voice room's chat: "Kopyala" and "Rapor Et".
     */
    private static final int CELL_MORE = 100;
    private static final int CELL_REPLY = 101;
    private static final int CELL_EDIT = 102;
    private static final int CELL_DELETE = 103;
    private static final int CELL_COPY = 104;
    private static final int CELL_REPORT = 105;
    /** The toolbar's buttons: "Tepki ver", "Yanıtla" and, on the player's own message, "…" (CHAT3). */
    private static final int TOOL_REACT = 0;
    private static final int TOOL_REPLY = 1;
    private static final int TOOL_MORE = 2;
    /** The strip's second row: "Düzenle" and "Sil" as words, the failed message's actions' size. */
    private static final int ACTION_H = 14;
    private static final int ACTION_GAP = 2;

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

        /** CHAT3: "Düzenle" on the player's own message (the field takes its words). */
        default void edit(OverlayState.ChatLine line) {
        }

        /** CHAT3: "Sil" on the player's own message (asked first; it goes for everyone). */
        default void delete(OverlayState.ChatLine line) {
        }

        /** CHAT3: "Başka tepki": the emoji picker gives the reaction to this message. */
        default void moreReactions(OverlayState.ChatLine line) {
        }

        /** SQOL-O, the voice room's chat: "Kopyala" (the message's words to the clipboard). */
        default void copy(OverlayState.ChatLine line) {
        }

        /** SQOL-O, the voice room's chat: "Rapor Et" on someone else's message. */
        default void report(OverlayState.ChatLine line) {
        }
    }

    private static final class Row {
        /** The message, or null for a quiet line among them ("Ece ekibe katıldı") or a day line. */
        OverlayState.ChatLine line;
        /** CHAT-UX: a day line's words ("Bugün", "3 gün önce", "28 Ekim"); null for any other row. */
        FormattedCharSequence day;
        float dayW;
        boolean mine;
        FormattedCharSequence name;
        /** The name's width (a right click on it opens the sender's menu, OVL-UX2). */
        float nameW;
        /** TEAM6: the tag the sender wears, before their name over a group in crew chat. */
        TagChip chip;
        /** OVL-NEXT: the room before the tag and name for the sender's presence dot (crew chat's groups). */
        int lead;
        /**
         * BADGES-UI: the sender's profile badges the catalogue knows (someone else's message in crew chat); the first
         * is drawn right of their name over a group, every one is said with the message.
         */
        List<OverlayState.MarkDef> marks = List.of();
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
        /**
         * OVL-SOC 2 (SOC-NEXT §3.7): the folded lines of someone the player ignores (their first line's key; null for
         * any other row), and that first line (the keys land on it once shown).
         */
        String fold;
        OverlayState.ChatLine foldLine;
        FormattedCharSequence foldAction;
        float foldActionX;
        float foldActionW;
    }

    private final Actions actions;
    private final List<Row> rows = new ArrayList<>();
    /** OVL-SOC 2: the folds of ignored lines the player opened in this conversation ("Göster"). */
    private final Set<String> revealed = new HashSet<>();
    /** The first line of a fold just opened: the keys go to it after the next layout. */
    private OverlayState.ChatLine revealedLine;
    private final Spring scroll = Spring.snappy(0f);
    private final Set<String> known = new HashSet<>();
    private final float[] pose = new float[StickerMotion.SIZE];
    /** TEAM6: the bubble of the tag before a sender's name (the pointer only: messages take no focus one by one). */
    private final BadgeBubble bubble = new BadgeBubble();
    /** BADGES-UI: the tooltip of the badge right of a sender's name (the pointer only, as the tag's bubble). */
    private final MarkTip markTip = new MarkTip();
    /** OVL-NEXT: whose presence dot a sender's name carries (the state's, as of the last refresh). */
    private Function<String, Presence> presenceOf = id -> Presence.OFFLINE;
    private String conversation;
    private boolean crew;
    /**
     * SQOL-O: the voice room's own chat ({@code v:<room>}): names over groups as in crew chat; no reactions, replies or
     * edits; "Kopyala" on any message, "Sil" on the player's own, "Rapor Et" on others'.
     */
    private boolean room;
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
    /** SQOL-O: a room chat message its leader, a moderator or Projects took away. */
    private String removedText = "";
    private FormattedCharSequence editedWord = FormattedCharSequence.EMPTY;
    private float editedW;
    private FormattedCharSequence seenWord = FormattedCharSequence.EMPTY;
    private float seenW;
    private FormattedCharSequence dividerWord = FormattedCharSequence.EMPTY;
    private float dividerW;
    private FormattedCharSequence newBelowWord = FormattedCharSequence.EMPTY;
    private float newBelowWidth;
    /** The empty conversation's words, on up to three lines (SQOL-O: the room chat's sentence is long). */
    private final List<FormattedCharSequence> emptyLines = new ArrayList<>(3);
    private final List<Float> emptyWidths = new ArrayList<>(3);
    private final FormattedCharSequence[] quickLines = new FormattedCharSequence[QUICK.length];
    // What the player may do now (CHAT2): set on every refresh.
    private boolean canReply;
    private boolean canReact;
    /** CHAT3: the player may edit (the field is open) and delete their own messages; the emoji picker can open. */
    private boolean canEdit;
    private boolean canDelete;
    private boolean morePicker;
    private FormattedCharSequence editWord = FormattedCharSequence.EMPTY;
    private float editW;
    private FormattedCharSequence deleteWord = FormattedCharSequence.EMPTY;
    private float deleteW;
    /** SQOL-O: "Kopyala" and "Rapor Et" in the room chat's strip. */
    private FormattedCharSequence copyWord = FormattedCharSequence.EMPTY;
    private float copyW;
    private FormattedCharSequence reportWord = FormattedCharSequence.EMPTY;
    private float reportW;
    private boolean turkish = true;
    // Pointer and keyboard (row indexes; -1 none).
    private int hoverRow = -1;
    private int hoverChip = -1;
    private int hoverTool = -1;
    private int hoverQuick = -1;
    /**
     * The pointer rests on a sender's tag or badge: its bubble or tooltip is the one popover there (no toolbar beside
     * the message).
     */
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
        emptyLines.clear();
        emptyWidths.clear();
        for (FormattedCharSequence line : Texts.wrap(small, words, Math.max(60, getWidth() - 16), 3)) {
            emptyLines.add(line);
            emptyWidths.add((float) small.width(line));
        }
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
        bubble.setReducedMotion(value);
    }

    /** Closes the tag's bubble and a badge's tooltip (another conversation, another page). */
    public void dismissBubble() {
        bubble.dismiss(now());
        markTip.track(null, null, 0f, 0f, now());
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
        // SQOL-O: the voice room's chat names its senders over their groups, as crew chat does.
        room = Protocol.roomConversation(conv);
        crew = crewChat || room;
        unreadAtOpen = Math.max(0, unread);
        dividerLine = null;
        laidOutRevision = -1;
        known.clear();
        rows.clear();
        revealed.clear();
        revealedLine = null;
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
        presenceOf = state::presenceOf;
        OverlayState.Conversation conv = state.conversation(conversation);
        boolean writable = conv != null && conv.writable() && state.online() && state.has("chat");
        // SQOL-O: the voice room's chat has no reactions, replies or edits; the player's own message is deleted there
        // while the room lasts.
        canReply = !room && writable && state.has("replies");
        canReact = !room && writable && state.has("reactions");
        // CHAT3: an edit is written in the field (open only while the player may write); a delete needs no field.
        canEdit = !room && writable && state.chat3();
        canDelete = conv != null && state.online() && (room || state.chat3());
        if (conversation == null) {
            rows.clear();
            contentHeight = 0f;
            boolean tr = Texts.turkish();
            if (laidOutRevision != -2 || tr != turkish || getWidth() != laidOutWidth) {
                // Once, not on every frame: nothing is open, nothing changes.
                turkish = tr;
                setEmpty(Texts.str("chat.none"), Fonts.small());
                laidOutRevision = -2;
                laidOutWidth = getWidth();
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
        if (revealedLine != null) {
            // A fold just opened: the keys stay on its first message.
            selected = indexOf(revealedLine);
            revealedLine = null;
        }
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
        removedText = Texts.str("room.removed");
        String copyWords = Texts.str("chat.copy");
        copyWord = Texts.visual(copyWords);
        copyW = small.width(copyWords) + 8;
        String reportWords = Texts.str("chat.report");
        reportWord = Texts.visual(reportWords);
        reportW = small.width(reportWords) + 8;
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
        String editWords = Texts.str("chat.edit");
        editWord = Texts.visual(editWords);
        editW = small.width(editWords) + 8;
        String deleteWords = Texts.str("chat.delete");
        deleteWord = Texts.visual(deleteWords);
        deleteW = small.width(deleteWords) + 8;
        morePicker = EmojiCatalog.available(turkish ? "tr" : "en");
        // SQOL-O: an empty room chat says who sees it and that it goes with the room.
        setEmpty(Texts.str(room ? "room.empty" : "chat.empty"), small);
        rows.clear();
        // The frames' clock (`now`) counts from the game's start, messages carry wall times: the day lines and a
        // message's ten seconds to be answered read the wall clock, and the next moment either changes is put on the
        // frames' clock, where a frame lays the list out again (the day lines at midnight; no timer).
        ZoneId zone = ZoneId.systemDefault();
        ChatTime.Words words = Texts::str;
        long wall = System.currentTimeMillis();
        nextTimeout = now + (ChatTime.nextDay(wall, zone) - wall);
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
        long lastDay = Long.MIN_VALUE;
        for (int i = 0; i < lines.size(); i++) {
            OverlayState.ChatLine line = lines.get(i);
            OverlayState.ChatLine next = i + 1 < lines.size() ? lines.get(i + 1) : null;
            while (nextJoin < joins.size() && joins.get(nextJoin).at() <= line.at) {
                OverlayState.Arrival join = joins.get(nextJoin++);
                long joinDay = ChatTime.day(join.at(), zone);
                if (joinDay != lastDay) {
                    y = addDay(ChatTime.dayLine(join.at(), wall, zone, words), joinDay, y, previous != null, small,
                        first, now, seenIds);
                    previous = rows.get(rows.size() - 1);
                    lastDay = joinDay;
                }
                y = addJoin(join, y, previous != null, small, first, now, seenIds);
                previous = rows.get(rows.size() - 1);
            }
            long day = ChatTime.day(line.at, zone);
            if (day != lastDay) {
                y = addDay(ChatTime.dayLine(line.at, wall, zone, words), day, y, previous != null, small, first, now,
                    seenIds);
                previous = rows.get(rows.size() - 1);
                lastDay = day;
            }
            // SOC-NEXT §3.7 (OVL-SOC 2): the lines in a row of someone the player ignores fold into one quiet line
            // with "Göster" (the launcher's crew chat); a fold opened shows them all.
            boolean continuing = previous != null && previous.line != null && previous.line.ignored
                && sameSender(previous.line, line);
            if (crew && line.ignored && !continuing && !revealed.contains(foldKey(line))) {
                int count = 1;
                while (i + count < lines.size()) {
                    OverlayState.ChatLine more = lines.get(i + count);
                    if (!more.ignored || !sameSender(more, line) || ChatTime.day(more.at, zone) != day
                        || nextJoin < joins.size() && joins.get(nextJoin).at() <= more.at) {
                        break;
                    }
                    count++;
                }
                for (int k = 0; k < count; k++) {
                    String key = foldKey(lines.get(i + k));
                    if (key != null) {
                        seenIds.add(key);
                    }
                }
                y = addFold(line, count, y, previous != null, small, first, now, seenIds);
                previous = rows.get(rows.size() - 1);
                i += count - 1;
                continue;
            }
            // A join line that comes before the next message ends this group.
            boolean joinNext = next != null && nextJoin < joins.size() && joins.get(nextJoin).at() <= next.at;
            Row row = new Row();
            row.line = line;
            row.mine = line.mine;
            row.delivery = line.deliveryAt(wall);
            if (row.delivery == OverlayState.Delivery.PENDING) {
                // Unanswered for ten seconds it says so, with "Tekrar dene" (CHAT-UX: it read the frames' clock as a
                // wall time, so it never did).
                nextTimeout = Math.min(nextTimeout, now + (line.createdAt + OverlayState.SEND_TIMEOUT_MS + 1 - wall));
            }
            // A group: one sender, one local day, each message at most five minutes after the one before (CHAT-UX).
            boolean newGroup = previous == null || previous.line == null || previous.mine != line.mine
                || !previous.line.from.equals(line.from) || i == dividerAt
                || !ChatTime.together(previous.line.at, line.at, zone);
            boolean lastOfGroup = next == null || joinNext || next.mine != line.mine || !next.from.equals(line.from)
                || i + 1 == dividerAt || !ChatTime.together(line.at, next.at, zone);
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
            if (crew && !line.mine) {
                // BADGES-UI: their badges, drawn over a group and said with every message of theirs.
                row.marks = state.marks(line.marks);
            }
            if (crew && !line.mine && newGroup) {
                // "[GK] Mert": the tag they wear before the name (TEAM6), their first badge after it (BADGES-UI); the
                // name gives way, the chip and the badge do not.
                row.chip = TagChip.of(state.badge(line.badge), TagChip.CHAT);
                // OVL-NEXT: their presence dot before it all, as a conversation chip's.
                row.lead = DOT_LEAD;
                String fitted = Texts.fit(small, line.from, Math.round(maxBubble - row.lead - TagChip.room(row.chip)
                    - Marks.room(row.marks)));
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
                String gone = line.removed ? removedText : deletedText;
                row.text = List.of(Texts.visual(gone));
                row.bubbleW = small.width(gone) + 2 * BUBBLE_PAD_X;
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
                String time = ChatTime.clock(line.at, zone);
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
                String reason = line.failureAt(wall);
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
            // SQOL-O: a room chat message is named by the room's id (no service id).
            row.actionable = !line.deleted && (room ? line.id != null : line.sid > 0)
                && row.delivery == OverlayState.Delivery.SENT;
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
            OverlayState.Arrival join = joins.get(nextJoin++);
            long joinDay = ChatTime.day(join.at(), zone);
            if (joinDay != lastDay) {
                y = addDay(ChatTime.dayLine(join.at(), wall, zone, words), joinDay, y, previous != null, small, first,
                    now, seenIds);
                previous = rows.get(rows.size() - 1);
                lastDay = joinDay;
            }
            y = addJoin(join, y, previous != null, small, first, now, seenIds);
            previous = rows.get(rows.size() - 1);
        }
        known.clear();
        known.addAll(seenIds);
        arrivedBelow = lastArrival > lastKnown;
        rowsEnd = y;
        contentHeight = rows.isEmpty() ? 0f : y + 4f + floatRoom;
        lastNarration = rows.isEmpty() ? Texts.tr(room ? "room.empty" : "chat.empty")
            : narrationOf(rows.get(rows.size() - 1));
        tooltipRow = -1;
    }

    /** Two lines by the same person: by their id where the launcher names it (crew chat), else by name. */
    private static boolean sameSender(OverlayState.ChatLine a, OverlayState.ChatLine b) {
        return a.by != null && b.by != null ? a.by.equals(b.by) : a.from.equals(b.from);
    }

    /** A line's key (its first one stands for a fold): the panel's own id first, as {@link #known} keeps them. */
    private static String foldKey(OverlayState.ChatLine line) {
        return line.clientId != null ? line.clientId : line.id;
    }

    /**
     * OVL-SOC 2: "Yok saydığın birinin 3 mesajı" and "Göster" in the place of someone ignored's lines (the launcher's
     * fold); returns where the next row starts.
     */
    private float addFold(OverlayState.ChatLine line, int count, float y, boolean gapBefore, Font small,
        boolean first, long now, Set<String> seenIds) {
        float top = y + (gapBefore ? GROUP_GAP - ROW_GAP : 0);
        Row row = new Row();
        String words = count == 1 ? Texts.str("chat.ignored.one") : Texts.str("chat.ignored.many", count);
        String show = Texts.str("chat.ignored.show");
        float showW = small.width(show) + 8;
        String fitted = Texts.fit(small, words, Math.max(20, Math.round(getWidth() - showW - 10)));
        row.arrival = Texts.visual(fitted);
        row.arrivalW = small.width(fitted);
        row.arrivalWords = Texts.tr("chat.ignored.narration", words);
        row.fold = foldKey(line) == null ? "" : foldKey(line);
        row.foldLine = line;
        row.foldAction = Texts.visual(show);
        row.foldActionW = showW;
        row.foldActionX = 1 + row.arrivalW + 4;
        row.y = top;
        row.height = ARRIVAL_H;
        String key = "fold:" + row.fold;
        seenIds.add(key);
        if (!known.contains(key) && !first) {
            row.appearedAt = now;
        }
        rows.add(row);
        return top + ARRIVAL_H + ROW_GAP;
    }

    /** The scenario driver: "Göster" on the first fold; false without one. */
    public boolean unfoldFirst() {
        for (Row row : rows) {
            if (row.fold != null) {
                unfold(row);
                return true;
            }
        }
        return false;
    }

    /** The folds of ignored lines laid out now (the scenario driver checks them). */
    public int folds() {
        int n = 0;
        for (Row row : rows) {
            if (row.fold != null) {
                n++;
            }
        }
        return n;
    }

    /** "Göster" on a fold: its lines show (the keys go to its first one). */
    private void unfold(Row row) {
        if (row.fold == null) {
            return;
        }
        revealed.add(row.fold);
        revealedLine = PanelInput.keyboardMode() ? row.foldLine : null;
        laidOutWidth = -1;
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

    /**
     * The day line over the first message (or quiet line) of a local day: "Bugün", "Dün", "3 gün önce", "28 Ekim"
     * (CHAT-UX, {@link ChatTime#dayLine}); returns where the next row starts. A new one fades in where it lands.
     */
    private float addDay(String words, long day, float y, boolean gapBefore, Font small, boolean first, long now,
        Set<String> seenIds) {
        float top = y + (gapBefore ? GROUP_GAP - ROW_GAP : 0);
        Row row = new Row();
        row.day = Texts.visual(words);
        row.dayW = small.width(words);
        row.arrivalWords = Component.literal(words);
        row.y = top;
        row.height = DAY_H;
        String key = "day:" + day;
        seenIds.add(key);
        if (!known.contains(key) && !first) {
            row.appearedAt = now;
        }
        rows.add(row);
        return top + DAY_H + ROW_GAP;
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
            case "link", "phone", "words", "rate", "muted", "off", "stickers", "links", "slow" -> reason;
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

    private Component narrationOf(Row row) {
        OverlayState.ChatLine line = row.line;
        if (line == null) {
            return row.arrivalWords == null ? Component.empty() : row.arrivalWords;
        }
        if (line.deleted) {
            return Texts.tr(line.removed ? "room.removed" : "chat.deleted");
        }
        // "Mert, Projects yöneticisi: …": a crewmate's badges after their name (BADGES-UI).
        String from = Marks.spoken(line.from, row.marks, Texts.current());
        if (line.sticker != null) {
            String name = row.sticker == null ? Texts.str("chat.sticker") : row.sticker.name(turkish);
            return line.mine ? Texts.tr("narration.sticker.mine", name)
                : Texts.tr("narration.sticker.theirs", from, name);
        }
        if (line.mine) {
            return Texts.tr("narration.mine", line.text);
        }
        Component said = Texts.tr("narration.theirs", from, line.text);
        // OVL-SOC 2: the gold ring is said too (never colour alone).
        return line.mention ? Component.empty().append(said).append(". ").append(Texts.tr("chat.mention"))
            : said;
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
            Row strip = stripRow < rows.size() ? rows.get(stripRow) : null;
            int count = strip == null ? 0 : stripCount(strip);
            if (count == 0) {
                closeStrip();
                return false;
            }
            int top = topCells(strip);
            stripFocus = Math.max(0, Math.min(count - 1, stripFocus));
            int before = stripFocus;
            if (event.isLeft()) {
                stripFocus = Math.max(0, stripFocus - 1);
            } else if (event.isRight()) {
                stripFocus = Math.min(count - 1, stripFocus + 1);
            } else if (event.isSelection() || event.isConfirmation()) {
                activateStrip(stripFocus);
            } else if (event.isDown() && stripFocus < top && count > top) {
                // CHAT3: ↓ goes to "Düzenle" or "Sil" under it, ↑ back to the cell over it.
                stripFocus = nearestCell(strip, cellCenter(strip, stripFocus), top, count);
            } else if (event.isUp() && stripFocus >= top && top > 0) {
                stripFocus = nearestCell(strip, cellCenter(strip, stripFocus), 0, top);
            } else if (event.isUp() || event.isDown()) {
                closeStrip();
                return keyPressed(event);
            } else {
                return false;
            }
            PanelInput.usedKeyboard();
            if (stripRow >= 0 && stripFocus != before) {
                sayStripCell();
            }
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
            if (row.line != null && actionCells(row) > 0) {
                // CHAT3: on the player's own message, its strip with the keys on "Düzenle" (the launcher's menu).
                PanelInput.usedKeyboard();
                openStrip(selected, topCells(row));
                sayStripCell();
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
                if (row.fold != null) {
                    unfold(row);
                } else if (row.quote && row.quoteTarget != null && !row.actionable) {
                    jumpTo(row.quoteTarget);
                } else if (row.actionable && stripCount(row) > 0) {
                    // The keys start on the strip's first cell, whatever it is (a reply alone, "Düzenle").
                    openStrip(selected, 0);
                    sayStripCell();
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
            // A fold of ignored lines is picked too (Enter shows them).
            if (rows.get(i).line != null || rows.get(i).fold != null) {
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
     * A right click on a sender's name (or the tag before it, or the badge after it) over a group in crew chat: their
     * person menu at the pointer, as the launcher's name does (OVL-UX2). On the player's own bubble (CHAT3): its strip
     * with "Düzenle" and "Sil", as the launcher's message menu. Anything else a right click lands on does nothing.
     */
    private void rightClick(float mx, float my) {
        if (my >= visibleBottom) {
            return;
        }
        float cy = my - getY() + scroll.value();
        for (int i = 0; i < rows.size(); i++) {
            Row row = rows.get(i);
            if (row.line == null || cy < row.y || cy > row.y + row.height) {
                continue;
            }
            if (row.mine || room) {
                // SQOL-O: in the voice room's chat someone else's bubble has its strip too ("Kopyala", "Rapor Et").
                float bx = bubbleX(row);
                float top = row.y + row.bubbleTop;
                if (actionCells(row) > 0 && mx >= bx && mx < bx + row.bubbleW && cy >= top
                    && cy < top + row.bubbleH) {
                    openStrip(i, topCells(row));
                    return;
                }
                if (row.mine) {
                    return;
                }
            }
            if (row.name == null) {
                continue;
            }
            float nameTop = row.y + (row.divider ? DIVIDER_H : 0);
            float nameRight = getX() + 1 + row.lead + TagChip.room(row.chip) + row.nameW + Marks.room(row.marks);
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
                return new float[] {getX() + 1 + row.lead + TagChip.room(row.chip) + row.nameW / 2f, y};
            }
        }
        return null;
    }

    /**
     * Scrolls message {@code id} into view as a player would with the wheel (the scenario driver: a sender's name above
     * the newest messages); false when the conversation does not have it.
     */
    public boolean revealForDriver(String id) {
        for (int i = 0; i < rows.size(); i++) {
            Row row = rows.get(i);
            if (row.line != null && id.equals(row.line.id)) {
                reveal(i);
                return true;
            }
        }
        return false;
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
            int cell = stripCellAt(mx, my);
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
            if (row.fold != null) {
                // A fold's "Göster" (or its words): its lines show.
                if (mx < getX() + row.foldActionX + row.foldActionW) {
                    selected = i;
                    unfold(row);
                }
                return;
            }
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
            if (tool == TOOL_REACT && canReact) {
                openStrip(i, -1);
                return;
            }
            if (tool == TOOL_REPLY && canReply) {
                actions.reply(row.line);
                return;
            }
            if (tool == TOOL_MORE) {
                // "…" on the player's own message: the same strip, its "Düzenle" and "Sil" under the reactions.
                openStrip(i, topCells(row));
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

    private void openStrip(int index, int focus) {
        Row row = rows.get(index);
        int count = stripCount(row);
        if (!row.actionable || count == 0) {
            return;
        }
        stripRow = index;
        stripLine = row.line;
        stripFocus = Math.max(0, Math.min(count - 1, focus));
        stripOpenedAt = now();
    }

    private void closeStrip() {
        stripRow = -1;
        stripLine = null;
    }

    /**
     * The strip's first row: six reactions and "+" while the message takes them, then "Yanıtla" when replies are on
     * (however it opened: the toolbar steps aside while the strip is open).
     */
    private int topCells(Row row) {
        int n = canReact && reactable(row) ? QUICK.length + (morePicker ? 1 : 0) : 0;
        return n + (canReply ? 1 : 0);
    }

    /**
     * CHAT3: the strip's second row on the player's own message: "Düzenle" while it may be edited, "Sil". SQOL-O, the
     * voice room's chat: "Kopyala" on any message with words, then "Sil" on the player's own, "Rapor Et" on others'.
     */
    private int actionCells(Row row) {
        if (row.line == null || !row.actionable) {
            return 0;
        }
        if (room) {
            return (copyable(row.line) ? 1 : 0) + (row.mine ? (canDelete ? 1 : 0) : 1);
        }
        if (!row.mine) {
            return 0;
        }
        long wall = System.currentTimeMillis();
        return (canEdit && editable(row.line, wall) ? 1 : 0) + (canDelete && deletable(row.line, wall) ? 1 : 0);
    }

    /** SQOL-O: a message whose words "Kopyala" takes (not a sticker, not taken away). */
    private static boolean copyable(OverlayState.ChatLine line) {
        return line.sticker == null && !line.deleted && line.text != null && !line.text.isBlank();
    }

    private int stripCount(Row row) {
        return topCells(row) + actionCells(row);
    }

    /** What strip cell {@code cell} of {@code row} is: a quick reaction's index or a {@code CELL_} kind; -1 none. */
    private int stripKind(Row row, int cell) {
        if (cell < 0) {
            return -1;
        }
        int c = cell;
        if (canReact && reactable(row)) {
            if (c < QUICK.length) {
                return c;
            }
            c -= QUICK.length;
            if (morePicker) {
                if (c == 0) {
                    return CELL_MORE;
                }
                c--;
            }
        }
        if (canReply) {
            if (c == 0) {
                return CELL_REPLY;
            }
            c--;
        }
        if (room && row.line != null && row.actionable) {
            // SQOL-O: "Kopyala", then "Sil" (the player's own) or "Rapor Et" (someone else's).
            if (copyable(row.line)) {
                if (c == 0) {
                    return CELL_COPY;
                }
                c--;
            }
            return c != 0 ? -1 : !row.mine ? CELL_REPORT : canDelete ? CELL_DELETE : -1;
        }
        if (row.line == null || !row.actionable || !row.mine) {
            return -1;
        }
        long wall = System.currentTimeMillis();
        if (canEdit && editable(row.line, wall)) {
            if (c == 0) {
                return CELL_EDIT;
            }
            c--;
        }
        return c == 0 && canDelete && deletable(row.line, wall) ? CELL_DELETE : -1;
    }

    private void activateStrip(int cell) {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return;
        }
        Row row = rows.get(stripRow);
        int kind = stripKind(row, cell);
        closeStrip();
        if (kind < 0) {
            return;
        }
        if (kind < QUICK.length) {
            OverlayState.Reaction given = row.line.reaction(QUICK_SHOWN[kind]);
            actions.react(row.line, given != null ? given.emoji() : QUICK[kind], QUICK_SHOWN[kind],
                given == null || !given.mine());
            return;
        }
        switch (kind) {
            case CELL_MORE -> actions.moreReactions(row.line);
            case CELL_REPLY -> actions.reply(row.line);
            case CELL_EDIT -> actions.edit(row.line);
            case CELL_DELETE -> actions.delete(row.line);
            case CELL_COPY -> actions.copy(row.line);
            case CELL_REPORT -> actions.report(row.line);
            default -> {
            }
        }
    }

    /** The narrator says the strip cell the keys are on ("👍 ile tepki ver", "Başka tepki", "Düzenle"…). */
    private void sayStripCell() {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return;
        }
        Component label = cellLabel(stripKind(rows.get(stripRow), stripFocus));
        if (label != null) {
            net.minecraft.client.Minecraft.getInstance().getNarrator().saySystemQueued(label);
        }
    }

    private static Component cellLabel(int kind) {
        if (kind < 0) {
            return null;
        }
        if (kind < QUICK.length) {
            return Texts.tr("chat.reactions.give", QUICK_SHOWN[kind]);
        }
        return switch (kind) {
            case CELL_MORE -> Texts.tr("chat.react.more");
            case CELL_REPLY -> Texts.tr("chat.reply");
            case CELL_EDIT -> Texts.tr("chat.edit");
            case CELL_DELETE -> Texts.tr("chat.delete");
            case CELL_COPY -> Texts.tr("chat.copy");
            case CELL_REPORT -> Texts.tr("chat.report");
            default -> null;
        };
    }

    /**
     * CHAT3: whether the player may still edit this message (as the service decides it): their own, confirmed (it has
     * the service's id), not deleted, not a sticker, younger than 15 minutes on this computer's clock.
     */
    static boolean editable(OverlayState.ChatLine line, long now) {
        return line != null && editable(line.mine, line.sid, line.deleted, line.sticker != null, line.at, now);
    }

    static boolean editable(boolean mine, long sid, boolean deleted, boolean sticker, long at, long now) {
        return mine && sid > 0 && !deleted && !sticker && now - at < EDIT_WINDOW_MS;
    }

    /** CHAT3: whether the player may still delete this message for everyone: their own, confirmed, a week. */
    static boolean deletable(OverlayState.ChatLine line, long now) {
        return line != null && deletable(line.mine, line.sid, line.deleted, line.at, now);
    }

    static boolean deletable(boolean mine, long sid, boolean deleted, long at, long now) {
        return mine && sid > 0 && !deleted && now - at < DELETE_WINDOW_MS;
    }

    // ---- Geometry --------------------------------------------------------------------------------------

    private float bubbleX(Row row) {
        return row.mine ? getX() + getWidth() - row.bubbleW : getX();
    }

    /** The toolbar's buttons beside a bubble (react, reply, and "…" on the player's own); 0 when the row has none. */
    private int toolCount(Row row) {
        if (!row.actionable) {
            return 0;
        }
        return (canReact && reactable(row) ? 1 : 0) + (canReply ? 1 : 0) + (actionCells(row) > 0 ? 1 : 0);
    }

    /** What the toolbar's {@code k}-th button is ({@code TOOL_}), -1 none. */
    private int toolKind(Row row, int k) {
        int c = k;
        if (canReact && reactable(row)) {
            if (c == 0) {
                return TOOL_REACT;
            }
            c--;
        }
        if (canReply) {
            if (c == 0) {
                return TOOL_REPLY;
            }
            c--;
        }
        return c == 0 && actionCells(row) > 0 ? TOOL_MORE : -1;
    }

    private float toolbarW(int n) {
        return n * TOOL + (n - 1) * TOOL_GAP + 2 * TOOL_PAD;
    }

    /** Beside the bubble; over its corner when the bubble leaves no room (a long message in a narrow panel). */
    private float toolbarX(Row row, int n) {
        float bx = bubbleX(row);
        float w = toolbarW(n);
        float x = row.mine ? bx - 4 - w : bx + row.bubbleW + 4;
        return Math.max(getX(), Math.min(getX() + getWidth() - w, x));
    }

    /** Which toolbar button ({@code TOOL_}) is at {@code mx}, row-local {@code ly}; -1 none. */
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
        for (int k = 0; k < n; k++) {
            float bx = tx + k * (TOOL + TOOL_GAP);
            if (mx >= bx && mx < bx + TOOL) {
                return toolKind(row, k);
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
    private float stripX(Row row) {
        float w = stripW(row);
        float x = row.mine ? bubbleX(row) + row.bubbleW - w : bubbleX(row);
        return Math.max(getX(), Math.min(getX() + getWidth() - w, x));
    }

    private float stripW(Row row) {
        int top = topCells(row);
        int n = stripCount(row);
        float w = top > 0 ? top * QUICK_CELL + 6 : 0f;
        if (n > top) {
            float actionsW = 6;
            for (int i = top; i < n; i++) {
                actionsW += actionW(stripKind(row, i)) + (i > top ? ACTION_GAP : 0);
            }
            w = Math.max(w, actionsW);
        }
        return w;
    }

    /** The reactions' row, then (CHAT3) a hairline and "Düzenle" and "Sil". */
    private float stripH(Row row) {
        int top = topCells(row);
        boolean acts = actionCells(row) > 0;
        return 6 + (top > 0 ? QUICK_CELL : 0) + (acts ? (top > 0 ? 2 : 0) + ACTION_H : 0);
    }

    /** The top of the strip's second row. */
    private static float actionsTop(float stripY, int top) {
        return stripY + 3 + (top > 0 ? QUICK_CELL + 2 : 0);
    }

    private float actionW(int kind) {
        return switch (kind) {
            case CELL_DELETE -> deleteW;
            case CELL_EDIT -> editW;
            case CELL_COPY -> copyW;
            case CELL_REPORT -> reportW;
            default -> 0f;
        };
    }

    /** The middle of strip cell {@code cell} from the strip's left edge (↑ and ↓ between its rows). */
    private float cellCenter(Row row, int cell) {
        int top = topCells(row);
        if (cell < top) {
            return 3 + cell * QUICK_CELL + QUICK_CELL / 2f;
        }
        float ax = 3;
        for (int i = top; i < cell; i++) {
            ax += actionW(stripKind(row, i)) + ACTION_GAP;
        }
        return ax + actionW(stripKind(row, cell)) / 2f;
    }

    /** The cell from {@code from} (inclusive) to {@code to} whose middle is nearest {@code center}. */
    private int nearestCell(Row row, float center, int from, int to) {
        int best = from;
        float bestDistance = Float.MAX_VALUE;
        for (int i = from; i < to; i++) {
            float d = Math.abs(cellCenter(row, i) - center);
            if (d < bestDistance) {
                bestDistance = d;
                best = i;
            }
        }
        return best;
    }

    private float stripY(Row row) {
        float top = getY() + row.y + row.bubbleTop - scroll.value();
        float h = stripH(row);
        float above = top - h - 3;
        return above >= getY() ? above : Math.min(getY() + getHeight() - h, top + row.bubbleH + 3);
    }

    /** The strip cell at {@code (mx, my)}: the reactions' row, then "Düzenle" and "Sil" (CHAT3); -1 none. */
    private int stripCellAt(float mx, float my) {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return -1;
        }
        Row row = rows.get(stripRow);
        int top = topCells(row);
        int n = stripCount(row);
        float x = stripX(row);
        float y = stripY(row);
        if (top > 0 && my >= y + 3 && my < y + 3 + QUICK_CELL) {
            int cell = (int) Math.floor((mx - x - 3) / QUICK_CELL);
            return cell >= 0 && cell < top ? cell : -1;
        }
        float ay = actionsTop(y, top);
        if (n > top && my >= ay && my < ay + ACTION_H) {
            float ax = x + 3;
            for (int i = top; i < n; i++) {
                float aw = actionW(stripKind(row, i));
                if (mx >= ax && mx < ax + aw) {
                    return i;
                }
                ax += aw + ACTION_GAP;
            }
        }
        return -1;
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
            // One quiet sentence in the middle, on up to three lines (SQOL-O: the room chat's is long).
            float lineTop = y0 + h / 2f - emptyLines.size() * LINE_H / 2f;
            for (int i = 0; i < emptyLines.size(); i++) {
                Draw.text(g, small, emptyLines.get(i), x0 + (w - emptyWidths.get(i)) / 2f,
                    Fonts.centeredY(Fonts.Face.SMALL, lineTop + i * LINE_H + LINE_H / 2f), Theme.ASH_400);
            }
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
        markTip.draw(g, now, x0, y0, x0 + w);
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
            markTip.track(null, null, 0f, 0f, now);
            return;
        }
        if (stripRow >= 0) {
            hoverQuick = stripCellAt(mouseX, mouseY);
        }
        if (mouseY >= visibleBottom) {
            // Under the floating line: nothing of the list shows there.
            updateTooltip(-1, -1);
            bubble.track(null, onBubble, now);
            markTip.track(null, null, 0f, 0f, now);
            return;
        }
        boolean onMark = false;
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
                float chipX = getX() + 1 + row.lead;
                if (mouseX >= chipX && mouseX < chipX + row.chip.width && mouseY >= chipY
                    && mouseY < chipY + row.chip.height) {
                    float groupTop = getY() + row.y - top;
                    tag = new BadgeBubble.Target(row.line.id != null ? row.line.id : row.line.clientId,
                        row.chip.badge, chipX, chipY, row.chip.width, row.chip.height, groupTop,
                        groupTop + row.height, false);
                }
            }
            if (row.name != null && !row.marks.isEmpty() && stripRow < 0 && tag == null) {
                // The badge right of the sender's name names all of theirs (BADGES-UI).
                float nameMid = getY() + row.y - top + (row.divider ? DIVIDER_H : 0) + SMALL_H / 2f;
                float markX = Marks.left(getX() + 1 + row.lead + TagChip.room(row.chip) + row.nameW);
                float markY = Marks.top(nameMid);
                if (Marks.over(mouseX, mouseY, markX, markY)) {
                    onMark = true;
                    markTip.track(row.line.id != null ? row.line.id : row.line.clientId, row.marks, markX, markY,
                        now);
                }
            }
            break;
        }
        if (!onMark) {
            markTip.track(null, null, 0f, 0f, now);
        }
        // The tag's bubble or a badge's tooltip is the one popover there (no toolbar, no time).
        hoverTag = tag != null || onBubble || onMark;
        if (hoverTag) {
            hoverTool = -1;
        }
        int tip = hoverChip >= 0 ? hoverChip : timeTipAt(mouseX, cy);
        updateTooltip(tip != -1 ? hoverRow : -1, tip);
        bubble.track(tag, onBubble, now);
    }

    /**
     * {@link #TIME_TIP} while the pointer rests on a message's bubble and on nothing with its own press or tip (a link,
     * the quote, the toolbar, a tag), else -1 (CHAT-UX).
     */
    private int timeTipAt(int mouseX, float cy) {
        if (hoverRow < 0 || hoverTool >= 0 || hoverQuote || hoverLink || hoverTag || stripRow >= 0) {
            return -1;
        }
        Row row = rows.get(hoverRow);
        if (row.line == null) {
            return -1;
        }
        float bx = bubbleX(row);
        float top = row.y + row.bubbleTop;
        return mouseX >= bx && mouseX < bx + row.bubbleW && cy >= top && cy < top + row.bubbleH ? TIME_TIP : -1;
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
        Font small = Fonts.small();
        if (chip == TIME_TIP) {
            // CHAT-UX: the message's full time, "28 Ekim 2026 Çarşamba 14:05".
            String full = ChatTime.full(rows.get(row).line.at, ZoneId.systemDefault(), Texts::str);
            tooltip = List.of(Texts.visual(full));
            tooltipW = small.width(full) + 12;
            return;
        }
        OverlayState.Reaction r = rows.get(row).line.reactions.get(chip);
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
        if (row.day != null) {
            // The day line: quiet words in the middle, without lines, as the launcher's (CHAT-UX).
            Draw.text(g, small, row.day, Draw.snap(x0 + (w - row.dayW) / 2f),
                Fonts.centeredY(Fonts.Face.SMALL, y + DAY_H / 2f), Theme.ASH_400);
            return;
        }
        if (row.fold != null) {
            // OVL-SOC 2: someone ignored's lines, folded: quiet words where their messages would start, "Göster".
            float mid = y + ARRIVAL_H / 2f;
            Draw.text(g, small, row.arrival, x0 + 1, Fonts.centeredY(Fonts.Face.SMALL, mid), Theme.ASH_400);
            float ax = x0 + row.foldActionX;
            drawAction(g, row.foldAction, ax, y + 1, row.foldActionW, mouseX, mouseY, small);
            if (index == selected && keyboardFocus()) {
                Draw.frame(g, Frames.FOCUS_14, ax, y, row.foldActionW, 14);
            }
            return;
        }
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
            // Bone words between quiet lines, as the launcher's: gold is never a sentence (one gold per screen).
            float lw = dividerW;
            float mid = y + DIVIDER_H / 2f - 1;
            Draw.hairline(g, x0, x0 + (w - lw) / 2f - 6, mid, Theme.withAlpha(Theme.ASH_500, 0.45f));
            Draw.hairline(g, x0 + (w + lw) / 2f + 6, right, mid, Theme.withAlpha(Theme.ASH_500, 0.45f));
            Draw.text(g, small, dividerWord, x0 + (w - lw) / 2f, Fonts.centeredY(Fonts.Face.SMALL, mid),
                Theme.BONE_100);
        }
        if (row.name != null) {
            float ny = rowTop + (row.divider ? DIVIDER_H : 0);
            if (row.lead > 0) {
                // OVL-NEXT: the sender's presence dot, as of now (nothing offline).
                PresenceDot.dot(g, presenceOf.apply(row.line.by), x0 + 1 + PresenceDot.DOT / 2f, ny + SMALL_H / 2f);
            }
            if (row.chip != null) {
                row.chip.draw(g, x0 + 1 + row.lead, ny + SMALL_H / 2f);
            }
            float nameX = x0 + 1 + row.lead + TagChip.room(row.chip);
            Draw.text(g, small, row.name, nameX, Fonts.centeredY(Fonts.Face.SMALL, ny + SMALL_H / 2f), Theme.ASH_300);
            Marks.draw(g, row.marks, nameX + row.nameW, ny + SMALL_H / 2f);
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
            : row.mine ? Frames.BUBBLE_MINE : row.line.mention ? Frames.BUBBLE_MENTION : Frames.BUBBLE_THEIRS;
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
            Draw.text(g, ui, row.chipEmoji[i], cx + 4, textY, Theme.BONE_100);
            Draw.text(g, small, row.chipCount[i], cx + 4 + row.chipEmojiW[i] + 3,
                Fonts.centeredY(Fonts.Face.SMALL, cy + CHIP_H / 2f), reactionCountColor(r.mine()));
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
            Draw.icon(g, mark, 8, mx, my + 0.5f, deliveryMarkColor(row.delivery));
            mx += 11;
        }
        Draw.text(g, small, row.time, mx, textY, Theme.ASH_400);
        if (row.edited != null) {
            Draw.text(g, small, row.edited, mx + row.timeW + 4, textY, Theme.ASH_400);
        }
    }

    /**
     * The mark before the player's own message's time: a failed one's warning is flare; the clock of one on its way
     * and the tick of one sent are ash, as the time beside them and as in the launcher (BETA2-FIX: the tick was gold;
     * a check mark is never gold, docs/design.md §2).
     */
    static int deliveryMarkColor(OverlayState.Delivery delivery) {
        return delivery == OverlayState.Delivery.FAILED ? Theme.FLARE_400 : Theme.ASH_400;
    }

    /**
     * A reaction chip's count: the player's own is bone on its quiet plate ({@link Frames#REACTION_MINE}), the others
     * ash; never gold (BETA2-FIX, as the launcher's chip). The emoji itself is drawn bone on both.
     */
    static int reactionCountColor(boolean mine) {
        return mine ? Theme.BONE_50 : Theme.ASH_300;
    }

    /**
     * "Tepki ver", "Yanıtla" and, on the player's own message, "…" (CHAT3) beside the message under the pointer (or
     * picked with the keys).
     */
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
        float bx = x + TOOL_PAD;
        for (int k = 0; k < n; k++) {
            int kind = toolKind(row, k);
            boolean lit = index == hoverRow && hoverTool == kind;
            if (lit) {
                Draw.frame(g, Frames.CHIP_HOVER, bx, y + TOOL_PAD, TOOL, TOOL);
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            Icon icon = kind == TOOL_REACT ? Icon.SMILEY : kind == TOOL_REPLY ? Icon.REPLY : Icon.DOTS;
            Draw.icon(g, icon, 10, bx + 2, y + TOOL_PAD + 2, lit ? Theme.BONE_50 : Theme.ASH_300);
            bx += TOOL + TOOL_GAP;
        }
    }

    /**
     * The six quick reactions, "+" and "Yanıtla" over the message (its toolbar steps aside meanwhile); on the player's
     * own message "Düzenle" and "Sil" under them (CHAT3), "Sil" in flare as the menus' actions that end something.
     */
    private void drawStrip(GuiGraphicsExtractor g, long now, Font small) {
        if (stripRow < 0 || stripRow >= rows.size()) {
            return;
        }
        Row row = rows.get(stripRow);
        int top = topCells(row);
        int n = stripCount(row);
        if (n == 0) {
            closeStrip();
            return;
        }
        // "Düzenle" goes from the strip when its 15 minutes end while it is open: the keys stay on a cell.
        stripFocus = Math.min(stripFocus, n - 1);
        float t = Math.min(1f, (now - stripOpenedAt) / (float) STRIP_MS);
        float eased = Ease.OUT_EXPO.at(t);
        float x = stripX(row);
        float y = stripY(row);
        float w = stripW(row);
        float h = stripH(row);
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
        boolean keys = keyboardFocus();
        for (int i = 0; i < top; i++) {
            int kind = stripKind(row, i);
            float cx = x + 3 + i * QUICK_CELL;
            float cy = y + 3;
            boolean lit = i == hoverQuick || (keys && i == stripFocus);
            if (lit) {
                Draw.frame(g, Frames.CELL_HOVER, cx, cy, QUICK_CELL, QUICK_CELL);
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            if (keys && i == stripFocus) {
                Draw.frame(g, Frames.CELL_FOCUS, cx, cy, QUICK_CELL, QUICK_CELL);
            }
            if (kind >= 0 && kind < QUICK.length) {
                OverlayState.Reaction given = row.line.reaction(QUICK_SHOWN[kind]);
                if (given != null && given.mine()) {
                    // Already given: a bone dot under it (pressing it takes it back; never gold, BETA2-FIX).
                    Draw.disc(g, 3, cx + QUICK_CELL / 2f, cy + QUICK_CELL - 1f, Theme.BONE_100);
                }
                // The game's emoji glyphs are 16 texels: at twice the text size they stay whole pixels.
                m.pushMatrix();
                m.translate(cx + (QUICK_CELL - 2f * quickW[kind]) / 2f + 1f, cy + 1f);
                m.scale(2f, 2f);
                Draw.text(g, Fonts.ui(), quickLines[kind], 0, -0.5f, Theme.BONE_50);
                m.popMatrix();
            } else {
                // "+" (CHAT3: any emoji, the picker) and "Yanıtla".
                Draw.icon(g, kind == CELL_MORE ? Icon.PLUS : Icon.REPLY, 10, cx + (QUICK_CELL - 10) / 2f,
                    cy + (QUICK_CELL - 10) / 2f, lit ? Theme.BONE_50 : Theme.ASH_300);
            }
        }
        if (n > top) {
            // CHAT3: "Düzenle" and "Sil" as words, under a hairline when the reactions' row is over them.
            float ay = actionsTop(y, top);
            if (top > 0) {
                Draw.hairline(g, x + 3, x + w - 3, ay - 1, Theme.withAlpha(0xFFFFFF, 0.07f));
            }
            float ax = x + 3;
            for (int i = top; i < n; i++) {
                int kind = stripKind(row, i);
                float aw = actionW(kind);
                boolean lit = i == hoverQuick || (keys && i == stripFocus);
                if (lit) {
                    Draw.frame(g, Frames.CHIP_HOVER, ax, ay, aw, ACTION_H);
                    g.requestCursor(CursorTypes.POINTING_HAND);
                }
                if (keys && i == stripFocus) {
                    Draw.frame(g, Frames.FOCUS_14, ax, ay, aw, ACTION_H);
                }
                boolean delete = kind == CELL_DELETE;
                // SQOL-O: "Kopyala" and "Rapor Et" in the voice room's chat, quiet like "Düzenle".
                FormattedCharSequence word = switch (kind) {
                    case CELL_DELETE -> deleteWord;
                    case CELL_COPY -> copyWord;
                    case CELL_REPORT -> reportWord;
                    default -> editWord;
                };
                Draw.text(g, small, word, ax + 4, Fonts.centeredY(Fonts.Face.SMALL, ay + ACTION_H / 2f),
                    delete ? Theme.FLARE_300 : lit ? Theme.BONE_100 : Theme.ASH_300);
                ax += aw + ACTION_GAP;
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
        if (tooltipChip >= row.chips || row.line == null) {
            return;
        }
        float right = getX() + getWidth();
        float h = 6 + tooltip.size() * 10;
        float x;
        float y;
        if (tooltipChip == TIME_TIP) {
            // Over the bubble on its sender's side, under it at the list's top.
            float bx = bubbleX(row);
            float by = getY() + row.y + row.bubbleTop - scroll.value();
            x = Math.max(getX(), Math.min(right - tooltipW, row.mine ? bx + row.bubbleW - tooltipW : bx));
            y = by - h - 3;
            if (y < getY()) {
                y = by + row.bubbleH + 3;
            }
        } else {
            float rowW = chipsWidth(row);
            float cx = row.mine ? right - rowW + row.chipX[tooltipChip] : getX() + row.chipX[tooltipChip];
            float cy = getY() + row.y + row.chipsTop + row.chipY[tooltipChip] - scroll.value();
            x = Math.max(getX(), Math.min(right - tooltipW, cx + row.chipW[tooltipChip] / 2f - tooltipW / 2f));
            y = cy - h - 3;
            if (y < getY()) {
                y = cy + CHIP_H + 3;
            }
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
