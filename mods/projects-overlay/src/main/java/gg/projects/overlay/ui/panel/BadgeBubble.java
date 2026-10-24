package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Emblems;
import gg.projects.overlay.ui.draw.Frames;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * A crew tag's bubble in the panel (TEAM6 §6.3, the launcher's tag card without its actions): the crew's mark, its
 * name with the tag, its motto (two lines at most), "Kuruluş Eylül 2026 · 12 üye" and, in a crew of the player's,
 * "Sen de bu ekiptesin". Requests, reports and wearing a tag stay in the launcher: nothing here can be pressed.
 *
 * <p>It opens 350 ms after the pointer comes to rest on a chip (600 ms after a friend row with a chip is picked with the
 * keys), at once and without motion when it moves from one chip to another, and closes 120 ms after the pointer has
 * left both the chip and the bubble. In: 120 ms, fading in as it rises 3 px; out: 80 ms, fading. Under reduced motion
 * it only fades. Laid out once per card; nothing runs while it is closed.
 *
 * <p>It stands beside the panel, over the dimmed world, its first line level with the chip: nothing in the panel goes
 * under it. Where the screen has no room there, it opens under the row (or the group of messages) the chip belongs
 * to, or over it, never across it.
 */
public final class BadgeBubble {
    public static final long HOVER_MS = 350;
    public static final long KEYS_MS = 600;
    /** Another chip within this long of the last bubble: no wait, no motion. */
    static final long SWITCH_MS = 150;
    static final long LEAVE_MS = 120;
    private static final long IN_MS = 120;
    private static final long OUT_MS = 80;
    private static final int MAX_W = 160;
    private static final int PAD = 7;
    private static final int TEXT_X = PAD + Emblems.MARK + 6;
    private static final int NAME_H = 10;
    private static final int SMALL_H = 9;
    /** Between the panel and the bubble beside it, and the least room it leaves at the screen's edge. */
    private static final int BESIDE_GAP = 6;

    /**
     * A chip someone points at (or picked with the keys): which one, its card, where the chip is ({@code x, y, w, h})
     * and the rows the bubble must not cover when it has to open inside the panel ({@code keepTop..keepBottom}).
     */
    public record Target(Object key, OverlayState.Badge badge, float x, float y, float w, float h, float keepTop,
        float keepBottom, boolean keys) {
    }

    private final Tween alpha = new Tween(0f);
    private boolean reducedMotion;
    // What shows, where.
    private Object shownKey;
    private OverlayState.Badge shown;
    private float anchorX;
    private float anchorY;
    private float anchorH;
    private float keepTop;
    private float keepBottom;
    private long shownAt;
    private boolean animateIn;
    private long closedAt = Long.MIN_VALUE / 2;
    // What waits to show, and a chip Esc closed the bubble of (it stays quiet while still pointed at).
    private Object waitingKey;
    private Object quietKey;
    private long waitingSince;
    private long leftAt = -1;
    // The layout of the card shown (made once per card and language).
    private OverlayState.Badge laidOut;
    private boolean laidOutTurkish;
    private FormattedCharSequence name;
    private TagChip chip;
    private final List<FormattedCharSequence> motto = new ArrayList<>(2);
    private FormattedCharSequence meta;
    private FormattedCharSequence member;
    private float width;
    private float height;
    private Component narration = Component.empty();
    // Where it was drawn last (for the pointer).
    private float boxX;
    private float boxY;

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** Whether a bubble shows (or is fading out). */
    public boolean showing(long now) {
        return shown != null && alpha.value(now) > 0.01f;
    }

    /** Whether {@code (mx, my)} is on the bubble as it was drawn last. */
    public boolean contains(double mx, double my) {
        return shown != null && alpha.target() > 0f && mx >= boxX && mx < boxX + width && my >= boxY
            && my < boxY + height;
    }

    /** Closes at once (the list changed under it, the panel went to another page). */
    public void dismiss(long now) {
        if (shown != null && alpha.target() > 0f) {
            alpha.animateTo(0f, now, OUT_MS, Ease.OUT);
            closedAt = now;
        }
        waitingKey = null;
        leftAt = -1;
    }

    /**
     * Esc: closes it, and the chip it was on stays quiet until the pointer or the keys go somewhere else (else the
     * picked row would open it again a moment later). Returns whether one was open.
     */
    public boolean close(long now) {
        if (shown == null || alpha.target() <= 0f) {
            return false;
        }
        quietKey = shownKey;
        dismiss(now);
        return true;
    }

    /**
     * Follows what the pointer (or the keyboard) is on this frame: {@code target} null when no chip, {@code onBubble}
     * when the pointer is on the bubble itself (it stays open to be read).
     */
    public void track(Target target, boolean onBubble, long now) {
        if (target != null && quietKey != null) {
            if (Objects.equals(target.key(), quietKey)) {
                target = null;
            } else {
                quietKey = null;
            }
        } else if (target == null && !onBubble) {
            quietKey = null;
        }
        if (target == null || target.badge() == null) {
            waitingKey = null;
            if (onBubble || shown == null || alpha.target() <= 0f) {
                leftAt = -1;
                return;
            }
            if (leftAt < 0) {
                leftAt = now;
            } else if (now - leftAt >= LEAVE_MS) {
                leftAt = -1;
                alpha.animateTo(0f, now, OUT_MS, Ease.OUT);
                closedAt = now;
            }
            return;
        }
        leftAt = -1;
        boolean open = shown != null && alpha.target() > 0f;
        if (open && Objects.equals(target.key(), shownKey)) {
            // Still on it: it follows the chip (the list may glide) and takes a card that changed.
            place(target);
            if (!target.badge().equals(shown)) {
                shown = target.badge();
                layout();
            }
            return;
        }
        if (!target.keys() && (open || now - closedAt < SWITCH_MS)) {
            // From one chip to the next: the bubble goes there at once, without motion.
            show(target, now, false);
            return;
        }
        if (open) {
            // The keys went to another row: this bubble closes and the next waits its own moment (arrowing through
            // the list does not flash a bubble on every row).
            alpha.animateTo(0f, now, OUT_MS, Ease.OUT);
            closedAt = now;
        }
        if (!Objects.equals(target.key(), waitingKey)) {
            waitingKey = target.key();
            waitingSince = now;
            return;
        }
        if (now - waitingSince >= (target.keys() ? KEYS_MS : HOVER_MS)) {
            show(target, now, true);
        }
    }

    private void show(Target target, long now, boolean animate) {
        shownKey = target.key();
        shown = target.badge();
        place(target);
        waitingKey = null;
        animateIn = animate;
        shownAt = now;
        if (animate) {
            alpha.animateTo(1f, now, IN_MS, Ease.OUT);
        } else {
            alpha.snap(1f);
        }
        layout();
        // Read once as it opens (the narrator's own choice whether it speaks).
        Minecraft.getInstance().getNarrator().saySystemQueued(narration);
    }

    private void place(Target target) {
        anchorX = target.x();
        anchorY = target.y();
        anchorH = target.h();
        keepTop = target.keepTop();
        keepBottom = target.keepBottom();
    }

    /** Lays the card out (once per card and language). */
    private void layout() {
        boolean turkish = Texts.turkish();
        if (shown.equals(laidOut) && turkish == laidOutTurkish) {
            return;
        }
        laidOut = shown;
        laidOutTurkish = turkish;
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        chip = TagChip.of(shown, TagChip.ROW);
        int room = MAX_W - TEXT_X - PAD;
        String nameText = shown.name().isEmpty() ? shown.tag() : shown.name();
        String fitted = Texts.fit(strong, nameText, Math.round(room - TagChip.room(chip)));
        name = Texts.visual(fitted);
        float textW = strong.width(fitted) + TagChip.room(chip);
        motto.clear();
        if (!shown.motto().isEmpty()) {
            for (FormattedCharSequence line : Texts.wrap(small, shown.motto(), room, 2)) {
                motto.add(line);
                textW = Math.max(textW, small.width(line));
            }
        }
        String founded = founded(shown.since());
        String count = shown.members() <= 0 ? null : Texts.str(shown.members() == 1 ? "badge.members.one"
            : "badge.members", shown.members());
        String metaText = founded != null && count != null ? Texts.str("badge.meta", founded, count)
            : founded != null ? founded : count;
        meta = metaText == null ? null : Texts.visual(Texts.fit(small, metaText, room));
        if (metaText != null) {
            textW = Math.max(textW, Math.min(room, small.width(metaText)));
        }
        String memberText = shown.mine() ? Texts.str("badge.member") : null;
        member = memberText == null ? null : Texts.visual(memberText);
        if (memberText != null) {
            textW = Math.max(textW, small.width(memberText));
        }
        width = Math.min(MAX_W, TEXT_X + (float) Math.ceil(textW) + PAD);
        float text = NAME_H + (motto.isEmpty() ? 0 : 2 + motto.size() * SMALL_H) + (meta == null ? 0 : 2 + SMALL_H)
            + (member == null ? 0 : SMALL_H);
        height = 2 * PAD + Math.max(Emblems.MARK, text);
        List<String> spoken = new ArrayList<>(4);
        spoken.add(Texts.str("badge.narration", nameText));
        if (!shown.motto().isEmpty()) {
            spoken.add(shown.motto());
        }
        if (metaText != null) {
            spoken.add(metaText);
        }
        if (memberText != null) {
            spoken.add(memberText);
        }
        narration = Component.literal(String.join(". ", spoken) + ".");
    }

    /** "Kuruluş Eylül 2026" from "2026-09" (the month in the game's language), or null. */
    static String founded(String since) {
        int[] ym = yearMonth(since);
        return ym == null ? null : Texts.str("badge.founded", Texts.str("month." + ym[1]) + " " + ym[0]);
    }

    /** {@code {year, month}} of "YYYY-MM", or null when it is not one. */
    static int[] yearMonth(String since) {
        if (since == null || !since.matches("\\d{4}-\\d{2}")) {
            return null;
        }
        int year = Integer.parseInt(since.substring(0, 4));
        int month = Integer.parseInt(since.substring(5, 7));
        return month < 1 || month > 12 ? null : new int[] {year, month};
    }

    /**
     * Draws the bubble beside the panel ({@code panelLeft}, its left edge), level with its chip; without room there,
     * inside {@code left..right}, under or over the rows it must keep clear, within {@code top..bottom}.
     */
    public void draw(GuiGraphicsExtractor g, long now, float panelLeft, float left, float top, float right,
        float bottom) {
        if (shown == null) {
            return;
        }
        float a = alpha.value(now);
        if (a <= 0.01f) {
            if (alpha.target() <= 0f) {
                shown = null;
                shownKey = null;
            }
            return;
        }
        float x;
        float y;
        if (panelLeft - BESIDE_GAP - width >= BESIDE_GAP) {
            x = panelLeft - BESIDE_GAP - width;
            y = anchorY + anchorH / 2f - PAD - NAME_H / 2f;
        } else {
            x = Math.max(left, Math.min(right - width, anchorX));
            y = keepBottom + 3;
            if (y + height > bottom && keepTop - 3 - height >= top) {
                y = keepTop - 3 - height;
            }
        }
        y = Math.max(top, Math.min(bottom - height, y));
        x = Draw.snap(x);
        y = Draw.snap(y);
        boxX = x;
        boxY = y;
        boolean rising = animateIn && alpha.target() > 0f && !reducedMotion;
        float rise = rising ? (1f - Ease.OUT.at(Math.min(1f, (now - shownAt) / (float) IN_MS))) * 3f : 0f;
        Matrix3x2fStack m = g.pose();
        m.pushMatrix();
        m.translate(0f, rise);
        Draw.pushAlpha(a);
        Draw.frame(g, Frames.POPOVER, x, y, width, height);
        Emblems.draw(g, shown.color(), shown.frame(), shown.emblem(), x + PAD, y + PAD);
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        float tx = x + TEXT_X;
        float ty = y + PAD;
        Draw.text(g, strong, name, tx, Fonts.centeredY(Fonts.Face.STRONG, ty + NAME_H / 2f), Theme.BONE_50);
        if (chip != null) {
            chip.draw(g, tx + strong.width(name) + TagChip.GAP, ty + NAME_H / 2f);
        }
        ty += NAME_H;
        if (!motto.isEmpty()) {
            ty += 2;
            for (FormattedCharSequence line : motto) {
                Draw.text(g, small, line, tx, Fonts.centeredY(Fonts.Face.SMALL, ty + SMALL_H / 2f), Theme.BONE_300);
                ty += SMALL_H;
            }
        }
        if (meta != null) {
            ty += 2;
            Draw.text(g, small, meta, tx, Fonts.centeredY(Fonts.Face.SMALL, ty + SMALL_H / 2f), Theme.ASH_300);
            ty += SMALL_H;
        }
        if (member != null) {
            Draw.text(g, small, member, tx, Fonts.centeredY(Fonts.Face.SMALL, ty + SMALL_H / 2f), Theme.ASH_300);
        }
        Draw.popAlpha();
        m.popMatrix();
    }
}
