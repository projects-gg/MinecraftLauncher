package gg.projects.overlay.toast;

import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.PixelSprites;
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
 * (HOST1) has the grass block for its face and "F7 ile katıl"; the host's own notes ("Dünyan arkadaşlarına açık", "Mert
 * dünyana katıldı") are the overlay's, under the player's "Diğer bildirimler".
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
    private static final int KEY_PAD = 3;

    public enum Kind { MESSAGE, CALL, INVITE, INFO, TRAVEL, WORLD, HOST }

    final Kind kind;
    final String conversation;
    final boolean crewChat;
    final String initial;
    final int initialWidth;
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
    final String hintBefore;
    final int hintBeforeWidth;
    final String hintKey;
    final int hintKeyWidth;
    final String hintAfter;
    /** The face is the world's grass block (a world invite, the world opening) rather than a disc. */
    final boolean sprite;
    // The short strings again, in the form the game draws: made once here, not on every frame (each String
    // draw runs the game's bidirectional text pass).
    private final FormattedCharSequence initialLine;
    private final FormattedCharSequence countLine;
    private final FormattedCharSequence hintBeforeLine;
    private final FormattedCharSequence hintKeyLine;
    private final FormattedCharSequence hintAfterLine;
    final float height;
    final Component narration;

    private ToastCard(Builder b) {
        this.kind = b.kind;
        this.conversation = b.conversation;
        this.crewChat = b.crewChat;
        this.initial = b.initial;
        this.initialWidth = Fonts.strong().width(b.initial);
        this.title = b.title;
        this.titleWidth = b.titleWidth;
        this.chip = b.chip;
        this.countText = b.countText;
        this.countWidth = b.countWidth;
        this.crew = b.crew;
        this.body = b.body;
        this.sticker = b.sticker;
        this.hintBefore = b.hintBefore;
        this.hintBeforeWidth = b.hintBefore == null ? 0 : Fonts.small().width(b.hintBefore);
        this.hintKey = b.hintKey;
        this.hintKeyWidth = b.hintKey == null ? 0 : Fonts.small().width(b.hintKey) + 2 * KEY_PAD;
        this.hintAfter = b.hintAfter;
        this.sprite = b.sprite;
        this.narration = b.narration;
        this.initialLine = line(b.initial);
        this.countLine = line(b.countText);
        this.hintBeforeLine = line(b.hintBefore);
        this.hintKeyLine = line(b.hintKey);
        this.hintAfterLine = line(b.hintAfter);
        if (kind == Kind.TRAVEL) {
            this.height = PAD_Y + Math.max(FACE, body.size() * LINE_H) + PAD_Y;
            return;
        }
        float h = PAD_Y + TITLE_H + (crew != null ? SMALL_H : 0) + 1
            + (sticker != null ? Math.max(STICKER + 1, body.size() * LINE_H) : body.size() * LINE_H);
        if (hasHint()) {
            h += HINT_GAP + HINT_H;
        }
        this.height = h + PAD_Y;
    }

    /** The hint line ("F7 ile arayüzü aç"): every kind but the overlay's quiet notes. */
    private boolean hasHint() {
        return hintBefore != null || hintKey != null;
    }

    private static FormattedCharSequence line(String text) {
        return text == null || text.isEmpty() ? null : Texts.visual(text);
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
        String hintBefore;
        String hintKey;
        String hintAfter;
        boolean sprite;
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
     */
    public static ToastCard of(OverlayState.ToastRequest request, boolean crewChat, int count, boolean showText,
        String keyName) {
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
                : "toast.hint.open", keyName);
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

    /** The hint with the overlay's key where the sentence has it ("F7 ile arayüzü aç", "Press F7 to open"). */
    private static void hint(Builder b, String key, String keyName) {
        if (keyName == null) {
            b.hintBefore = Texts.str("toast.hint.unbound");
            return;
        }
        String template = Texts.str(key, "\u0000");
        int at = template.indexOf('\u0000');
        b.hintBefore = at < 0 ? template.strip() : template.substring(0, at).strip();
        b.hintAfter = at < 0 ? "" : template.substring(at + 1).strip();
        b.hintKey = Texts.fit(Fonts.small(), keyName, 48);
    }

    /**
     * The host's own note (HOST1): {@code title} in bold with {@code body} under it, the grass block for its face when
     * it is about the world ({@code sprite}) or the friend's initial when it is about them; with {@code hintKey} the
     * hint line ("F7 ile yönet") follows.
     */
    public static ToastCard host(String title, String body, boolean sprite, String hintKey, String keyName) {
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
            hint(b, hintKey, keyName);
        }
        b.narration = Component.literal(body == null || body.isEmpty() ? title : title + ". " + body);
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
        if (hasHint()) {
            drawHint(g, tx, ty + HINT_GAP);
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
                Draw.disc(g, FACE, cx, cy, Theme.withAlpha(Theme.GOLD_500, 0.2f));
                Draw.icon(g, Icon.JOIN, 10, cx - 5, cy - 5, Theme.GOLD_300);
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
                Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_600);
                if (crewChat) {
                    Draw.icon(g, Icon.CREW, 10, cx - 5, cy - 5, Theme.BONE_100);
                } else if (initialLine != null) {
                    Draw.text(g, Fonts.strong(), initialLine, cx - initialWidth / 2f,
                        Fonts.centeredY(Fonts.Face.STRONG, cy), Theme.BONE_100);
                }
            }
        }
    }

    private void drawHint(GuiGraphicsExtractor g, float x, float y) {
        Font small = Fonts.small();
        float textY = Fonts.centeredY(Fonts.Face.SMALL, y + HINT_H / 2f);
        float cx = x;
        if (hintBeforeLine != null) {
            Draw.text(g, small, hintBeforeLine, cx, textY, Theme.ASH_300);
            cx += hintBeforeWidth + 3;
        }
        if (hintKeyLine == null) {
            return;
        }
        Draw.frame(g, Frames.KEYCAP, cx, y, hintKeyWidth, HINT_H);
        Draw.text(g, small, hintKeyLine, cx + KEY_PAD, textY - 0.5f, Theme.BONE_100);
        cx += hintKeyWidth + 3;
        if (hintAfterLine != null) {
            Draw.text(g, small, hintAfterLine, cx, textY, Theme.ASH_300);
        }
    }
}
