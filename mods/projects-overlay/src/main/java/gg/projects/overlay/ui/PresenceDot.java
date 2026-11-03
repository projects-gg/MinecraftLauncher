package gg.projects.overlay.ui;

import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.state.Presence;
import gg.projects.overlay.ui.draw.Draw;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/**
 * The presence dot (OVL-SOC), the launcher's {@code PresenceDot} (608825c9; docs/design.md "Friends (S2)"): every state a
 * filled disc, never a ring. Bright jade in a game; a solid softer jade in the launcher (jade mixed with obsidian as the
 * launcher mixes it, never see-through); gold with a dark bar across it on "Rahatsız etmeyin"; nothing offline. On a
 * face it sits on the corner in a plate of the surface behind it, so it reads against the face's gold; beside a name (a
 * conversation chip) it stands alone. It never moves: the dot is a fact, not a signal. A text line or the narration
 * always says the state too (never colour alone).
 */
public final class PresenceDot {
    /** The 5 GUI px dot and the 8 px plate it sits in on a face. */
    public static final int DOT = 5;
    public static final int PLATE = 8;
    /**
     * "In the launcher": jade-400 mixed 58 % into obsidian-950 in OKLab, as the launcher's
     * {@code color-mix(in oklab, jade-400 58%, obsidian-950)} (#3B7153).
     */
    public static final int LAUNCHER_JADE = 0xFF3B7153;

    private PresenceDot() {
    }

    /** The dot on a face's corner, centred on {@code cx, cy}, in a plate of {@code surface}; nothing offline. */
    public static void onFace(GuiGraphicsExtractor g, Presence presence, float cx, float cy, int surface) {
        if (presence == null || !presence.online()) {
            return;
        }
        Draw.disc(g, PLATE, cx, cy, surface);
        dot(g, presence, cx, cy);
    }

    /** The dot alone, centred on {@code cx, cy}; nothing offline. */
    public static void dot(GuiGraphicsExtractor g, Presence presence, float cx, float cy) {
        if (presence == null) {
            return;
        }
        switch (presence) {
            case PLAYING -> Draw.disc(g, DOT, cx, cy, Theme.JADE_400);
            case LAUNCHER -> Draw.disc(g, DOT, cx, cy, LAUNCHER_JADE);
            case DND -> {
                Draw.disc(g, DOT, cx, cy, Theme.GOLD_400);
                // The bar: about half the dot wide, one GUI pixel tall, in the dark the dot sits on, on whole device
                // pixels so it stays even at every GUI scale.
                float bx = Draw.snap(cx - 1.5f);
                float by = Draw.snap(cy - 0.5f);
                Draw.fill(g, bx, by, bx + 3f, by + 1f, Theme.OBSIDIAN_950);
            }
            case OFFLINE -> {
            }
        }
    }

    /** What the narration says after the place ("rahatsız etmeyin"), or null where the place says enough. */
    public static String spoken(Presence presence) {
        if (presence != Presence.DND) {
            return null;
        }
        String words = Texts.str("presence.dnd");
        return words.isEmpty() ? words : words.substring(0, 1).toLowerCase(Texts.current().locale) + words.substring(1);
    }

    /**
     * Where a friend is and how they stand, for a narration that has no place line of its own (a conversation chip):
     * "Projects'te", "Launcher'da, rahatsız etmeyin"; null offline (the chip has no dot then).
     */
    public static String heard(OverlayState.Friend f) {
        if (f == null || !f.online()) {
            return null;
        }
        String key = switch (f.where()) {
            case "projects", "launcher", "single", "server", "game" -> f.where();
            default -> "game";
        };
        String place = Texts.str("friend.where." + key);
        String dnd = spoken(f.presence());
        return dnd == null ? place : place + ", " + dnd;
    }
}
