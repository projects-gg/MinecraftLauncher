package gg.projects.overlay.voice;

import gg.projects.overlay.Perf;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.Shapes;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElement;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.util.FormattedCharSequence;

/**
 * Voice chat's corner of the HUD (VOICE-O; voice.md §5.3), top left while the panel is closed: the people of the
 * player's room who speak now, a face with its jade ring and the name on a small plate each ({@link Speakers}: in as
 * they start, a second after they stop, then out; opacity only), and above them, while the player's microphone is off
 * or they are deafened, the small sign of it. Nothing at all out of a room, while nobody speaks and the microphone is
 * on, with F1, under the debug screen or with the panel open (it shows the same itself); the speakers alone go with
 * the overlay's "Konuşanları göster". Out of a room it costs one check a frame.
 */
public final class VoiceHud implements HudElement {
    static final int MARGIN = 6;
    static final int ROW_H = 14;
    static final int GAP = 3;
    static final int FACE = 10;
    static final int PAD = 2;
    static final int NAME_GAP = 4;
    static final int END_PAD = 5;
    static final long MARK_MS = 120;

    private final Supplier<VoiceState> state;
    private final BooleanSupplier showSpeakers;
    private final Speakers speakers = new Speakers();
    private final Tween mark = new Tween(0f);
    private Icon markIcon = Icon.MIC_OFF;
    /** Ash for the player's own choice, flare for a moderator's mute (VOICE-KEYS: the room list's signs). */
    private int markInk = Theme.ASH_300;
    /** Names as the game draws them, made once per name and GUI scale (each String draw runs the bidi pass). */
    private final Map<String, Line> lines = new HashMap<>();
    /** The names of the room's people, kept while one who left still fades out. */
    private final Map<String, String> names = new HashMap<>();
    private int linesScale = -1;

    private record Line(FormattedCharSequence text, int width, Face face) {
    }

    public VoiceHud(Supplier<VoiceState> state, BooleanSupplier showSpeakers) {
        this.state = state;
        this.showSpeakers = showSpeakers;
    }

    /** Who speaks now in {@code room} (after every voice chat frame); someone who leaves keeps their name as they fade. */
    public void speaking(VoiceState.Room room, Collection<String> ids, long now) {
        if (room != null) {
            for (VoiceState.Person p : room.people()) {
                names.put(p.id(), p.name());
            }
        }
        speakers.update(ids, now);
    }

    /** The player's microphone or hearing changed (after every voice chat frame). */
    public void me(VoiceState.Me me, long now) {
        boolean show = me != null && me.micOff();
        if (show) {
            // Deafened shows the headphones alone (the microphone is off with it); a moderator's mute holds over both.
            markIcon = !me.mutedByModerator() && me.deaf() ? Icon.DEAF : Icon.MIC_OFF;
            markInk = me.mutedByModerator() ? Theme.FLARE_300 : Theme.ASH_300;
        }
        float target = show ? 1f : 0f;
        if (mark.target() != target) {
            mark.animateTo(target, now, MARK_MS, Ease.OUT);
        }
    }

    /** Out of the room, or the launcher went away: nothing stays. */
    public void clear() {
        speakers.clear();
        mark.snap(0f);
        lines.clear();
        names.clear();
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, DeltaTracker delta) {
        VoiceState voice = state.get();
        if (voice == null || voice.room() == null) {
            return;
        }
        long now = now();
        boolean marked = mark.target() > 0f || !mark.done(now);
        boolean speaking = showSpeakers.getAsBoolean() && !speakers.idle(now);
        if (!marked && !speaking) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if (Compat.hudHidden(mc) || mc.getDebugOverlay().showDebugScreen()
            || OverlayScreen.hudRightOf(Compat.screen(mc)) > 0) {
            return;
        }
        long started = Perf.start();
        Draw.stratum(g);
        Shapes.prepare();
        int scale = Draw.scale();
        if (scale != linesScale) {
            lines.clear();
            linesScale = scale;
        }
        float y = MARGIN;
        float m = mark.value(now);
        if (m > 0.004f) {
            Draw.pushAlpha(m);
            Draw.frame(g, Frames.SPEAKER, MARGIN, y, ROW_H, ROW_H);
            Draw.icon(g, markIcon, 8, MARGIN + 3, y + 3, markInk);
            Draw.popAlpha();
            y += ROW_H + GAP;
        }
        if (speaking) {
            VoiceState.Room room = voice.room();
            Font font = Fonts.ui();
            List<Speakers.Shown> rows = speakers.shown(now);
            for (Speakers.Shown row : rows) {
                if (row.alpha() <= 0.004f) {
                    y += ROW_H + GAP;
                    continue;
                }
                VoiceState.Person person = room.person(row.id());
                String name = person != null ? person.name() : names.getOrDefault(row.id(), "");
                Line line = lines.computeIfAbsent(name, k -> line(font, k));
                int w = PAD + FACE + NAME_GAP + line.width + END_PAD;
                Draw.pushAlpha(row.alpha());
                Draw.frame(g, Frames.SPEAKER, MARGIN, y, w, ROW_H);
                float fx = MARGIN + PAD;
                float fy = y + (ROW_H - FACE) / 2f;
                line.face.draw(g, FACE, fx + FACE / 2f, fy + FACE / 2f);
                Draw.frame(g, Frames.SPEAKING, Draw.snap(fx), Draw.snap(fy), FACE, FACE);
                Draw.text(g, font, line.text, fx + FACE + NAME_GAP, Fonts.centeredY(Fonts.Face.UI, y + ROW_H / 2f),
                    Theme.BONE_100);
                Draw.popAlpha();
                y += ROW_H + GAP;
            }
            int beyond = speakers.beyond(now);
            if (beyond > 0) {
                String more = "+" + beyond;
                int w = PAD + font.width(more) + END_PAD;
                Draw.frame(g, Frames.SPEAKER, MARGIN, y, w + 2, ROW_H);
                Draw.text(g, font, more, MARGIN + PAD + 2, Fonts.centeredY(Fonts.Face.UI, y + ROW_H / 2f),
                    Theme.ASH_300);
            }
        }
        Perf.toast(started);
    }

    private static Line line(Font font, String name) {
        String shown = name.isEmpty() ? Texts.str("voice.someone") : name;
        String fitted = Texts.fit(font, shown, 120);
        return new Line(Texts.visual(fitted), font.width(fitted), Face.of(shown));
    }

    private static long now() {
        return System.nanoTime() / 1_000_000L;
    }
}
