package gg.projects.overlay.ui.draw;

import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.ShapeBaker.FrameSpec;

/**
 * Every frame the overlay draws, from the launcher's surfaces (docs/design.md §2 and §6): containers are
 * rounded (the launcher's 10 px, 5 GUI pixels), actions carry the hextech chamfer. Sizes are GUI pixels.
 */
public enum Frames {
    /**
     * The toast card: translucent obsidian (the owner: the world should show through a little, never a solid
     * block), a hairline and an inner top light so the edge reads on any scene. 80 % keeps ash-300 text at
     * 4.5:1 even over white snow or sky. Always {@link #TOAST_WIDTH} wide, so it is baked whole across.
     */
    TOAST(FrameSpec.rounded(6, Theme.withAlpha(Theme.OBSIDIAN_900, Alphas.TOAST), Theme.withAlpha(0xFFFFFF, 0.10f))
        .withHighlight(Theme.withAlpha(0xFFFFFF, 0.07f))
        .withShadow(Theme.withAlpha(0x220606, 0.5f), 8f, 3f).fixedWidth(Sizes.TOAST_WIDTH)),
    /** A crew call's toast: the same card with a gold hairline (a call is the one loud thing). */
    TOAST_CALL(FrameSpec.rounded(6, Theme.withAlpha(Theme.mix(Theme.OBSIDIAN_900, Theme.GOLD_500, 0.06f), Alphas.TOAST),
        Theme.withAlpha(Theme.GOLD_400, 0.34f))
        .withHighlight(Theme.withAlpha(Theme.GOLD_200, 0.08f))
        .withShadow(Theme.withAlpha(0x220606, 0.5f), 8f, 3f).fixedWidth(Sizes.TOAST_WIDTH)),
    /**
     * A question over the game (a link to open, a friend's server to join): the toast's card at any width (the
     * toast's own frame is baked at the toast's width only), a little denser, since it is read word by word.
     */
    DIALOG(FrameSpec.rounded(6, Theme.withAlpha(Theme.OBSIDIAN_900, Alphas.DIALOG), Theme.withAlpha(0xFFFFFF, 0.10f))
        .withHighlight(Theme.withAlpha(0xFFFFFF, 0.07f))
        // Softer than a toast's: over the dimmed game a heavy ember shadow read as a dirty ring at GUI 3 (SKIN1-O).
        .withShadow(Theme.withAlpha(0x220606, 0.32f), 5f, 2f)),
    /** Cards inside the panel (crew call, notices). */
    CARD(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.86f), Theme.withAlpha(0xFFFFFF, 0.07f))),
    /** The crew call card: a touch of gold, the one loud thing in the panel. */
    CARD_GOLD(FrameSpec.rounded(5, Theme.withAlpha(Theme.mix(Theme.OBSIDIAN_800, Theme.GOLD_500, 0.11f), 0.92f),
        Theme.withAlpha(Theme.GOLD_400, 0.30f))),
    BUBBLE_THEIRS(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.9f), Theme.withAlpha(0xFFFFFF, 0.07f))),
    BUBBLE_MINE(FrameSpec.rounded(5, Theme.withAlpha(Theme.BUBBLE_MINE, 0.96f), Theme.withAlpha(Theme.GOLD_400, 0.2f))),
    BUBBLE_FAILED(FrameSpec.rounded(5, Theme.withAlpha(Theme.BUBBLE_MINE, 0.96f), Theme.withAlpha(Theme.FLARE_400, 0.5f))),
    BUBBLE_DELETED(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.35f), Theme.withAlpha(0xFFFFFF, 0.06f))),
    FIELD(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.82f), Theme.withAlpha(0xFFFFFF, 0.10f))),
    FIELD_HOVER(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.82f), Theme.withAlpha(0xFFFFFF, 0.18f))),
    FIELD_FOCUS(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.86f), Theme.withAlpha(Theme.GOLD_400, 0.6f))
        .withRing(Theme.withAlpha(Theme.GOLD_400, 0.2f), 1.5f)),
    FIELD_ERROR(FrameSpec.rounded(5, Theme.withAlpha(Theme.OBSIDIAN_800, 0.86f), Theme.withAlpha(Theme.FLARE_400, 0.55f))),
    /** Gold action: the launcher's gradient (gold-300 to gold-600) under a gold-600 hairline. */
    BUTTON_PRIMARY(FrameSpec.rounded(3, 0, Theme.GOLD_600)
        .withGradient(Theme.mix(Theme.GOLD_300, Theme.GOLD_400, 0.45f), Theme.GOLD_600).chamfer().fixed(16)),
    BUTTON_PRIMARY_HOVER(FrameSpec.rounded(3, 0, Theme.GOLD_600)
        .withGradient(Theme.GOLD_200, Theme.mix(Theme.GOLD_500, Theme.GOLD_600, 0.4f)).chamfer().fixed(16)),
    BUTTON_SECONDARY(FrameSpec.rounded(3, Theme.withAlpha(Theme.OBSIDIAN_800, 0.88f), Theme.withAlpha(0xFFFFFF, 0.12f))
        .chamfer().fixed(16)),
    BUTTON_SECONDARY_HOVER(FrameSpec.rounded(3, Theme.withAlpha(Theme.OBSIDIAN_700, 0.92f),
        Theme.withAlpha(0xFFFFFF, 0.2f)).chamfer().fixed(16)),
    BUTTON_GHOST_HOVER(FrameSpec.rounded(3, Theme.withAlpha(0xFFFFFF, 0.07f), 0).chamfer().fixed(16)),
    /**
     * A destructive action ("Paylaşımı kapat", "Dünyadan çıkar"; HOST1): the secondary button's obsidian under a
     * flare hairline. Flare only for what ends something (docs/design.md §2).
     */
    BUTTON_DANGER(FrameSpec.rounded(3, Theme.withAlpha(Theme.OBSIDIAN_800, 0.88f), Theme.withAlpha(Theme.FLARE_400, 0.5f))
        .chamfer().fixed(16)),
    BUTTON_DANGER_HOVER(FrameSpec.rounded(3, Theme.withAlpha(Theme.mix(Theme.OBSIDIAN_800, Theme.FLARE_400, 0.16f), 0.94f),
        Theme.withAlpha(Theme.FLARE_400, 0.78f)).chamfer().fixed(16)),
    /** A friend picked for an invite (HOST1): a small chamfered box, gold once picked. */
    CHECK_OFF(FrameSpec.rounded(2, Theme.withAlpha(Theme.OBSIDIAN_950, 0.55f), Theme.withAlpha(0xFFFFFF, 0.24f))
        .chamfer().fixed(10)),
    CHECK_ON(FrameSpec.rounded(2, 0, Theme.GOLD_600)
        .withGradient(Theme.mix(Theme.GOLD_300, Theme.GOLD_400, 0.45f), Theme.GOLD_600).chamfer().fixed(10)),
    FOCUS_16(FrameSpec.rounded(3, 0, 0).withRing(Theme.GOLD_300, 1.5f).chamfer().fixed(16)),
    /**
     * Conversation chips, quick replies, "Katıl", "Yeni mesajlar" and the small text actions: actions, so chamfered
     * like every chip and button (the two-shape rule; CHAT2-O2's second look found them rounded).
     */
    CHIP(FrameSpec.rounded(3, Theme.withAlpha(0xFFFFFF, 0.055f), Theme.withAlpha(0xFFFFFF, 0.08f)).chamfer().fixed(14)),
    CHIP_HOVER(FrameSpec.rounded(3, Theme.withAlpha(0xFFFFFF, 0.10f), Theme.withAlpha(0xFFFFFF, 0.14f)).chamfer()
        .fixed(14)),
    CHIP_SELECTED(FrameSpec.rounded(3, Theme.withAlpha(Theme.OBSIDIAN_700, 0.95f), Theme.withAlpha(Theme.GOLD_400, 0.4f))
        .chamfer().fixed(14)),
    FOCUS_14(FrameSpec.rounded(3, 0, 0).withRing(Theme.GOLD_300, 1.5f).chamfer().fixed(14)),
    /** A row under the pointer or picked with the keys (friends, settings): a container, so rounded, at any height. */
    ROW_HOVER(FrameSpec.rounded(4, Theme.withAlpha(0xFFFFFF, 0.10f), Theme.withAlpha(0xFFFFFF, 0.14f))),
    /** The ring around a row picked with the keys. */
    FOCUS_ROW(FrameSpec.rounded(4, 0, 0).withRing(Theme.GOLD_300, 1.5f)),
    /** An address shown in a question (a server, a link): read, not pressed, so rounded. */
    ADDRESS(FrameSpec.rounded(4, Theme.withAlpha(Theme.OBSIDIAN_700, 0.95f), Theme.withAlpha(Theme.GOLD_400, 0.4f))
        .fixed(14)),
    /**
     * A crew tag's chip (TEAM6): white, tinted at draw time with the crew colour at 24 % (one frame for every colour),
     * chamfered like the launcher's {@code TeamTag}.
     */
    TAG(FrameSpec.rounded(2, 0xFFFFFFFF, 0).chamfer()),
    /** The chip at its two heights, baked whole down so it draws in three slices (a toast's budget is tight). */
    TAG_9(FrameSpec.rounded(2, 0xFFFFFFFF, 0).chamfer().fixed(9)),
    TAG_8(FrameSpec.rounded(2, 0xFFFFFFFF, 0).chamfer().fixed(8)),
    /** The send button inside the message field: rounded (the owner's wish for the chat), gold once ready. */
    SEND_READY(FrameSpec.rounded(4, 0, Theme.GOLD_600)
        .withGradient(Theme.mix(Theme.GOLD_300, Theme.GOLD_400, 0.45f), Theme.GOLD_600).fixed(18)),
    SEND_HOVER(FrameSpec.rounded(4, Theme.withAlpha(0xFFFFFF, 0.07f), 0).fixed(18)),
    FOCUS_18(FrameSpec.rounded(4, 0, 0).withRing(Theme.GOLD_300, 1.5f).fixed(18)),
    /** A keycap ("F7"): raised, with a darker lip. */
    KEYCAP(FrameSpec.rounded(2, Theme.OBSIDIAN_700, Theme.withAlpha(0xFFFFFF, 0.13f))
        .withShadow(Theme.withAlpha(Theme.OBSIDIAN_950, 0.9f), 0.5f, 1f).fixed(10)),
    /** Counts ("3"). */
    PILL(FrameSpec.rounded(4.5f, Theme.OBSIDIAN_700, Theme.withAlpha(0xFFFFFF, 0.10f)).fixed(9)),
    PILL_GOLD(FrameSpec.rounded(4.5f, Theme.GOLD_500, 0).fixed(9)),
    TOGGLE_OFF(FrameSpec.rounded(5, Theme.OBSIDIAN_700, Theme.withAlpha(0xFFFFFF, 0.14f)).fixed(10)),
    TOGGLE_ON(FrameSpec.rounded(5, Theme.GOLD_500, Theme.GOLD_600).withGradient(Theme.GOLD_400, Theme.GOLD_500).fixed(10)),
    FOCUS_10(FrameSpec.rounded(5, 0, 0).withRing(Theme.GOLD_300, 1.5f).fixed(10)),
    SLIDER_TRACK(FrameSpec.rounded(1.5f, Theme.OBSIDIAN_700, Theme.withAlpha(0xFFFFFF, 0.08f)).fixed(3)),
    SLIDER_FILL(FrameSpec.rounded(1.5f, Theme.GOLD_500, 0).fixed(3)),
    /** The segmented control's well and its gliding gold plate. */
    SEGMENT_WELL(FrameSpec.rounded(3, Theme.withAlpha(Theme.OBSIDIAN_950, 0.55f), Theme.withAlpha(0xFFFFFF, 0.09f))
        .chamfer().fixed(16)),
    SEGMENT_PLATE(FrameSpec.rounded(2.5f, 0, Theme.GOLD_600)
        .withGradient(Theme.mix(Theme.GOLD_300, Theme.GOLD_400, 0.45f), Theme.GOLD_600).chamfer().fixed(12)),
    /**
     * CHAT2: small surfaces over the conversation (a message's toolbar, the quick reactions, the sticker
     * picker, a tooltip): opaque, since text under text never reads well (even 3 % of a white sticker's
     * outline showed through on a dark screen), with a soft shadow.
     */
    POPOVER(FrameSpec.rounded(5, Theme.OBSIDIAN_850, Theme.withAlpha(0xFFFFFF, 0.11f))
        .withHighlight(Theme.withAlpha(0xFFFFFF, 0.05f))
        .withShadow(Theme.withAlpha(0x0A0408, 0.55f), 5f, 2f)),
    TOOLTIP(FrameSpec.rounded(4, Theme.OBSIDIAN_950, Theme.withAlpha(0xFFFFFF, 0.12f))
        .withShadow(Theme.withAlpha(0x0A0408, 0.5f), 4f, 1.5f)),
    /** A reaction under a message: an action, so chamfered; the player's own carries a gold edge. */
    REACTION(FrameSpec.rounded(2.5f, Theme.withAlpha(0xFFFFFF, 0.06f), Theme.withAlpha(0xFFFFFF, 0.10f))
        .chamfer().fixed(12)),
    REACTION_HOVER(FrameSpec.rounded(2.5f, Theme.withAlpha(0xFFFFFF, 0.11f), Theme.withAlpha(0xFFFFFF, 0.18f))
        .chamfer().fixed(12)),
    REACTION_MINE(FrameSpec.rounded(2.5f, Theme.withAlpha(Theme.mix(Theme.OBSIDIAN_700, Theme.GOLD_500, 0.2f), 0.95f),
        Theme.withAlpha(Theme.GOLD_400, 0.55f)).chamfer().fixed(12)),
    FOCUS_12(FrameSpec.rounded(2.5f, 0, 0).withRing(Theme.GOLD_300, 1.5f).chamfer().fixed(12)),
    /** A message picked with the keyboard, and the moment a quote takes the player to its message. */
    FOCUS_BUBBLE(FrameSpec.rounded(5, 0, 0).withRing(Theme.GOLD_300, 1.5f)),
    FLASH(FrameSpec.rounded(5, Theme.withAlpha(Theme.GOLD_400, 0.16f), Theme.withAlpha(Theme.GOLD_400, 0.45f))),
    /** A sticker's place while its picture is on its way, and a sticker of a set this build does not have. */
    STICKER_TILE(FrameSpec.rounded(6, Theme.withAlpha(0xFFFFFF, 0.045f), Theme.withAlpha(0xFFFFFF, 0.07f))),
    /** A cell of the sticker picker or the quick reactions under the pointer, or picked with the keyboard. */
    CELL_HOVER(FrameSpec.rounded(3, Theme.withAlpha(0xFFFFFF, 0.08f), Theme.withAlpha(0xFFFFFF, 0.12f)).chamfer()),
    CELL_FOCUS(FrameSpec.rounded(3, 0, 0).withRing(Theme.GOLD_300, 1.5f).chamfer());

    /** The toast card's width in GUI pixels. */
    public static final int TOAST_WIDTH = Sizes.TOAST_WIDTH;

    public final FrameSpec spec;
    private final String key;

    Frames(FrameSpec spec) {
        this.spec = spec;
        this.key = "frame:" + name();
    }

    /** The frame's name in the atlas (made once: frames are looked up every frame). */
    public String key() {
        return key;
    }

    /** Sizes the constants above need before the enum's own static fields exist. */
    private static final class Sizes {
        static final int TOAST_WIDTH = 184;
    }

    /** Surface opacities shared with the panel. */
    public static final class Alphas {
        /** Toast cards over the open world. */
        public static final float TOAST = 0.80f;
        /** The panel, over the scrim that already dims the world a little. */
        public static final float PANEL = 0.88f;
        /** A question over the dimmed game: read word by word, so denser than a toast. */
        public static final float DIALOG = 0.92f;

        private Alphas() {
        }
    }
}
