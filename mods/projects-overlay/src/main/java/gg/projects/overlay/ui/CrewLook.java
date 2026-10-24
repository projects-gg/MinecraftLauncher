package gg.projects.overlay.ui;

import java.util.List;
import java.util.Map;

/**
 * A crew's look (TEAM2 §4.1, the launcher's {@code crew-look.ts}): 12 colours, 4 frames and 36 emblems. The crew
 * colour is the crew's content, like an avatar: it fills the mark's frame and tints the tag chip, never an interface
 * colour (buttons, focus and links stay gold). Pure, so it is tested on its own.
 */
public final class CrewLook {
    /** A colour's frame fill, frame edge and emblem colour (ARGB). */
    public record Tone(int fill, int edge, int glyph) {
    }

    /** A frame's outline in a 40 x 40 box and where its emblem sits (a banner's tail and a shield's point sit low). */
    public record Shape(String key, String path, float glyphY) {
    }

    /** The box the frames are drawn in. */
    public static final float VIEW_BOX = 40f;
    /** The chip's tint: the crew colour's edge at 24 % (the launcher's {@code tagTint}). */
    public static final float TAG_TINT = 0.24f;

    private static final int GOLD_GLYPH = 0xFFF6D58E;
    private static final int BONE_GLYPH = 0xFFF3EBDD;
    private static final Map<String, Tone> TONES = Map.ofEntries(
        Map.entry("gold", new Tone(0xFF6E4A12, 0xFFE89A2C, GOLD_GLYPH)),
        Map.entry("copper", new Tone(0xFF74361A, 0xFFD9824E, GOLD_GLYPH)),
        Map.entry("crimson", new Tone(0xFF6E1C22, 0xFFD9574D, GOLD_GLYPH)),
        Map.entry("rose", new Tone(0xFF6C2141, 0xFFDB6E99, GOLD_GLYPH)),
        Map.entry("purple", new Tone(0xFF452868, 0xFFA57BE0, GOLD_GLYPH)),
        Map.entry("navy", new Tone(0xFF1C2B57, 0xFF6F8EE0, GOLD_GLYPH)),
        Map.entry("blue", new Tone(0xFF17466E, 0xFF5AA6E0, BONE_GLYPH)),
        Map.entry("teal", new Tone(0xFF12514F, 0xFF46BDB4, BONE_GLYPH)),
        Map.entry("green", new Tone(0xFF1C4F31, 0xFF5DB47E, GOLD_GLYPH)),
        Map.entry("silver", new Tone(0xFF3A3D48, 0xFFB3B9C8, BONE_GLYPH)),
        Map.entry("ivory", new Tone(0xFFD6CAB4, 0xFFF3EBDD, 0xFF2A1D10)),
        Map.entry("obsidian", new Tone(0xFF141118, 0xFF6B6172, GOLD_GLYPH)));

    /** The four frames, in the launcher's order; an unknown one is the cut. */
    public static final List<Shape> SHAPES = List.of(
        new Shape("cut", "M9 1.5H38.5V31L31 38.5H1.5V9Z", 20f),
        new Shape("shield", "M20 1.5L36.5 6.5V19C36.5 28.6 29.6 35.2 20 38.5C10.4 35.2 3.5 28.6 3.5 19V6.5Z", 18.5f),
        new Shape("banner", "M4.5 1.5H35.5V38.5L20 31L4.5 38.5Z", 16.5f),
        new Shape("hex", "M20 1.5L36.5 10.75V29.25L20 38.5L3.5 29.25V10.75Z", 20f));

    /**
     * Every emblem (the launcher's {@code EMBLEMS}, in the picker's order) and its Phosphor icon (fill weight), as
     * {@code team-emblem.tsx} draws them. An unknown emblem is a shield.
     */
    public static final List<String[]> EMBLEMS = List.of(
        new String[] {"sword", "Sword"}, new String[] {"shield", "Shield"}, new String[] {"axe", "Axe"},
        new String[] {"crown", "Crown"}, new String[] {"trophy", "Trophy"}, new String[] {"flag", "Flag"},
        new String[] {"target", "Target"}, new String[] {"skull", "Skull"}, new String[] {"ghost", "Ghost"},
        new String[] {"flame", "Fire"}, new String[] {"bolt", "Lightning"}, new String[] {"sun", "Sun"},
        new String[] {"moon", "Moon"}, new String[] {"star", "Star"}, new String[] {"planet", "Planet"},
        new String[] {"snowflake", "Snowflake"}, new String[] {"waves", "Waves"}, new String[] {"leaf", "Leaf"},
        new String[] {"tree", "Tree"}, new String[] {"cactus", "Cactus"}, new String[] {"paw", "PawPrint"},
        new String[] {"cat", "Cat"}, new String[] {"horse", "Horse"}, new String[] {"bird", "Bird"},
        new String[] {"fish", "FishSimple"}, new String[] {"butterfly", "Butterfly"}, new String[] {"heart", "Heart"},
        new String[] {"diamond", "Diamond"}, new String[] {"cube", "Cube"}, new String[] {"rocket", "Rocket"},
        new String[] {"potion", "Flask"}, new String[] {"wand", "MagicWand"}, new String[] {"castle", "CastleTurret"},
        new String[] {"controller", "GameController"}, new String[] {"alien", "Alien"}, new String[] {"robot", "Robot"});

    private CrewLook() {
    }

    /** A colour key's tones; an unknown one is gold (today's look, as in the launcher). */
    public static Tone tone(String color) {
        Tone t = color == null ? null : TONES.get(color);
        return t == null ? TONES.get("gold") : t;
    }

    /** The tag chip's fill: the crew colour's edge at 24 %. */
    public static int tagTint(String color) {
        return Theme.withAlpha(tone(color).edge(), TAG_TINT);
    }

    /** A frame key's shape; an unknown one is the cut. */
    public static Shape shape(String frame) {
        for (Shape s : SHAPES) {
            if (s.key().equals(frame)) {
                return s;
            }
        }
        return SHAPES.get(0);
    }

    /** An emblem key as drawn; an unknown one is the shield. */
    public static String emblem(String emblem) {
        for (String[] e : EMBLEMS) {
            if (e[0].equals(emblem)) {
                return e[0];
            }
        }
        return "shield";
    }
}
