package gg.projects.overlay.toast;

import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.PixelSprites;
import java.util.ArrayList;
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
 * arkadaşlarına açık", "Mert dünyana katıldı") are the overlay's, under the player's "Diğer bildirimler".
 *
 * <p>The hint says to press a key (F7-OPEN: "F7 ile arayüzü aç" read as a button, and players clicked it), and where the
 * key may need Fn a second line says so ("Olmazsa [Fn] ile birlikte bas", {@link gg.projects.overlay.keys.FnTip}).
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
    /** Between two hint lines: their key caps stand clear of each other. */
    static final int HINT_LINE_GAP = 2;
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

    public enum Kind { MESSAGE, CALL, INVITE, INFO, TRAVEL, WORLD, HOST }

    final Kind kind;
    final String conversation;
    final boolean crewChat;
    /** The sender's face (the launcher's player head with their letter; OVL-UX2), the crew sign for crew chat. */
    final Face face;
    final FormattedCharSequence title;
    final int titleWidth;
    /** TEAM6: the sender's crew tag before the name, or null. */
    final TagChip chip;
    final String countText;
    final int countWidth;
    final FormattedCharSequence crew;
    final List<FormattedCharSequence> body;
    /** A sticker message's sticker, drawn small before the words. */
    final StickerSet.Sticker sticker;
    /** The hint lines under the text, top first ("Açmak için [F7] tuşuna bas", then maybe the Fn line); maybe none. */
    final List<Hint> hints;
    /** The hints end with "Olmazsa [Fn] ile birlikte bas" (F7-OPEN). */
    final boolean fnTip;
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
            : b.initial == null || b.initial.isEmpty() ? null : Face.of(b.initial);
        this.title = b.title;
        this.titleWidth = b.titleWidth;
        this.chip = b.chip;
        this.countText = b.countText;
        this.countWidth = b.countWidth;
        this.crew = b.crew;
        this.body = b.body;
        this.sticker = b.sticker;
        this.hints = List.copyOf(b.hints);
        this.fnTip = b.fnTip;
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
        if (!hints.isEmpty()) {
            h += HINT_GAP + hintsHeight(hints.size());
        }
        this.height = h + PAD_Y;
    }

    /** The height of {@code lines} hint lines, one under the other. */
    static int hintsHeight(int lines) {
        return lines <= 0 ? 0 : lines * HINT_H + (lines - 1) * HINT_LINE_GAP;
    }

    private static FormattedCharSequence line(String text) {
        return text == null || text.isEmpty() ? null : Texts.visual(text);
    }

    /**
     * The width a key's name may take on its cap in a hint line whose words around it are this wide (0: none on that
     * side), so the line stays inside the card: at most {@link #MAX_KEY_NAME}, never under {@link #MIN_KEY_NAME}.
     */
    static int keyRoom(int lineWidth, int beforeWidth, int afterWidth) {
        int room = lineWidth - 2 * KEY_PAD - (beforeWidth > 0 ? beforeWidth + HINT_SPACE : 0)
            - (afterWidth > 0 ? afterWidth + HINT_SPACE : 0);
        return Math.max(MIN_KEY_NAME, Math.min(MAX_KEY_NAME, room));
    }

    /**
     * The words of a hint sentence formatted with the NUL character where the key goes: {before, after}, each stripped
     * ({@code after} empty when the sentence has no key).
     */
    static String[] around(String formatted) {
        int at = formatted.indexOf('\u0000');
        return at < 0 ? new String[] {formatted.strip(), ""}
            : new String[] {formatted.substring(0, at).strip(), formatted.substring(at + 1).strip()};
    }

    /** One hint line, laid out once: words around a key cap ("Açmak için [F7] tuşuna bas"), or words alone. */
    static final class Hint {
        final String before;
        final String key;
        final String after;
        final int beforeWidth;
        final int keyWidth;
        private final FormattedCharSequence beforeLine;
        private final FormattedCharSequence keyLine;
        private final FormattedCharSequence afterLine;

        private Hint(String before, String key, String after) {
            Font small = Fonts.small();
            this.before = before;
            this.key = key;
            this.after = after;
            this.beforeWidth = before.isEmpty() ? 0 : small.width(before);
            this.keyWidth = key == null ? 0 : small.width(key) + 2 * KEY_PAD;
            this.beforeLine = line(before);
            this.keyLine = line(key);
            this.afterLine = line(after);
        }

        /** The sentence {@code template} (its words' key) with {@code keyName} on a cap, cut to the room it leaves. */
        static Hint of(String template, String keyName) {
            String[] words = around(Texts.str(template, "\u0000"));
            Font small = Fonts.small();
            int room = keyRoom(TEXT_W, words[0].isEmpty() ? 0 : small.width(words[0]),
                words[1].isEmpty() ? 0 : small.width(words[1]));
            return new Hint(words[0], Texts.fit(small, keyName, room), words[1]);
        }

        /** Words without a key ("Kontroller'den bir tuş ata"). */
        static Hint words(String text) {
            return new Hint(text, null, "");
        }

        void draw(GuiGraphicsExtractor g, float x, float y) {
            Font small = Fonts.small();
            float textY = Fonts.centeredY(Fonts.Face.SMALL, y + HINT_H / 2f);
            float cx = x;
            if (beforeLine != null) {
                Draw.text(g, small, beforeLine, cx, textY, Theme.ASH_300);
                cx += beforeWidth + HINT_SPACE;
            }
            if (keyLine == null) {
                return;
            }
            Draw.frame(g, Frames.KEYCAP, cx, y, keyWidth, HINT_H);
            Draw.text(g, small, keyLine, cx + KEY_PAD, textY - 0.5f, Theme.BONE_100);
            cx += keyWidth + HINT_SPACE;
            if (afterLine != null) {
                Draw.text(g, small, afterLine, cx, textY, Theme.ASH_300);
            }
        }
    }

    private static final class Builder {
        Kind kind;
        String conversation;
        boolean crewChat;
        String initial;
        FormattedCharSequence title;
        int titleWidth;
        TagChip chip;
        String countText;
        int countWidth;
        FormattedCharSequence crew;
        List<FormattedCharSequence> body;
        StickerSet.Sticker sticker;
        final List<Hint> hints = new ArrayList<>(2);
        boolean fnTip;
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
     * @param fnTip the key hint gets "Olmazsa [Fn] ile birlikte bas" under it (F7-OPEN)
     */
    public static ToastCard of(OverlayState.ToastRequest request, boolean crewChat, int count, boolean showText,
        String keyName, boolean fnTip) {
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
            case INFO, TRAVEL, HOST -> request.body();
            case MESSAGE -> showText && !request.body().isEmpty() ? request.body() : Texts.str("toast.new_message");
        };
        if (count > 1) {
            b.countText = count > 99 ? "99+" : Integer.toString(count);
            b.countWidth = small.width(b.countText) + 8;
        }
        if (b.kind == Kind.MESSAGE) {
            b.chip = TagChip.of(request.badge(), TagChip.ROW);
        }
        int titleRoom = Math.round(TEXT_W - (b.countText == null ? 0 : b.countWidth + 4) - TagChip.room(b.chip));
        String titleText = Texts.fit(strong, name, titleRoom);
        b.title = Texts.visual(titleText);
        b.titleWidth = strong.width(titleText);
        b.initial = Texts.initial(name);
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
                : "toast.hint.open", keyName, fnTip);
        }
        // The chip is drawn; the narrator says it with the name ("Ayşe, etiket GK").
        String spokenName = b.chip == null ? name : Texts.str("row.badge", name, b.chip.badge.tag());
        b.narration = switch (b.kind) {
            case CALL, INVITE, WORLD -> Texts.tr("toast.narration.notice", name, bodyText);
            case INFO, TRAVEL, HOST -> Component.literal(name + ". " + bodyText);
            case MESSAGE -> crewName != null ? Texts.tr("toast.narration.crew", spokenName, crewName, bodyText)
                : Texts.tr("toast.narration.message", spokenName, bodyText);
        };
        return new ToastCard(b);
    }

    /**
     * The hint with the overlay's key where the sentence has it ("Açmak için [F7] tuşuna bas", "Press [F7] to open"),
     * with {@code fnTip} the Fn line under it; with no key bound, where to bind one.
     */
    private static void hint(Builder b, String key, String keyName, boolean fnTip) {
        if (keyName == null) {
            b.hints.add(Hint.words(Texts.str("toast.hint.unbound")));
            return;
        }
        b.hints.add(Hint.of(key, keyName));
        if (fnTip) {
            b.hints.add(Hint.of("toast.hint.fn", FN));
            b.fnTip = true;
        }
    }

    /**
     * The host's own note (HOST1): {@code title} in bold with {@code body} under it, the grass block for its face when
     * it is about the world ({@code sprite}) or the friend's initial when it is about them; with {@code hintKey} the
     * hint line ("Yönetmek için [F7] tuşuna bas") follows, and with {@code fnTip} the Fn line.
     */
    public static ToastCard host(String title, String body, boolean sprite, String hintKey, String keyName,
        boolean fnTip) {
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
            hint(b, hintKey, keyName, fnTip);
        }
        b.narration = Component.literal(body == null || body.isEmpty() ? title : title + ". " + body);
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

    /** Whether the card carries "Olmazsa [Fn] ile birlikte bas" (F7-OPEN: a game shows it a few times only). */
    public boolean fnTip() {
        return fnTip;
    }

    /** Draws the card with its top-left corner at {@code x, y}; the caller set the opacity. */
    void draw(GuiGraphicsExtractor g, float x, float y, float refresh) {
        if (kind == Kind.TRAVEL) {
            drawTravel(g, x, y);
            return;
        }
        Draw.frame(g, kind == Kind.CALL ? Frames.TOAST_CALL : Frames.TOAST, x, y, WIDTH, height);
        drawFace(g, x + PAD_X + FACE / 2f, y + PAD_Y + FACE / 2f);
        float tx = x + TEXT_X;
        float ty = y + PAD_Y;
        if (chip != null) {
            chip.draw(g, tx, ty + TITLE_H / 2f);
        }
        float nameX = tx + TagChip.room(chip);
        Draw.text(g, Fonts.strong(), title, nameX, Fonts.centeredY(Fonts.Face.STRONG, ty + TITLE_H / 2f),
            Theme.BONE_50);
        if (countText != null) {
            float px = nameX + titleWidth + 4;
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
        float hy = ty + HINT_GAP;
        for (Hint hint : hints) {
            hint.draw(g, tx, hy);
            hy += HINT_H + HINT_LINE_GAP;
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
            case MESSAGE, HOST, WORLD -> {
                if (face != null) {
                    face.draw(g, FACE, cx, cy);
                } else {
                    Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_600);
                }
            }
        }
    }

}
