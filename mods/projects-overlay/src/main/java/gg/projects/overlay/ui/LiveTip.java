package gg.projects.overlay.ui;

import gg.projects.overlay.lang.Language;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.voice.VoiceState;
import java.util.Objects;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;

/**
 * The live preview card (SHARE-LIVE; the launcher's card on "YAYINDA"): the pointer resting on someone's "YAYINDA" for
 * 350 ms opens it beside the row: their stream's last small picture (16:9), their name, what they share and how many
 * watch. Where the player may not go it shows no picture: their face large and dimmed, a lock and one line; until a
 * picture comes the dimmed face waits alone. It asks the launcher once per opening (again only when the room's picture
 * time moves while it stays open) and lets the picture go when it closes. A card right after another opens at once.
 * Pointer only, and no buttons: the game shows no stream ("İzle" is the launcher's), and the narrator already says
 * "Yayında" with the row. Fades in over 110 ms (opacity only), goes at once.
 */
public final class LiveTip {
    /** The pointer rests this long on a "YAYINDA" before its card opens (the launcher's cards wait as long). */
    public static final long DELAY_MS = 350;
    /** Another "YAYINDA" within this long of the last card: no wait. */
    static final long QUICK_MS = 150;
    static final long FADE_MS = 110;
    /** Not followed for this long (its list was not drawn): it closes. */
    private static final long STALE_MS = 250;
    /** The picture at most (GUI pixels, 16:9). */
    static final int IMAGE_W = 128;
    static final int PAD = 3;
    /** The name and the line under it. */
    static final int TEXT_H = 21;
    /** Between the row and the card. */
    private static final int GAP = 2;
    private static final int FACE = 12;

    /** Asks the launcher for someone's picture ({@code voice-share-thumb}). */
    public interface Asker {
        void ask(String person);
    }

    private String person;
    private Face face;
    private String name = "";
    private String meta = "";
    private float anchorX;
    private float anchorTop;
    private float anchorBottom;
    private long since;
    private long trackedAt = Long.MIN_VALUE / 2;
    private long closedAt = Long.MIN_VALUE / 2;
    private boolean asked;
    private long askedTh;
    private VoiceState voice;
    private final SharePicture picture = new SharePicture();
    // Laid out for one width, name, line and language.
    private Language laidOutIn;
    private int laidOutW = -1;
    private String laidOutName;
    private String laidOutMeta;
    private String laidOutWhy;
    private FormattedCharSequence nameLine;
    private FormattedCharSequence metaLine;
    private FormattedCharSequence lockLine;
    private float lockWidth;

    /**
     * Follows what the pointer rests on this frame: {@code person}'s "YAYINDA" drawn at {@code x} on the row from
     * {@code top} to {@code bottom} ({@code name}, their {@code face}, {@code meta}: what they share and how many watch,
     * {@code th}: the room's picture time, 0 when not known), or nothing ({@code person} null).
     */
    public void track(VoiceState voice, String person, Face face, String name, String meta, long th, float x,
        float top, float bottom, long now, Asker asker) {
        if (now - trackedAt > STALE_MS && this.person != null) {
            // The list was away (the panel closed, another page): the card went with it.
            close(now);
            closedAt = Long.MIN_VALUE / 2;
        }
        trackedAt = now;
        this.voice = voice;
        if (person == null) {
            if (this.person != null) {
                close(now);
            }
            return;
        }
        anchorX = x;
        anchorTop = top;
        anchorBottom = bottom;
        if (!person.equals(this.person)) {
            if (this.person != null) {
                close(now);
            }
            this.person = person;
            since = now - closedAt < QUICK_MS ? now - DELAY_MS : now;
            asked = false;
        }
        this.face = face;
        this.name = name == null ? "" : name;
        this.meta = meta == null ? "" : meta;
        if (now - since >= DELAY_MS && (!asked || (th > 0 && th != askedTh))) {
            asked = true;
            askedTh = th;
            asker.ask(person);
        }
    }

    /** Whether the pointer is on a "YAYINDA" (its card shows or waits to). */
    public boolean active() {
        return person != null;
    }

    private boolean open(long now) {
        return person != null && now - since >= DELAY_MS;
    }

    private void close(long now) {
        if (open(now)) {
            closedAt = now;
        }
        picture.close();
        if (voice != null) {
            voice.dropThumb(person);
        }
        person = null;
    }

    /** The panel closed or the world went: nothing is held. */
    public void release() {
        picture.close();
        if (voice != null && person != null) {
            voice.dropThumb(person);
        }
        person = null;
    }

    private void layout(int width, String why) {
        Language language = Texts.current();
        if (language == laidOutIn && width == laidOutW && Objects.equals(name, laidOutName)
            && Objects.equals(meta, laidOutMeta) && Objects.equals(why, laidOutWhy)) {
            return;
        }
        laidOutIn = language;
        laidOutW = width;
        laidOutName = name;
        laidOutMeta = meta;
        laidOutWhy = why;
        int textW = width - FACE - 5;
        nameLine = Texts.visual(Texts.fit(Fonts.strong(), name, textW));
        metaLine = meta.isEmpty() ? null : Texts.visual(Texts.fit(Fonts.small(), meta, textW));
        if (why == null) {
            lockLine = null;
        } else {
            String line = Texts.fit(Fonts.small(), Texts.str(switch (why) {
                case "full" -> "share.preview.full";
                case "private" -> "share.preview.private";
                default -> "share.preview.locked";
            }), width - 8);
            lockLine = Texts.visual(line);
            lockWidth = Fonts.small().width(line);
        }
    }

    /**
     * Draws it under the row (over it without room below), inside {@code left..right} and {@code top..bottom}: the
     * list's own box.
     */
    public void draw(GuiGraphicsExtractor g, long now, float left, float top, float right, float bottom) {
        if (!open(now) || face == null) {
            return;
        }
        VoiceState.Thumb thumb = voice == null ? null : voice.thumb(person);
        boolean locked = thumb != null && thumb.locked();
        int imageW = (int) Math.min(IMAGE_W, Math.max(80, right - left - 2 * PAD));
        int imageH = Math.round(imageW * 9f / 16f);
        float w = imageW + 2 * PAD;
        float h = PAD + imageH + TEXT_H + PAD;
        float x = Math.max(left, Math.min(right - w, anchorX - PAD));
        float y = anchorBottom + GAP;
        if (y + h > bottom) {
            y = anchorTop - GAP - h;
        }
        if (y < top) {
            y = Math.max(top, bottom - h);
        }
        x = Draw.snap(x);
        y = Draw.snap(y);
        layout(imageW, locked ? (thumb.why().isEmpty() ? "locked" : thumb.why()) : null);
        float t = Math.min(1f, (now - since - DELAY_MS) / (float) FADE_MS);
        Draw.pushAlpha(Ease.OUT.at(t));
        Draw.frame(g, Frames.POPOVER, x, y, w, h);
        float ix = x + PAD;
        float iy = y + PAD;
        Draw.fill(g, ix, iy, ix + imageW, iy + imageH, Theme.OBSIDIAN_950);
        Identifier texture = locked ? null : picture.texture(thumb);
        if (texture != null) {
            drawCovered(g, texture, ix, iy, imageW, imageH);
        } else {
            // The face large and dimmed: no picture yet, or none the player may see.
            Draw.pushAlpha(locked ? 0.2f : 0.32f);
            face.draw(g, Math.min(imageH - 10, 48), ix + imageW / 2f, iy + imageH / 2f);
            Draw.popAlpha();
            if (locked && lockLine != null) {
                float mid = iy + imageH / 2f;
                Draw.icon(g, Icon.LOCK, 12, ix + (imageW - 12) / 2f, mid - 12f, Theme.BONE_100);
                Draw.text(g, Fonts.small(), lockLine, ix + (imageW - lockWidth) / 2f,
                    Fonts.centeredY(Fonts.Face.SMALL, mid + 7f), Theme.BONE_100);
            }
        }
        float ty = iy + imageH + 3;
        face.draw(g, FACE, ix + FACE / 2f, ty + TEXT_H / 2f - 1);
        float tx = ix + FACE + 5;
        Font strong = Fonts.strong();
        if (metaLine == null) {
            Draw.text(g, strong, nameLine, tx, Fonts.centeredY(Fonts.Face.STRONG, ty + TEXT_H / 2f - 1), Theme.BONE_50);
        } else {
            Draw.text(g, strong, nameLine, tx, Fonts.centeredY(Fonts.Face.STRONG, ty + 5f), Theme.BONE_50);
            Draw.text(g, Fonts.small(), metaLine, tx, Fonts.centeredY(Fonts.Face.SMALL, ty + 14f), Theme.ASH_300);
        }
        Draw.popAlpha();
    }

    /** The picture filling the box, its middle kept when its shape is not 16:9 (a window). */
    private void drawCovered(GuiGraphicsExtractor g, Identifier texture, float x, float y, int w, int h) {
        int tw = Math.max(1, picture.width());
        int th = Math.max(1, picture.height());
        float boxAspect = w / (float) h;
        float u = 0f;
        float v = 0f;
        int uw = tw;
        int vh = th;
        if (tw / (float) th > boxAspect) {
            uw = Math.max(1, Math.round(th * boxAspect));
            u = (tw - uw) / 2f;
        } else {
            vh = Math.max(1, Math.round(tw / boxAspect));
            v = (th - vh) / 2f;
        }
        Draw.texture(g, texture, x, y, w, h, u, v, uw, vh, tw, th, 0xFFFFFFFF);
    }
}
