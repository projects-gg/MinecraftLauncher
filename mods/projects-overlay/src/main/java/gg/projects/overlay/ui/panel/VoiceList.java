package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Avatars;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.LiveChip;
import gg.projects.overlay.ui.LiveTip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import gg.projects.overlay.voice.VoiceMarks;
import gg.projects.overlay.voice.VoiceState;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * The people of the player's voice room on the room page (VOICE-O; the launcher's room panel, voice.md §5.1): the player
 * first ("Sen"), then everyone else as they came, then the player's invites that still ring ("Aranıyor…"). A row is a
 * face with its jade ring while they speak (in 100 ms, out 300 ms; opacity only), the name, "Lider" or "Moderatör" under
 * it, and on the right the signs of what is off: the microphone (flare when a moderator muted them), hearing, "Benim
 * için sustur", a weak connection (the server bars' colours). A press or a right click on someone opens their menu
 * (volume first); the arrows move, Enter, Space, Shift+F10 or the menu key open it. A note under the rows says how the
 * last action went, as the friend list's.
 */
public final class VoiceList extends PanelWidget {
    /** Opens someone's menu. */
    public interface Actions {
        void menu(String id, String name, PersonMenu.Anchor anchor, boolean keys);

        /** SHARE-LIVE: the card on someone's "YAYINDA" opened: asks the launcher for share {@code n}'s picture. */
        default void preview(String room, String person, int n) {
        }
    }

    static final int ROW_H = 22;
    private static final int FACE = 16;
    private static final int MARK = 10;
    private static final int MARK_GAP = 3;
    private static final int NOTE_H = 14;
    private static final long RING_IN_MS = 100;
    private static final long RING_OUT_MS = 300;

    private static final class Row {
        final String id;
        boolean me;
        boolean ringing;
        String label = "";
        FormattedCharSequence name;
        FormattedCharSequence sub;
        Face face;
        final List<Icon> marks = new ArrayList<>(4);
        final List<Integer> markInks = new ArrayList<>(4);
        /** Bars to fill of three (a weak connection: 2, a bad one: 1), 0 for none. */
        int bars;
        /** SHARE-LIVE: they share their screen: "YAYINDA" after the name. */
        boolean live;
        float nameWidth;
        Component narration;
        float y;
        final Tween ring = new Tween(0f);

        Row(String id) {
            this.id = id;
        }
    }

    private final Actions actions;
    private final List<Row> rows = new ArrayList<>();
    // SHARE-LIVE: "YAYINDA" and the card the pointer opens on it.
    private final LiveChip liveChip = new LiveChip();
    private final LiveTip liveTip = new LiveTip();
    private VoiceState voice;
    private final Spring scroll = Spring.snappy(0f);
    private String selected;
    private boolean reducedMotion;
    private long lastFrame;
    private float contentHeight;
    // The note under the rows.
    private OverlayState.PersonNote note;
    private FormattedCharSequence noteText;
    private float noteWidth;
    private boolean noteError;
    private final Tween noteAlpha = new Tween(0f);

    public VoiceList(Actions actions) {
        super(0, 0, 100, 100, Texts.tr("voice.room.people"));
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** Whether {@code id} has a row (someone in the room, not the player). */
    public boolean has(String id) {
        for (Row r : rows) {
            if (!r.me && !r.ringing && r.id.equals(id)) {
                return true;
            }
        }
        return false;
    }

    /** The rows for {@code room}: the player ({@code meName}), the others, the invites that ring. */
    public void show(VoiceState.Room room, String meName, VoiceState.Words words, long now) {
        show(room, meName, words, null, now);
    }

    /**
     * As {@link #show(VoiceState.Room, String, VoiceState.Words, long)}; {@code voice} (OVL-SHARE) says who shares their
     * screen: SHARE-LIVE's "YAYINDA" right after their name (the player's own too), the one live sign of the row.
     */
    public void show(VoiceState.Room room, String meName, VoiceState.Words words, VoiceState voice, long now) {
        this.voice = voice;
        Map<String, Row> old = new HashMap<>();
        for (Row r : rows) {
            old.put((r.ringing ? "~" : "") + r.id, r);
        }
        List<Row> next = new ArrayList<>();
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int textW = Math.max(40, getWidth() - FACE - 8 - 4 * (MARK + MARK_GAP));
        VoiceState.Me me = room.me();
        Row mine = old.getOrDefault("#me", new Row("#me"));
        mine.me = true;
        mine.live = voice != null && voice.live();
        String own = meName == null || meName.isEmpty() ? Texts.str("voice.room.you") : meName;
        // "Sen", and the player's role beside it ("Sen · Lider").
        String you = Texts.str("voice.room.you");
        String role = roleOr(me.role(), null);
        fill(mine, own, Avatars.ME, role == null ? you : you + " · " + role, strong, small, textW);
        mine.marks.clear();
        mine.markInks.clear();
        voiceMarks(mine, me.mute(), me.deaf(), me.mutedByModerator());
        mine.bars = bars(me.quality());
        mine.narration = narration(own, state(mine, me.talking()));
        ring(mine, me.talking(), now);
        next.add(mine);
        for (VoiceState.Person p : room.people()) {
            Row row = old.getOrDefault(p.id(), new Row(p.id()));
            row.live = voice != null && voice.sharing(p.id());
            String name = VoiceState.shown(p.name(), words);
            fill(row, name, p.id(), roleOr(p.role(), null), strong, small, textW);
            row.marks.clear();
            row.markInks.clear();
            voiceMarks(row, p.mute(), p.deaf(), p.mutedByModerator());
            if (p.mutedForMe()) {
                mark(row, Icon.MUTE_FOR_ME, Theme.ASH_300);
            }
            row.bars = bars(p.quality());
            row.narration = narration(name, state(row, p.speaking()));
            ring(row, p.speaking(), now);
            next.add(row);
        }
        for (VoiceState.Named ring : room.ringing()) {
            Row row = old.getOrDefault("~" + ring.id(), new Row(ring.id()));
            row.ringing = true;
            row.live = false;
            String name = VoiceState.shown(ring.name(), words);
            fill(row, name, ring.id(), Texts.str("voice.ringing"), strong, small, textW);
            row.marks.clear();
            row.markInks.clear();
            row.bars = 0;
            row.narration = narration(name, Texts.str("voice.ringing"));
            next.add(row);
        }
        float y = 0f;
        for (Row r : next) {
            r.y = y;
            y += ROW_H;
        }
        rows.clear();
        rows.addAll(next);
        contentHeight = y;
        if (selected != null && !has(selected)) {
            selected = null;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), scroll.target())));
    }

    /** Who speaks changed: only the rings move (no layout). */
    public void speaking(VoiceState.Room room, long now) {
        for (Row r : rows) {
            if (r.me) {
                ring(r, room.me().talking(), now);
            } else if (!r.ringing) {
                VoiceState.Person p = room.person(r.id);
                ring(r, p != null && p.speaking(), now);
            }
        }
    }

    private void ring(Row row, boolean speaking, long now) {
        float target = speaking ? 1f : 0f;
        if (row.ring.target() != target) {
            row.ring.animateTo(target, now, speaking ? RING_IN_MS : RING_OUT_MS, Ease.OUT);
        }
    }

    /** "Kaan_07, Konuşuyor, Mikrofonu kapalı", or the name alone. */
    private static Component narration(String name, String state) {
        return state.isEmpty() ? Component.literal(name) : Texts.tr("voice.person.narration", name, state);
    }

    /**
     * A row's name (it gives way to "YAYINDA" after it), the line under it and the face of {@code person} (a user id;
     * {@link Avatars#ME} the player).
     */
    private void fill(Row row, String label, String person, String sub, Font strong, Font small, int textW) {
        row.label = label;
        String fitted = Texts.fit(strong, label, Math.max(20, textW - (int) Math.ceil(liveChip.room(row.live))));
        row.name = Texts.visual(fitted);
        row.nameWidth = strong.width(fitted);
        row.sub = sub == null ? null : Texts.visual(Texts.fit(small, sub, textW));
        if (row.face == null || !row.face.is(label, person)) {
            row.face = Face.of(label, person);
        }
    }

    private static void mark(Row row, Icon icon, int ink) {
        row.marks.add(icon);
        row.markInks.add(ink);
    }

    /**
     * What is off, in one sign each (VOICE-KEYS, the launcher's room list; nothing while the microphone and the sound
     * are on): a moderator's mute in flare (it holds whatever they do), then deafened alone (the microphone is off
     * with it, as Discord shows it), else the microphone, in ash.
     */
    private static void voiceMarks(Row row, boolean mute, boolean deaf, boolean mutedByModerator) {
        if (mutedByModerator) {
            mark(row, Icon.MIC_OFF, Theme.FLARE_300);
        }
        Icon own = VoiceMarks.own(mute, deaf);
        if (own != null) {
            mark(row, own, Theme.ASH_300);
        }
    }

    private static String roleOr(String role, String fallback) {
        return switch (role) {
            case "leader" -> Texts.str("voice.room.leader");
            case "moderator" -> Texts.str("voice.room.moderator");
            default -> fallback;
        };
    }

    private static int bars(String quality) {
        return switch (quality) {
            case "weak" -> 2;
            case "bad" -> 1;
            default -> 0;
        };
    }

    /** What the narrator says after the name: "Yayında", speaking, then what is off. */
    private static String state(Row row, boolean speaking) {
        List<String> parts = new ArrayList<>(4);
        if (row.live) {
            parts.add(Texts.str("share.live"));
        }
        if (speaking) {
            parts.add(Texts.str("voice.person.speaking"));
        }
        for (int i = 0; i < row.marks.size(); i++) {
            Icon icon = row.marks.get(i);
            parts.add(Texts.str(icon == Icon.MIC_OFF
                ? (row.markInks.get(i) == Theme.FLARE_300 ? "voice.person.muted_for_all" : "voice.person.mic_off")
                : icon == Icon.DEAF ? "voice.person.deafened" : "voice.person.muted_for_me"));
        }
        if (row.bars > 0) {
            parts.add(Texts.str("voice.person.weak"));
        }
        if (parts.isEmpty() && row.sub != null) {
            return row.me ? Texts.str("voice.room.you") : "";
        }
        return String.join(", ", parts);
    }

    /** How the last action went (null: nothing to say). */
    public void setNote(OverlayState.PersonNote next, long now) {
        if (next == note) {
            return;
        }
        note = next;
        if (next != null) {
            String words = FriendList.noteWords(next);
            Font small = Fonts.small();
            String fitted = Texts.fit(small, words, Math.max(40, getWidth() - 12));
            noteText = Texts.visual(fitted);
            noteWidth = small.width(fitted) + 12;
            noteError = next.error();
            noteAlpha.animateTo(1f, now, 160, Ease.OUT);
        } else {
            noteAlpha.animateTo(0f, now, 110, Ease.OUT);
        }
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - getHeight());
    }

    private List<Row> people() {
        List<Row> out = new ArrayList<>();
        for (Row r : rows) {
            if (!r.me && !r.ringing) {
                out.add(r);
            }
        }
        return out;
    }

    private Row selectedRow() {
        for (Row r : people()) {
            if (r.id.equals(selected)) {
                return r;
            }
        }
        return null;
    }

    private void move(int direction) {
        List<Row> live = people();
        if (live.isEmpty()) {
            return;
        }
        int i = -1;
        for (int k = 0; k < live.size(); k++) {
            if (live.get(k).id.equals(selected)) {
                i = k;
            }
        }
        int next = i < 0 ? (direction > 0 ? 0 : live.size() - 1) : Math.max(0, Math.min(live.size() - 1, i + direction));
        selected = live.get(next).id;
        reveal(live.get(next));
    }

    private void reveal(Row row) {
        float target = scroll.target();
        if (row.y < target) {
            target = row.y;
        } else if (row.y + ROW_H > target + getHeight()) {
            target = row.y + ROW_H - getHeight();
        }
        target = Math.max(0f, Math.min(maxScroll(), target));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    /** Where someone's row opens their menu (the scenario driver), or null when they have no row. */
    PersonMenu.Anchor anchorFor(String id) {
        for (Row r : people()) {
            if (r.id.equals(id)) {
                selected = id;
                return rowAnchor(r);
            }
        }
        return null;
    }

    private PersonMenu.Anchor rowAnchor(Row row) {
        float y = getY() + row.y - scroll.value();
        return new PersonMenu.Anchor(getX() + FACE + 8, y + 1, getWidth() - FACE - 8, ROW_H - 2, false);
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isUp() || event.isDown()) {
            PanelInput.usedKeyboard();
            move(event.isUp() ? -1 : 1);
            return true;
        }
        Row row = selectedRow();
        if (row != null && (event.isSelection() || event.isConfirmation() || event.key() == Compat.KEY_MENU
            || (event.key() == InputConstants.KEY_F10 && event.hasShiftDown()))) {
            PanelInput.usedKeyboard();
            actions.menu(row.id, row.label, rowAnchor(row), true);
            return true;
        }
        return false;
    }

    /** A right click opens someone's menu too. */
    @Override
    protected boolean isValidClickButton(MouseButtonInfo info) {
        return info.button() == InputConstants.MOUSE_BUTTON_LEFT || info.button() == InputConstants.MOUSE_BUTTON_RIGHT;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        float cy = (float) event.y() - getY() + scroll.value();
        for (Row row : people()) {
            if (cy < row.y || cy >= row.y + ROW_H) {
                continue;
            }
            selected = row.id;
            actions.menu(row.id, row.label, event.button() == InputConstants.MOUSE_BUTTON_RIGHT
                ? PersonMenu.Anchor.at(event.x(), event.y()) : rowAnchor(row), false);
            return;
        }
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY) || maxScroll() <= 0f) {
            return false;
        }
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() - (float) scrollY * 24f));
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
        int h = getHeight();
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        float top = scroll.value();
        Row onLive = null;
        float onLiveX = 0f;
        float onLiveY = 0f;
        g.enableScissor(x0 - 2, y0, x0 + w + 2, y0 + h);
        for (Row row : rows) {
            float y = y0 + row.y - top;
            if (y > y0 + h || y + ROW_H < y0) {
                continue;
            }
            boolean person = !row.me && !row.ringing;
            boolean lit = person && isHovered() && mouseY >= y && mouseY < y + ROW_H;
            boolean chosen = person && row.id.equals(selected) && keyboardFocus();
            if (lit || chosen) {
                Draw.frame(g, Frames.ROW_HOVER, x0 - 2, y + 1, w + 4, ROW_H - 2, 0x99FFFFFF);
            }
            if (chosen) {
                Draw.frame(g, Frames.FOCUS_ROW, x0 - 2, y + 1, w + 4, ROW_H - 2);
            }
            Draw.pushAlpha(row.ringing ? 0.55f : 1f);
            float fx = x0 + 1;
            float fy = y + (ROW_H - FACE) / 2f;
            row.face.draw(g, FACE, fx + FACE / 2f, fy + FACE / 2f);
            float spoke = row.ring.value(now);
            if (spoke > 0.004f) {
                Draw.pushAlpha(spoke);
                Draw.frame(g, Frames.SPEAKING, Draw.snap(fx), Draw.snap(fy), FACE, FACE);
                Draw.popAlpha();
            }
            float tx = fx + FACE + 7;
            float nameMid = row.sub == null ? y + ROW_H / 2f : y + 7f;
            Draw.text(g, strong, row.name, tx, Fonts.centeredY(Fonts.Face.STRONG, nameMid), Theme.BONE_100);
            if (row.live) {
                float chipX = tx + row.nameWidth + LiveChip.GAP;
                liveChip.draw(g, chipX, nameMid);
                // SHARE-LIVE: the pointer resting on someone else's "YAYINDA" opens their stream's card.
                if (!row.me && !row.ringing && isHovered() && y >= y0 && y + ROW_H <= y0 + h
                    && liveChip.contains(mouseX, mouseY, chipX, nameMid)) {
                    onLive = row;
                    onLiveX = chipX;
                    onLiveY = y;
                }
            }
            if (row.sub != null) {
                Draw.text(g, small, row.sub, tx, Fonts.centeredY(Fonts.Face.SMALL, y + 16f), Theme.ASH_300);
            }
            float mx = x0 + w - MARK;
            if (row.bars > 0) {
                drawBars(g, mx, y + (ROW_H - MARK) / 2f, row.bars);
                mx -= MARK + MARK_GAP;
            }
            for (int i = row.marks.size() - 1; i >= 0; i--) {
                Draw.icon(g, row.marks.get(i), 10, mx, y + (ROW_H - MARK) / 2f, row.markInks.get(i));
                mx -= MARK + MARK_GAP;
            }
            Draw.popAlpha();
        }
        float above = Math.min(1f, Math.max(0f, top / ShapeSet.LIST_FADE));
        float below = Math.min(1f, Math.max(0f, (maxScroll() - top) / ShapeSet.LIST_FADE));
        if (above > 0.01f) {
            Draw.pushAlpha(above);
            Draw.column(g, ShapeSet.FADE_TOP, x0 - 2, y0, w + 4, ShapeSet.LIST_FADE);
            Draw.popAlpha();
        }
        if (below > 0.01f) {
            Draw.pushAlpha(below);
            Draw.column(g, ShapeSet.FADE_BOTTOM, x0 - 2, y0 + h - ShapeSet.LIST_FADE, w + 4, ShapeSet.LIST_FADE);
            Draw.popAlpha();
        }
        g.disableScissor();
        drawNote(g, now);
        trackLive(onLive, onLiveX, onLiveY, now);
    }

    /** SHARE-LIVE: what the pointer rests on this frame, for the live card (drawn last, over the page). */
    private void trackLive(Row row, float x, float y, long now) {
        if (row == null) {
            liveTip.track(voice, null, null, null, null, null, 0, null, 0L, 0f, 0f, 0f, now, actions::preview);
            return;
        }
        VoiceState.LiveShare share = voice == null ? null : voice.liveShare(row.id);
        VoiceState.Room room = voice == null ? null : voice.room();
        String meta = "";
        long th = 0L;
        int n = 1;
        if (share != null) {
            meta = Texts.str("share.kind." + share.kind());
            if (share.watchers() > 0) {
                meta = meta + ", " + Texts.str("share.watchers", share.watchers());
            }
            th = share.th();
            n = share.n();
        }
        liveTip.track(voice, row.id, row.face, row.label, meta, room == null ? null : room.id(), n, null, th, x, y + 1,
            y + ROW_H - 1, now, actions::preview);
    }

    /** SHARE-LIVE: the live card over the page ({@code left..right}, {@code top..bottom}: the room page's box). */
    public void drawLiveTip(GuiGraphicsExtractor g, long now, float left, float top, float right, float bottom) {
        if (liveTip.active()) {
            // A layer of its own over the page (as a badge's tooltip), so placing it scans nothing else.
            Draw.stratum(g);
            liveTip.draw(g, now, left, top, right, bottom);
        }
    }

    /** SHARE-LIVE: the panel closed: its card and picture go. */
    public void releaseLive() {
        liveTip.release();
    }

    /** SHARE-LIVE: the middle of someone's "YAYINDA" (the scenario driver points at it), or null without one. */
    float[] liveChipCenter(String id) {
        for (Row row : rows) {
            if (row.live && !row.me && !row.ringing && row.id.equals(id)) {
                float y = getY() + row.y - scroll.value();
                float x = getX() + 1 + FACE + 7 + row.nameWidth + LiveChip.GAP;
                return new float[] {x + liveChip.width() / 2f, row.sub == null ? y + ROW_H / 2f : y + 7f};
            }
        }
        return null;
    }

    /** The server bars' look (docs/design.md §6): three rising bars, the filled ones amber (weak) or red (bad). */
    private static void drawBars(GuiGraphicsExtractor g, float x, float y, int filled) {
        int ink = filled >= 2 ? Theme.GOLD_400 : Theme.FLARE_400;
        for (int i = 0; i < 3; i++) {
            float bh = 4 + i * 3;
            float bx = x + 1 + i * 3;
            float by = y + MARK - bh;
            Draw.fill(g, bx, by, bx + 2, y + MARK, i < filled ? ink : Theme.withAlpha(Theme.ASH_500, 0.6f));
        }
    }

    private void drawNote(GuiGraphicsExtractor g, long now) {
        float a = noteAlpha.value(now);
        if (a <= 0.01f || noteText == null) {
            return;
        }
        float x = getX();
        float y = getY() + getHeight() - NOTE_H - 2;
        Draw.pushAlpha(a);
        Draw.frame(g, Frames.POPOVER, x, y, noteWidth, NOTE_H);
        Draw.text(g, Fonts.small(), noteText, x + 6, Fonts.centeredY(Fonts.Face.SMALL, y + NOTE_H / 2f),
            noteError ? Theme.FLARE_300 : Theme.BONE_100);
        Draw.popAlpha();
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        Row row = selectedRow();
        if (row != null) {
            output.add(NarratedElementType.HINT, row.narration);
            output.add(NarratedElementType.USAGE, Texts.tr("narration.friend.usage.menu"));
        }
    }
}
