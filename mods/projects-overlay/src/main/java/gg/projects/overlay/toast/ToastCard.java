package gg.projects.overlay.toast;

import gg.projects.overlay.keys.KeyHint;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Marks;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.PixelSprites;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * One toast, laid out once when it arrives (or updates) and only drawn afterwards: a face on the left (a
 * name's initial, or the crew, call, invite or info sign), the name in bold with its count, the crew's name
 * under it for crew chat, up to two lines of text, and the hint with the key that opens the overlay. A
 * travel toast ("Projects sunucusuna bağlanılıyor…") is one sentence beside the gold join sign. A sticker
 * message (CHAT2) shows the sticker small before its words ("Çıkartma · GG"), unless message text is hidden. A
 * message's sender who wears a crew tag has its chip before the name ("[GK] Ayşe", TEAM6). A friend's world invite
 * (HOST1) has the grass block for its face and "Katılmak için F7 tuşuna bas"; the host's own notes ("Dünyan
 * arkadaşlarına açık", "Mert dünyana katıldı") are the overlay's, under the player's "Diğer bildirimler". The person a
 * title names (a message's sender, the friend of an invite or a world invite) has their first profile badge right of
 * the name (BADGES-UI, {@link Marks}); the narrator says all of them with the name. A voice chat ring (VOICE-O) is the
 * caller's face, "Kaan_07 arıyor" (a long name gives way, never the verb), who else is in the room, and "Yanıtlamak
 * için [F7] tuşuna bas"; it stands under the call's gold hairline for as long as it rings.
 *
 * <p>The hint is one line that says to press a key (F7-OPEN: "F7 ile arayüzü aç" read as a button, and players clicked
 * it): "Açmak için [F7] tuşuna bas", or where this computer gives the key only with Fn, "Açmak için [Fn + F7] tuşlarına
 * bas" (KEY-HINT, {@link KeyHint}); a line too wide for the card in the current letters (the game's pixel font at GUI
 * scale 1) says the keys alone ("[Fn + F7] tuşlarına bas"). None while "Açma ipucunu göster" is off.
 */
public final class ToastCard {
    public static final int WIDTH = Frames.TOAST_WIDTH;
    static final int PAD_X = 8;
    static final int PAD_Y = 7;
    static final int FACE = 18;
    static final int TEXT_X = PAD_X + FACE + 7;
    static final int TEXT_W = WIDTH - TEXT_X - PAD_X;
    static final int TITLE_H = 10;
    static final int LINE_H = 10;
    static final int SMALL_H = 9;
    static final int HINT_GAP = 4;
    static final int HINT_H = 10;
    static final int MAX_BODY_LINES = 2;
    /** A sticker's picture in a toast, GUI pixels (CHAT2 §8.3). */
    static final int STICKER = 16;
    static final int KEY_PAD = 3;
    /** Between a hint's words and its key cap. */
    static final int HINT_SPACE = 3;
    /** A key's name on its cap: at most this wide, and never cut below the shortest room (GUI pixels). */
    static final int MAX_KEY_NAME = 48;
    static final int MIN_KEY_NAME = 16;
    /** The key the Fn line names: the same word on every keyboard and in both languages. */
    static final String FN = "Fn";
    /** The hint's sentence when its full one is too wide for the card: the keys alone ("[F7] tuşuna bas"). */
    static final String SHORT_HINT = "toast.hint.short";
    /** The ending of a hint sentence's Fn form ("toast.hint.open.fn": "Açmak için %s tuşlarına bas"). */
    static final String FN_FORM = ".fn";

    public enum Kind { MESSAGE, CALL, INVITE, INFO, TRAVEL, WORLD, HOST, RING }

    final Kind kind;
    final String conversation;
    final boolean crewChat;
    /** The sender's face (the launcher's player head with their letter; OVL-UX2), the crew sign for crew chat. */
    final Face face;
    final FormattedCharSequence title;
    final int titleWidth;
    /** TEAM6: the sender's crew tag before the name, or null. */
    final TagChip chip;
    /** BADGES-UI: the profile badges of the person the title names; the first is drawn right of the name. */
    final List<OverlayState.MarkDef> marks;
    final String countText;
    final int countWidth;
    final FormattedCharSequence crew;
    final List<FormattedCharSequence> body;
    /** A sticker message's sticker, drawn small before the words. */
    final StickerSet.Sticker sticker;
    /** The hint under the text ("Açmak için [F7] tuşuna bas"), or null: none (an info card, the hint turned off). */
    final Hint hint;
    /** What the hint says, its sentence and the key it names: enough to say the Fn line instead later (KEY-HINT). */
    final KeyHint.Line line;
    final String hintTemplate;
    final String keyName;
    /** The face is the world's grass block (a world invite, the world opening) rather than a disc. */
    final boolean sprite;
    /** A travel card that only tells something (OVL-FIX): the quiet info sign instead of the gold join sign. */
    final boolean info;
    // The short strings again, in the form the game draws: made once here, not on every frame (each String
    // draw runs the game's bidirectional text pass).
    private final FormattedCharSequence countLine;
    final float height;
    final Component narration;

    private ToastCard(Builder b) {
        this.kind = b.kind;
        this.conversation = b.conversation;
        this.crewChat = b.crewChat;
        this.face = b.crewChat ? Face.sign(Icon.CREW)
            : b.initial == null || b.initial.isEmpty() ? null : Face.of(b.initial, b.by);
        this.title = b.title;
        this.titleWidth = b.titleWidth;
        this.chip = b.chip;
        this.marks = b.marks;
        this.countText = b.countText;
        this.countWidth = b.countWidth;
        this.crew = b.crew;
        this.body = b.body;
        this.sticker = b.sticker;
        this.hint = b.hint;
        this.line = b.line;
        this.hintTemplate = b.hintTemplate;
        this.keyName = b.keyName;
        this.sprite = b.sprite;
        this.info = b.info;
        this.narration = b.narration;
        this.countLine = line(b.countText);
        if (kind == Kind.TRAVEL) {
            this.height = PAD_Y + Math.max(FACE, body.size() * LINE_H) + PAD_Y;
            return;
        }
        float h = PAD_Y + TITLE_H + (crew != null ? SMALL_H : 0) + 1
            + (sticker != null ? Math.max(STICKER + 1, body.size() * LINE_H) : body.size() * LINE_H);
        if (hint != null) {
            h += HINT_GAP + HINT_H;
        }
        this.height = h + PAD_Y;
    }

    /** {@code o} with another key hint of one line (the height stays). */
    private ToastCard(ToastCard o, Hint hint, KeyHint.Line line) {
        this.kind = o.kind;
        this.conversation = o.conversation;
        this.crewChat = o.crewChat;
        this.face = o.face;
        this.title = o.title;
        this.titleWidth = o.titleWidth;
        this.chip = o.chip;
        this.marks = o.marks;
        this.countText = o.countText;
        this.countWidth = o.countWidth;
        this.crew = o.crew;
        this.body = o.body;
        this.sticker = o.sticker;
        this.hint = hint;
        this.line = line;
        this.hintTemplate = o.hintTemplate;
        this.keyName = o.keyName;
        this.sprite = o.sprite;
        this.info = o.info;
        this.countLine = o.countLine;
        this.height = o.height;
        this.narration = o.narration;
    }

    private static FormattedCharSequence line(String text) {
        return text == null || text.isEmpty() ? null : Texts.visual(text);
    }

    /**
     * The width a key's name may take on its cap in a hint line whose other parts take {@code others} (its words, the
     * other caps, the spaces between the parts and this cap's padding), so the line stays inside the card: at most
     * {@link #MAX_KEY_NAME}, never under {@link #MIN_KEY_NAME}.
     */
    static int keyRoom(int lineWidth, int others) {
        return Math.max(MIN_KEY_NAME, Math.min(MAX_KEY_NAME, lineWidth - others));
    }

    /**
     * The words of a hint sentence formatted with the NUL character where each key goes, each stripped: one more than
     * the keys (an empty one where a key starts or ends the sentence).
     */
    static List<String> split(String formatted) {
        List<String> words = new ArrayList<>(3);
        int from = 0;
        for (int at = formatted.indexOf('\u0000'); at >= 0; at = formatted.indexOf('\u0000', from)) {
            words.add(formatted.substring(from, at).strip());
            from = at + 1;
        }
        words.add(formatted.substring(from).strip());
        return words;
    }

    /**
     * How wide a hint line is: its words' widths in order (0 for an empty one) with a cap after each but the last
     * ({@code caps}, padding included), and {@link #HINT_SPACE} between the parts that are there.
     */
    static int lineWidth(int[] words, int[] caps) {
        int width = 0;
        int parts = 0;
        for (int i = 0; i < Math.max(words.length, caps.length); i++) {
            if (i < words.length && words[i] > 0) {
                width += words[i];
                parts++;
            }
            if (i < caps.length) {
                width += caps[i];
                parts++;
            }
        }
        return width + Math.max(0, parts - 1) * HINT_SPACE;
    }

    /**
     * One hint line, laid out once: words with key caps among them ("Açmak için [F7] tuşuna bas", "Açmak için [Fn + F7]
     * tuşlarına bas"), or words alone.
     */
    static final class Hint {
        /** The words, one more than the keys: before the first key, between two, after the last; null where none. */
        private final FormattedCharSequence[] words;
        private final int[] wordWidths;
        private final FormattedCharSequence[] keys;
        /** Each cap's width, its padding included. */
        private final int[] capWidths;
        /** The whole line's width in the letters it was laid out in. */
        final int width;
        /** A key's name had to be cut to keep the line inside the card. */
        boolean cut;

        private Hint(List<String> words, List<String> keys) {
            Font small = Fonts.small();
            this.words = new FormattedCharSequence[words.size()];
            this.wordWidths = new int[words.size()];
            for (int i = 0; i < words.size(); i++) {
                this.words[i] = line(words.get(i));
                this.wordWidths[i] = words.get(i).isEmpty() ? 0 : small.width(words.get(i));
            }
            this.keys = new FormattedCharSequence[keys.size()];
            this.capWidths = new int[keys.size()];
            for (int i = 0; i < keys.size(); i++) {
                this.keys[i] = line(keys.get(i));
                this.capWidths[i] = small.width(keys.get(i)) + 2 * KEY_PAD;
            }
            this.width = lineWidth(wordWidths, capWidths);
        }

        /**
         * The sentence {@code template} (its words' key) with each of {@code keyNames} on a cap in turn, each name cut
         * to the room the rest of the line leaves it.
         */
        static Hint of(String template, String... keyNames) {
            Object[] marks = new Object[keyNames.length];
            Arrays.fill(marks, "\u0000");
            List<String> words = split(Texts.str(template, marks));
            while (words.size() < keyNames.length + 1) {
                // A sentence without a slot for every key: the caps follow its words.
                words.add("");
            }
            Font small = Fonts.small();
            int[] wordWidths = new int[words.size()];
            for (int i = 0; i < wordWidths.length; i++) {
                wordWidths[i] = words.get(i).isEmpty() ? 0 : small.width(words.get(i));
            }
            int[] caps = new int[keyNames.length];
            for (int i = 0; i < caps.length; i++) {
                caps[i] = small.width(keyNames[i]) + 2 * KEY_PAD;
            }
            int line = lineWidth(wordWidths, caps);
            List<String> names = new ArrayList<>(keyNames.length);
            boolean cut = false;
            for (int i = 0; i < keyNames.length; i++) {
                int others = line - (caps[i] - 2 * KEY_PAD);
                String name = Texts.fit(small, keyNames[i], keyRoom(TEXT_W, others));
                cut |= !name.equals(keyNames[i]);
                names.add(name);
            }
            Hint hint = new Hint(words, names);
            hint.cut = cut;
            return hint;
        }

        /** Words without a key ("Açmak için Kontroller'den bir tuş ata"). */
        static Hint plain(String text) {
            return new Hint(List.of(text), List.of());
        }

        void draw(GuiGraphicsExtractor g, float x, float y) {
            Font small = Fonts.small();
            float textY = Fonts.centeredY(Fonts.Face.SMALL, y + HINT_H / 2f);
            float cx = x;
            boolean first = true;
            for (int i = 0; i < Math.max(words.length, keys.length); i++) {
                if (i < words.length && words[i] != null) {
                    cx += first ? 0 : HINT_SPACE;
                    Draw.text(g, small, words[i], cx, textY, Theme.ASH_300);
                    cx += wordWidths[i];
                    first = false;
                }
                if (i < keys.length) {
                    cx += first ? 0 : HINT_SPACE;
                    Draw.frame(g, Frames.KEYCAP, cx, y, capWidths[i], HINT_H);
                    Draw.text(g, small, keys[i], cx + KEY_PAD, textY - 0.5f, Theme.BONE_100);
                    cx += capWidths[i];
                    first = false;
                }
            }
        }
    }

    private static final class Builder {
        Kind kind;
        String conversation;
        boolean crewChat;
        String initial;
        /** OVL-AVATARS: whose picture the face shows (a user id), or null for the letter. */
        String by;
        FormattedCharSequence title;
        int titleWidth;
        TagChip chip;
        List<OverlayState.MarkDef> marks = List.of();
        String countText;
        int countWidth;
        FormattedCharSequence crew;
        List<FormattedCharSequence> body;
        StickerSet.Sticker sticker;
        Hint hint;
        KeyHint.Line line = KeyHint.Line.NONE;
        String hintTemplate;
        String keyName;
        boolean sprite;
        boolean info;
        Component narration;
    }

    public static Kind kindOf(String kind) {
        return switch (kind) {
            case "call" -> Kind.CALL;
            case "invite" -> Kind.INVITE;
            case "info" -> Kind.INFO;
            case "world" -> Kind.WORLD;
            default -> Kind.MESSAGE;
        };
    }

    /**
     * Lays out a toast.
     *
     * @param crewChat the message comes from a crew conversation
     * @param count how many messages it stands for (shown from 2 on)
     * @param showText the player's "Mesaj metnini göster"
     * @param keyName the overlay key's name, or null when none is bound
     * @param line what the key hint says on this computer (KEY-HINT): the key, Fn and the key, or nothing
     */
    public static ToastCard of(OverlayState.ToastRequest request, boolean crewChat, int count, boolean showText,
        String keyName, KeyHint.Line line) {
        Builder b = new Builder();
        b.kind = kindOf(request.kind());
        b.conversation = request.conversation();
        b.crewChat = crewChat;
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        String someone = Texts.str("toast.someone");
        String name = request.title().isEmpty() ? someone : request.title();
        String bodyText = switch (b.kind) {
            case CALL -> Texts.str("toast.call", request.body().isEmpty() ? someone : request.body());
            case INVITE -> Texts.str("toast.invite");
            case WORLD -> Texts.str("toast.world");
            case INFO, TRAVEL, HOST, RING -> request.body();
            case MESSAGE -> showText && !request.body().isEmpty() ? request.body() : Texts.str("toast.new_message");
        };
        if (count > 1) {
            b.countText = count > 99 ? "99+" : Integer.toString(count);
            b.countWidth = small.width(b.countText) + 8;
        }
        if (b.kind == Kind.MESSAGE) {
            b.chip = TagChip.of(request.badge(), TagChip.ROW);
        }
        if ((b.kind == Kind.MESSAGE || b.kind == Kind.INVITE || b.kind == Kind.WORLD) && request.marks() != null) {
            // The title names a person: their first badge right of the name (BADGES-UI).
            b.marks = request.marks();
        }
        int titleRoom = Math.round(TEXT_W - (b.countText == null ? 0 : b.countWidth + 4) - TagChip.room(b.chip)
            - Marks.room(b.marks));
        String titleText = Texts.fit(strong, name, titleRoom);
        b.title = Texts.visual(titleText);
        b.titleWidth = strong.width(titleText);
        b.initial = Texts.initial(name);
        b.by = request.by();
        String crewName = request.crew() == null || request.crew().isEmpty() ? null : request.crew();
        if (b.kind == Kind.MESSAGE && crewName != null) {
            b.crew = Texts.visual(Texts.fit(small, crewName, TEXT_W));
        }
        if (b.kind == Kind.MESSAGE && showText && request.sticker() != null) {
            b.sticker = StickerSet.get().find(request.sticker());
        }
        if (b.sticker != null) {
            StickerArt.prepare(STICKER);
            b.body = Texts.wrap(Fonts.ui(), bodyText, TEXT_W - STICKER - 5, 1);
        } else {
            b.body = Texts.wrap(Fonts.ui(), bodyText, TEXT_W, MAX_BODY_LINES);
        }
        b.sprite = b.kind == Kind.WORLD;
        if (b.kind != Kind.INFO) {
            hint(b, b.kind == Kind.CALL ? "toast.hint.call" : b.kind == Kind.WORLD ? "toast.hint.join"
                : "toast.hint.open", keyName, line);
        }
        // The chip and the badge are drawn; the narrator says them with the name ("Ayşe, Projects yöneticisi, etiket
        // GK").
        String marked = Marks.spoken(name, b.marks, Texts.current());
        String spokenName = b.chip == null ? marked : Texts.str("row.badge", marked, b.chip.badge.tag());
        b.narration = switch (b.kind) {
            case CALL -> Texts.tr("toast.narration.notice", name, bodyText);
            case INVITE, WORLD -> Texts.tr("toast.narration.notice", marked, bodyText);
            case INFO, TRAVEL, HOST, RING -> Component.literal(name + ". " + bodyText);
            case MESSAGE -> crewName != null ? Texts.tr("toast.narration.crew", spokenName, crewName, bodyText)
                : Texts.tr("toast.narration.message", spokenName, bodyText);
        };
        return new ToastCard(b);
    }

    /** The card's key hint ({@code template}: its sentence, "toast.hint.open"), as {@link #hintOf} lays it out. */
    private static void hint(Builder b, String template, String keyName, KeyHint.Line line) {
        b.hintTemplate = template;
        b.keyName = keyName;
        b.line = line;
        b.hint = hintOf(template, keyName, line);
    }

    /**
     * The key hint, one line: the overlay's key where the sentence has it ("Açmak için [F7] tuşuna bas", "Press [F7] to
     * open"); for {@link KeyHint.Line#FN} Fn on the same cap, as laptop makers print it, in the sentence's Fn form
     * ("Açmak için [Fn + F7] tuşlarına bas"); the keys alone ("[Fn + F7] tuşlarına bas") when the sentence is wider than
     * the card in the current letters; with no key bound, where to bind one; null for {@link KeyHint.Line#NONE}.
     */
    static Hint hintOf(String template, String keyName, KeyHint.Line line) {
        if (line == KeyHint.Line.NONE) {
            return null;
        }
        if (keyName == null) {
            return Hint.plain(Texts.str("toast.hint.unbound"));
        }
        boolean fn = line == KeyHint.Line.FN;
        String cap = fn ? fnCap(keyName) : keyName;
        Hint full = Hint.of(fn ? template + FN_FORM : template, cap);
        // The whole sentence with the key's whole name, or the keys alone (a cut "F…" names no key).
        return !full.cut && full.width <= TEXT_W ? full : Hint.of(fn ? SHORT_HINT + FN_FORM : SHORT_HINT, cap);
    }

    /** Fn and the key on one cap ("Fn + F7"). */
    static String fnCap(String keyName) {
        return FN + " + " + keyName;
    }

    /**
     * This card with its key hint saying {@code next} (KEY-HINT: the computer turned out to need Fn while the card was
     * on screen): a copy of the same height (one line for another), or this card when its hint stays as it is.
     */
    public ToastCard rehinted(KeyHint.Line next) {
        if (hint == null || keyName == null || hintTemplate == null || next == line || next == KeyHint.Line.NONE) {
            return this;
        }
        return new ToastCard(this, hintOf(hintTemplate, keyName, next), next);
    }

    /** Whether the card shows the overlay key's hint (a key on its cap). */
    public boolean keyHint() {
        return hint != null && keyName != null;
    }

    /** What the card's key hint says (the dev scenarios'). */
    public KeyHint.Line line() {
        return line;
    }

    /** The hint line's width as laid out, 0 without one (the dev scenarios check it stays inside the card). */
    public int hintWidth() {
        return hint == null ? 0 : hint.width;
    }

    /** Whether the hint line stays inside the card's text column (or there is none). */
    public boolean hintFits() {
        return hint == null || hint.width <= TEXT_W;
    }

    /**
     * The host's own note (HOST1): {@code title} in bold with {@code body} under it, the grass block for its face when
     * it is about the world ({@code sprite}) or the friend's initial when it is about them; with {@code hintKey} the
     * hint line ("Yönetmek için [F7] tuşuna bas") follows as {@code line} says.
     */
    public static ToastCard host(String title, String body, boolean sprite, String hintKey, String keyName,
        KeyHint.Line line) {
        Builder b = new Builder();
        b.kind = Kind.HOST;
        b.sprite = sprite;
        Font strong = Fonts.strong();
        String titleText = Texts.fit(strong, title, TEXT_W);
        b.title = Texts.visual(titleText);
        b.titleWidth = strong.width(titleText);
        b.initial = Texts.initial(title);
        b.body = body == null || body.isEmpty() ? List.of() : Texts.wrap(Fonts.ui(), body, TEXT_W, MAX_BODY_LINES);
        if (hintKey != null) {
            hint(b, hintKey, keyName, line);
        }
        b.narration = Component.literal(body == null || body.isEmpty() ? title : title + ". " + body);
        return new ToastCard(b);
    }

    /**
     * A voice chat ring (VOICE-O): the caller's face, "{@code name} arıyor" in bold (the name gives way to the verb), who
     * else is in the room under it ("Odada: Ece_x, Mert"; else "Sesli sohbet"), and the hint to answer in the panel
     * ("Yanıtlamak için [F7] tuşuna bas", as {@code line} says). {@code from}: the caller's user id, for their picture.
     */
    public static ToastCard ring(String name, String from, String others, String keyName, KeyHint.Line line) {
        Builder b = new Builder();
        b.kind = Kind.RING;
        Font strong = Fonts.strong();
        String who = name == null || name.isEmpty() ? Texts.str("voice.someone") : name;
        String verb = Texts.str("voice.ring.verb");
        int verbRoom = strong.width(" " + verb);
        String titleText = Texts.fit(strong, who, Math.max(24, TEXT_W - verbRoom)) + " " + verb;
        b.title = Texts.visual(titleText);
        b.titleWidth = strong.width(titleText);
        b.initial = Texts.initial(who);
        b.by = from;
        String room = others == null || others.isEmpty() ? Texts.str("voice.title")
            : Texts.str("voice.ring.others", others);
        b.body = Texts.wrap(Fonts.ui(), room, TEXT_W, 1);
        hint(b, "toast.hint.call", keyName, line);
        b.narration = Component.literal(Texts.str("voice.ring.calling", who) + ". " + room);
        return new ToastCard(b);
    }

    /**
     * A note in the travel toast's form that goes nowhere (OVL-FIX: the "Devam et" world is gone): one sentence beside the
     * quiet info sign.
     */
    public static ToastCard note(String text) {
        Builder b = new Builder();
        b.kind = Kind.TRAVEL;
        b.info = true;
        b.initial = "";
        b.body = Texts.wrap(Fonts.ui(), text, TEXT_W, MAX_BODY_LINES);
        b.narration = Component.literal(text);
        return new ToastCard(b);
    }

    /** A travel toast: one sentence (up to two lines) beside the gold join sign, no title, no hint. */
    public static ToastCard travel(String text) {
        Builder b = new Builder();
        b.kind = Kind.TRAVEL;
        b.initial = "";
        b.body = Texts.wrap(Fonts.ui(), text, TEXT_W, MAX_BODY_LINES);
        b.narration = Component.literal(text);
        return new ToastCard(b);
    }

    public float height() {
        return height;
    }

    public Component narration() {
        return narration;
    }

    public String conversation() {
        return conversation;
    }

    public Kind kind() {
        return kind;
    }

    /** BADGES-UI: the badges of the person the title names (the tooltip of the one drawn names them all). */
    public List<OverlayState.MarkDef> marks() {
        return marks;
    }

    /** Where the title's name ends, from the card's left edge (its badge stands {@link Marks#GAP} right of it). */
    float nameEnd() {
        return TEXT_X + TagChip.room(chip) + titleWidth;
    }

    /** The middle of the title's line, from the card's top (its badge is centred on it). */
    static float titleMid() {
        return PAD_Y + TITLE_H / 2f;
    }

    /** Draws the card with its top-left corner at {@code x, y}; the caller set the opacity. */
    void draw(GuiGraphicsExtractor g, float x, float y, float refresh) {
        if (kind == Kind.TRAVEL) {
            drawTravel(g, x, y);
            return;
        }
        Draw.frame(g, kind == Kind.CALL || kind == Kind.RING ? Frames.TOAST_CALL : Frames.TOAST, x, y, WIDTH, height);
        drawFace(g, x + PAD_X + FACE / 2f, y + PAD_Y + FACE / 2f);
        float tx = x + TEXT_X;
        float ty = y + PAD_Y;
        if (chip != null) {
            chip.draw(g, tx, ty + TITLE_H / 2f);
        }
        float nameX = tx + TagChip.room(chip);
        Draw.text(g, Fonts.strong(), title, nameX, Fonts.centeredY(Fonts.Face.STRONG, ty + TITLE_H / 2f),
            Theme.BONE_50);
        Marks.draw(g, marks, nameX + titleWidth, ty + TITLE_H / 2f);
        if (countText != null) {
            float px = nameX + titleWidth + Marks.room(marks) + 4;
            float py = ty + (TITLE_H - 9) / 2f;
            Draw.frame(g, Frames.PILL, px, py, countWidth, 9);
            Draw.text(g, Fonts.small(), countLine, px + 4, Fonts.centeredY(Fonts.Face.SMALL, py + 4.5f),
                Theme.BONE_100);
        }
        ty += TITLE_H;
        if (crew != null) {
            Draw.text(g, Fonts.small(), crew, tx, Fonts.centeredY(Fonts.Face.SMALL, ty + SMALL_H / 2f - 0.5f),
                Theme.ASH_300);
            ty += SMALL_H;
        }
        ty += 1;
        // An update in place: the text comes up from dim, so the eye sees it changed.
        Draw.pushAlpha(0.45f + 0.55f * refresh);
        if (sticker != null) {
            if (!StickerArt.draw(g, sticker, STICKER, Draw.snap(tx), Draw.snap(ty))) {
                Draw.frame(g, Frames.STICKER_TILE, tx, ty, STICKER, STICKER);
            }
            for (FormattedCharSequence line : body) {
                Draw.text(g, Fonts.ui(), line, tx + STICKER + 5, Fonts.centeredY(Fonts.Face.UI, ty + STICKER / 2f),
                    Theme.BONE_300);
            }
            ty += STICKER + 1;
        } else {
            for (FormattedCharSequence line : body) {
                Draw.text(g, Fonts.ui(), line, tx, Fonts.centeredY(Fonts.Face.UI, ty + LINE_H / 2f), Theme.BONE_300);
                ty += LINE_H;
            }
        }
        Draw.popAlpha();
        if (hint != null) {
            hint.draw(g, tx, ty + HINT_GAP);
        }
    }

    private void drawTravel(GuiGraphicsExtractor g, float x, float y) {
        Draw.frame(g, Frames.TOAST, x, y, WIDTH, height);
        float mid = y + height / 2f;
        drawFace(g, x + PAD_X + FACE / 2f, mid);
        float ty = mid - body.size() * LINE_H / 2f;
        for (FormattedCharSequence line : body) {
            Draw.text(g, Fonts.ui(), line, x + TEXT_X, Fonts.centeredY(Fonts.Face.UI, ty + LINE_H / 2f),
                Theme.BONE_100);
            ty += LINE_H;
        }
    }

    private void drawFace(GuiGraphicsExtractor g, float cx, float cy) {
        if (sprite) {
            // Our grass block on whole pixels: a world is a block, never a globe (ICON1).
            PixelSprites.draw(g, PixelSprites.Sprite.WORLD, cx - PixelSprites.SIZE / 2f, cy - PixelSprites.SIZE / 2f);
            return;
        }
        switch (kind) {
            case TRAVEL -> {
                if (info) {
                    Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_700);
                    Draw.icon(g, Icon.INFO, 10, cx - 5, cy - 5, Theme.ASH_300);
                } else {
                    Draw.disc(g, FACE, cx, cy, Theme.withAlpha(Theme.GOLD_500, 0.2f));
                    Draw.icon(g, Icon.JOIN, 10, cx - 5, cy - 5, Theme.GOLD_300);
                }
            }
            case CALL -> {
                Draw.disc(g, FACE, cx, cy, Theme.withAlpha(Theme.GOLD_500, 0.2f));
                Draw.icon(g, Icon.CALL, 10, cx - 5, cy - 5, Theme.GOLD_300);
            }
            case INVITE -> {
                Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_700);
                Draw.icon(g, Icon.INVITE, 10, cx - 5, cy - 5, Theme.BONE_100);
            }
            case INFO -> {
                Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_700);
                Draw.icon(g, Icon.INFO, 10, cx - 5, cy - 5, Theme.ASH_300);
            }
            case MESSAGE, HOST, WORLD, RING -> {
                if (face != null) {
                    face.draw(g, FACE, cx, cy);
                } else {
                    Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_600);
                }
            }
        }
    }

}
