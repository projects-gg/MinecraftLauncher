package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import gg.projects.overlay.voice.VoiceState;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * Crew chat's "Sesli" (OVL-SOC; the owner: "ekip sohbetindeki odaları falan görebilsek"; the launcher's crew page
 * {@code crew-voice.tsx}): the crew's voice rooms as one row of pills under the conversation chips, in the launcher's
 * order and names, each with its headset (the launcher's "Katıl" carries one: a voice room, a press joins it), the faces
 * of who is in it (three, then "+2") and, in the player's own room, the jade ring of whoever speaks (in 100 ms, out
 * 300 ms; opacity only). With nobody in any room the crew's "Genel" stands alone. A press on a room joins it ("Katıl");
 * the player's own room sits on the quiet plate with a jade headset (never gold: a chosen chip is never gold) and its
 * press opens the room page; "+ Yeni oda" ends the row while the crew may open one more. Renaming, closing and moving
 * rooms stay in the launcher. A room the crew gave its own people limit says how full it is after the faces ("3/5",
 * the crew page's count; OVL-SOC 2), and a full one says so: its words and headset quiet, its press says "Oda dolu."
 * and asks nothing.
 *
 * <p>Keys as the conversation chips: ← and → pick a pill, Enter or Space presses it. Too many pills for the width scroll
 * sideways with the wheel, fading at the ends that have more. Nothing about it runs while it is hidden.
 */
public final class CrewVoiceStrip extends PanelWidget {
    public static final int HEIGHT = ChipRow.HEIGHT;
    private static final int GAP = 4;
    private static final int PAD = 6;
    /** A pill's sign (its headset, "Yeni oda"'s plus) and the gap after it. */
    private static final int SIGN = 8;
    private static final int SIGN_GAP = 3;
    /** Room before the first pill (none: each pill carries its own sign). */
    private static final int SIGN_ROOM = 0;
    private static final int FACE = 10;
    private static final int FACE_GAP = 2;
    private static final int FACES = 3;
    private static final long RING_IN_MS = 100;
    private static final long RING_OUT_MS = 300;

    /** What a pill does. */
    public interface Actions {
        /** "Katıl" on a room of {@code crew}. */
        void join(String crew, String room);

        /** The player's own room: its page. */
        void open();

        /** "Yeni oda" in {@code crew}. */
        void newRoom(String crew);

        /** A full room was pressed (OVL-SOC 2): "Oda dolu.", nothing asked. */
        void full();
    }

    /** A pill: a room ({@code add} false) or "Yeni oda". */
    private record Pill(String id, boolean add, boolean here, FormattedCharSequence label, int labelWidth,
        List<String> faces, String more, int moreWidth, String count, int countWidth, boolean full, int width,
        Component narration) {
    }

    private final Actions actions;
    private final List<Pill> pills = new ArrayList<>();
    private final Map<String, Face> faces = new HashMap<>();
    private final Map<String, Tween> rings = new HashMap<>();
    private final Spring scroll = Spring.snappy(0f);
    private String crew;
    private String focusedId;
    private float contentWidth;
    private long lastFrame;
    private boolean reducedMotion;

    public CrewVoiceStrip(Actions actions) {
        super(0, 0, 100, HEIGHT, Texts.tr("voice.crew.title"));
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** The crew it shows (the scenario driver presses its pills), or null. */
    public String crew() {
        return crew;
    }

    /**
     * What it shows: {@code rooms}' pills laid out for the strip's width, the player's own room lit, faces named from the
     * room's people ({@code words}: "Genel", "Bir oyuncu").
     */
    public void show(VoiceState.CrewRooms rooms, VoiceState voice, VoiceState.Words words, long now) {
        if (!rooms.crew().equals(crew)) {
            // Another crew's chat: its own row from the start.
            scroll.snap(0f);
            focusedId = null;
        }
        crew = rooms.crew();
        pills.clear();
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        Set<String> present = new HashSet<>();
        Set<String> drawn = new HashSet<>();
        for (VoiceState.CrewRoom room : rooms.rooms()) {
            String name = room.general() || room.name() == null ? words.general() : room.name();
            String fitted = Texts.fit(ui, name, 80);
            boolean here = voice.inRoom(room.id());
            List<String> ids = new ArrayList<>(FACES);
            List<String> names = new ArrayList<>(room.people().size());
            for (VoiceState.Named person : room.people()) {
                String shown = VoiceState.shown(person.name(), words);
                names.add(shown);
                present.add(person.id());
                if (ids.size() < FACES) {
                    ids.add(person.id());
                    drawn.add(person.id());
                    Face face = faces.get(person.id());
                    if (face == null || !Texts.initial(shown).equals(face.letter())) {
                        faces.put(person.id(), Face.of(shown));
                    }
                }
            }
            String more = room.people().size() > FACES ? "+" + (room.people().size() - FACES) : null;
            // OVL-SOC 2: "3/5" where the crew gave the room its own limit (the crew page's count).
            String count = room.max() > 0 ? room.people().size() + "/" + room.max() : null;
            boolean full = room.full() && !here;
            int labelWidth = ui.width(fitted);
            int moreWidth = more == null ? 0 : small.width(more);
            int countWidth = count == null ? 0 : small.width(count);
            int width = PAD + SIGN + SIGN_GAP + labelWidth
                + (ids.isEmpty() ? 0 : 5 + ids.size() * (FACE + FACE_GAP) - FACE_GAP)
                + (more == null ? 0 : 3 + moreWidth) + (count == null ? 0 : 5 + countWidth) + PAD;
            // "Genel: Ayşe, Cem, Ece. Katılmak için Enter'a bas." ("Bu odadasın" for the player's own, "Oda dolu."
            // for a full one; "3 kişi, en çok 5" with its own limit).
            String who = names.isEmpty() ? Texts.str("voice.crew.nobody") : VoiceState.names(names, words);
            if (count != null) {
                who = Texts.str("voice.crew.count.narration", who, room.people().size(), room.max());
            }
            String key = here ? "voice.crew.here.narration" : full ? "voice.crew.full.narration"
                : "voice.crew.room.narration";
            Component narration = Texts.tr(key, name, who);
            pills.add(new Pill(room.id(), false, here, Texts.visual(fitted), labelWidth, List.copyOf(ids), more,
                moreWidth, count, countWidth, full, width, narration));
        }
        if (rooms.add()) {
            String label = Texts.str("voice.crew.new_room");
            int labelWidth = ui.width(label);
            pills.add(new Pill(null, true, false, Texts.visual(label), labelWidth, List.of(), null, 0, null, 0,
                false, PAD + SIGN + SIGN_GAP + labelWidth + PAD, Texts.tr("voice.crew.new_room")));
        }
        faces.keySet().retainAll(drawn);
        rings.keySet().retainAll(present);
        float x = 0f;
        for (Pill p : pills) {
            x += p.width + GAP;
        }
        contentWidth = Math.max(0f, x - GAP);
        if (focusedId == null || indexOf(focusedId) < 0) {
            focusedId = pills.isEmpty() ? null : key(pills.get(0));
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), scroll.target())));
        speaking(voice, now);
    }

    /** Who speaks changed: only the rings of the player's own room move (others' speech never comes here). */
    public void speaking(VoiceState voice, long now) {
        VoiceState.Room room = voice.room();
        for (Pill p : pills) {
            if (p.add) {
                continue;
            }
            // The one face of the player's room that the room's own list lacks is the player's (two or more: a list
            // a moment behind the other, so none rings for the player).
            String me = null;
            if (p.here && room != null) {
                for (String id : p.faces) {
                    if (room.person(id) == null) {
                        me = me == null ? id : "";
                    }
                }
            }
            for (String id : p.faces) {
                boolean on = false;
                if (p.here && room != null) {
                    VoiceState.Person person = room.person(id);
                    on = person != null ? person.speaking() : id.equals(me) && room.me().talking();
                }
                Tween ring = rings.computeIfAbsent(id, k -> new Tween(0f));
                float target = on ? 1f : 0f;
                if (ring.target() != target) {
                    ring.animateTo(target, now, on ? RING_IN_MS : RING_OUT_MS, Ease.OUT);
                }
            }
        }
    }

    private static String key(Pill p) {
        return p.add ? "+" : p.id;
    }

    private int indexOf(String key) {
        for (int i = 0; i < pills.size(); i++) {
            if (key(pills.get(i)).equals(key)) {
                return i;
            }
        }
        return -1;
    }

    private float maxScroll() {
        return Math.max(0f, SIGN_ROOM + contentWidth - getWidth());
    }

    private float pillX(int index) {
        float x = getX() + SIGN_ROOM - scroll.value();
        for (int i = 0; i < index; i++) {
            x += pills.get(i).width + GAP;
        }
        return x;
    }

    private int pillAt(double mx) {
        float x = getX() + SIGN_ROOM - scroll.value();
        for (int i = 0; i < pills.size(); i++) {
            Pill p = pills.get(i);
            if (mx >= x && mx < x + p.width && mx >= getX() + SIGN_ROOM - 2) {
                return i;
            }
            x += p.width + GAP;
        }
        return -1;
    }

    private void reveal(int index) {
        float left = SIGN_ROOM;
        for (int i = 0; i < index; i++) {
            left += pills.get(i).width + GAP;
        }
        float right = left + pills.get(index).width;
        float target = scroll.target();
        if (left - SIGN_ROOM < target) {
            target = left - SIGN_ROOM;
        } else if (right > target + getWidth()) {
            target = right - getWidth() + 2;
        }
        target = Math.max(0f, Math.min(maxScroll(), target));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    private void press(int index) {
        Pill p = pills.get(index);
        focusedId = key(p);
        pressed();
        if (p.add) {
            actions.newRoom(crew);
        } else if (p.here) {
            actions.open();
        } else if (p.full) {
            actions.full();
        } else {
            actions.join(crew, p.id);
        }
    }

    /** Presses the pill of room {@code id} ("+": "Yeni oda") as a click would (the scenario driver); false without it. */
    public boolean pressRoom(String id) {
        int i = indexOf(id);
        if (i < 0) {
            return false;
        }
        press(i);
        return true;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int i = pillAt(event.x());
        if (i >= 0 && !doubleClick) {
            press(i);
        }
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (!visible) {
            // Hidden (the room page opened over it) while it still had the keys: it does nothing.
            return false;
        }
        int i = focusedId == null ? -1 : indexOf(focusedId);
        if (i < 0) {
            return false;
        }
        if (event.isLeft() && i > 0) {
            PanelInput.usedKeyboard();
            focusedId = key(pills.get(i - 1));
            reveal(i - 1);
            return true;
        }
        if (event.isRight() && i < pills.size() - 1) {
            PanelInput.usedKeyboard();
            focusedId = key(pills.get(i + 1));
            reveal(i + 1);
            return true;
        }
        if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            press(i);
            return true;
        }
        return false;
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY) || maxScroll() <= 0f) {
            return false;
        }
        float delta = (float) (scrollX != 0 ? -scrollX : -scrollY) * 30f;
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() + delta));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
        return true;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        g.enableScissor(x0 + SIGN_ROOM - 2, y0 - 3, x0 + w + 2, y0 + HEIGHT + 3);
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        int lit = isHovered() ? pillAt(mouseX) : -1;
        float x = x0 + SIGN_ROOM - scroll.value();
        float textY = Fonts.centeredY(Fonts.Face.UI, y0 + HEIGHT / 2f);
        for (int i = 0; i < pills.size(); i++) {
            Pill p = pills.get(i);
            if (x + p.width >= x0 - 2 && x <= x0 + w + 2) {
                boolean hot = i == lit;
                Frames frame = p.here || hot ? Frames.CHIP_HOVER : Frames.CHIP;
                Draw.frame(g, frame, x, y0, p.width, HEIGHT);
                if (keyboardFocus() && key(p).equals(focusedId)) {
                    Draw.frame(g, Frames.FOCUS_14, x, y0, p.width, HEIGHT);
                }
                if (hot) {
                    g.requestCursor(CursorTypes.POINTING_HAND);
                }
                // A full room is quiet (its press only says "Oda dolu."); hovered, it lights a step only.
                int ink = p.here ? Theme.BONE_50 : p.full ? (hot ? Theme.ASH_300 : Theme.ASH_400)
                    : hot ? Theme.BONE_100 : Theme.ASH_300;
                float cx = x + PAD;
                // The headset says "a voice room" (jade: the player is in it); "Yeni oda" has its plus.
                int sign = p.add ? ink : p.here ? Theme.JADE_400 : p.full ? Theme.ASH_500
                    : hot ? Theme.BONE_100 : Theme.ASH_400;
                Draw.icon(g, p.add ? Icon.PLUS : Icon.HEADSET, SIGN, cx, y0 + (HEIGHT - SIGN) / 2f, sign);
                cx += SIGN + SIGN_GAP;
                Draw.text(g, ui, p.label, cx, textY, ink);
                cx += p.labelWidth;
                if (!p.faces.isEmpty()) {
                    cx += 5;
                    float fy = y0 + (HEIGHT - FACE) / 2f;
                    for (String id : p.faces) {
                        Face face = faces.get(id);
                        if (face != null) {
                            face.draw(g, FACE, cx + FACE / 2f, fy + FACE / 2f);
                            Tween ring = rings.get(id);
                            float spoke = ring == null ? 0f : ring.value(now);
                            if (spoke > 0.004f) {
                                Draw.pushAlpha(spoke);
                                Draw.frame(g, Frames.SPEAKING, Draw.snap(cx), Draw.snap(fy), FACE, FACE);
                                Draw.popAlpha();
                            }
                        }
                        cx += FACE + FACE_GAP;
                    }
                    cx -= FACE_GAP;
                }
                if (p.more != null) {
                    Draw.text(g, small, p.more, cx + 3, Fonts.centeredY(Fonts.Face.SMALL, y0 + HEIGHT / 2f),
                        Theme.ASH_300);
                    cx += 3 + p.moreWidth;
                }
                if (p.count != null) {
                    Draw.text(g, small, p.count, cx + 5, Fonts.centeredY(Fonts.Face.SMALL, y0 + HEIGHT / 2f),
                        Theme.ASH_400);
                }
            }
            x += p.width + GAP;
        }
        g.disableScissor();
        if (scroll.value() > 0.5f) {
            Draw.strip(g, ShapeSet.FADE_LEFT, x0 + SIGN_ROOM - 2, y0 - 2, ShapeSet.FADE_WIDTH, HEIGHT + 4);
        }
        if (scroll.value() < maxScroll() - 0.5f) {
            Draw.strip(g, ShapeSet.FADE_RIGHT, x0 + w + 2 - ShapeSet.FADE_WIDTH, y0 - 2, ShapeSet.FADE_WIDTH,
                HEIGHT + 4);
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        int i = focusedId == null ? -1 : indexOf(focusedId);
        if (i >= 0) {
            output.add(NarratedElementType.HINT, pills.get(i).narration);
        }
    }
}
