package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.voice.VoiceMarks;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * The room strip's body on the panel's chats and friends pages (VOICE-O; voice.md §5.3): the headset, the room's title
 * and, while it is not live, how it stands ("Bağlanıyor…", "Yeniden bağlanıyor…"); under them the faces of the others,
 * each with its jade ring while they speak (in 100 ms, out 300 ms; opacity only) and, when their microphone or sound is
 * off, its sign on the face's corner (VOICE-KEYS), and "+3" for those who do not fit. A press, Enter or Space opens the
 * room page. The strip's three buttons are the panel's own widgets beside it.
 */
public final class VoiceStrip extends PanelWidget {
    static final int FACE = 12;
    static final int FACE_GAP = 4;
    /**
     * A face's mark: an 8 px sign on a 10 px dark disc whose centre is 1 px in from the face's corner (OVL-AVATARS: on a
     * picture it covers the corner, not the eyes).
     */
    static final int MARK = 8;
    static final int MARK_DISC = 10;
    private static final long RING_IN_MS = 100;
    private static final long RING_OUT_MS = 300;

    /** Someone's face in the strip; {@code mark} null while their microphone and sound are on. */
    public record Person(String id, String name, boolean speaking, VoiceMarks.Mark mark) {
    }

    private final Runnable onOpen;
    private FormattedCharSequence title;
    private FormattedCharSequence state;
    private float stateWidth;
    private final List<String> ids = new ArrayList<>();
    private final Map<String, Face> faces = new HashMap<>();
    private final Map<String, VoiceMarks.Mark> marks = new HashMap<>();
    private final Map<String, Tween> rings = new HashMap<>();
    private String more;
    private Component narration = Component.empty();

    public VoiceStrip(Runnable onOpen) {
        super(0, 0, 100, 32, Texts.tr("voice.room.open"));
        this.onOpen = onOpen;
    }

    /**
     * What it shows: the title, how the room stands ({@code null} while live), the others with whether they speak (as
     * many as fit), laid out for the strip's width.
     */
    public void show(String titleText, String stateText, List<Person> people, long now) {
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int w = getWidth();
        stateWidth = stateText == null ? 0 : small.width(stateText);
        title = Texts.visual(Texts.fit(strong, titleText, Math.max(30, Math.round(w - 10 - 5 - stateWidth - 6))));
        state = stateText == null ? null : Texts.visual(stateText);
        int fit = Math.max(1, (w + FACE_GAP) / (FACE + FACE_GAP));
        int shown = people.size() <= fit ? people.size() : Math.max(0, fit - 1);
        ids.clear();
        marks.clear();
        List<String> names = new ArrayList<>(people.size());
        for (int i = 0; i < people.size(); i++) {
            Person p = people.get(i);
            names.add(p.name);
            Tween ring = rings.computeIfAbsent(p.id, k -> new Tween(0f));
            float target = p.speaking ? 1f : 0f;
            if (ring.target() != target) {
                ring.animateTo(target, now, p.speaking ? RING_IN_MS : RING_OUT_MS, Ease.OUT);
            }
            if (i < shown) {
                ids.add(p.id);
                Face face = faces.get(p.id);
                if (face == null || !face.is(p.name, p.id)) {
                    faces.put(p.id, Face.of(p.name, p.id));
                }
                if (p.mark != null) {
                    marks.put(p.id, p.mark);
                }
            }
        }
        more = people.size() > shown ? "+" + (people.size() - shown) : null;
        faces.keySet().retainAll(ids);
        java.util.Set<String> present = new java.util.HashSet<>();
        for (Person p : people) {
            present.add(p.id);
        }
        rings.keySet().retainAll(present);
        narration = Texts.tr("voice.person.narration", titleText,
            stateText != null ? stateText : String.join(", ", names));
    }

    /** Who speaks changed: only the rings move. */
    public void speaking(List<Person> people, long now) {
        for (Person p : people) {
            Tween ring = rings.get(p.id);
            float target = p.speaking ? 1f : 0f;
            if (ring != null && ring.target() != target) {
                ring.animateTo(target, now, p.speaking ? RING_IN_MS : RING_OUT_MS, Ease.OUT);
            }
        }
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        pressed();
        onOpen.run();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (active && visible && (event.isSelection() || event.isConfirmation())) {
            PanelInput.usedKeyboard();
            pressed();
            onOpen.run();
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float x = getX();
        float y = getY();
        float h = hover.value(now);
        if (h > 0f) {
            Draw.pushAlpha(h);
            Draw.frame(g, Frames.ROW_HOVER, x - 3, y - 2, getWidth() + 6, getHeight() + 4, 0x99FFFFFF);
            Draw.popAlpha();
        }
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_ROW, x - 3, y - 2, getWidth() + 6, getHeight() + 4);
        }
        Draw.icon(g, Icon.HEADSET, 10, x, y, Theme.ASH_300);
        if (title != null) {
            Draw.text(g, Fonts.strong(), title, x + 15, Fonts.centeredY(Fonts.Face.STRONG, y + 5), Theme.BONE_50);
        }
        if (state != null) {
            Draw.text(g, Fonts.small(), state, x + getWidth() - stateWidth, Fonts.centeredY(Fonts.Face.SMALL, y + 5),
                Theme.ASH_300);
        }
        float fy = y + 14;
        float fx = x;
        for (String id : ids) {
            Face face = faces.get(id);
            if (face == null) {
                continue;
            }
            face.draw(g, FACE, fx + FACE / 2f, fy + FACE / 2f);
            Tween ring = rings.get(id);
            float spoke = ring == null ? 0f : ring.value(now);
            if (spoke > 0.004f) {
                Draw.pushAlpha(spoke);
                Draw.frame(g, Frames.SPEAKING, Draw.snap(fx), Draw.snap(fy), FACE, FACE);
                Draw.popAlpha();
            }
            VoiceMarks.Mark mark = marks.get(id);
            if (mark != null) {
                float cx = fx + FACE - 1;
                float cy = fy + FACE - 1;
                Draw.disc(g, MARK_DISC, cx, cy, Theme.OBSIDIAN_950);
                Draw.icon(g, mark.icon(), MARK, cx - MARK / 2f, cy - MARK / 2f, mark.ink());
            }
            fx += FACE + FACE_GAP;
        }
        if (more != null) {
            Draw.text(g, Fonts.small(), more, fx, Fonts.centeredY(Fonts.Face.SMALL, fy + FACE / 2f), Theme.ASH_300);
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(net.minecraft.client.gui.narration.NarratedElementType.TITLE, narration);
        output.add(net.minecraft.client.gui.narration.NarratedElementType.USAGE, getMessage());
    }
}
