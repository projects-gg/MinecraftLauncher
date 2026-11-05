package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.LabelFit;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.voice.VoiceMarks;
import gg.projects.overlay.voice.VoiceState;
import gg.projects.overlay.voice.VoiceWords;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * Voice chat in the panel (VOICE-O; voice.md §5.3, the launcher's room panel and incoming card): the card at the top of
 * the panel, and the room page.
 *
 * <p>The card: a ring for the player on every page ("Kaan_07 arıyor", who else is in the room, "Reddet" with the red
 * receiver and "Katıl" with the gold one) and, in a room, its strip on the chats and friends pages: the room's title, how
 * it stands while it is not live, the faces of the others with their speaking rings, and "Mikrofon", "Sağırlaştır" and
 * "Ayrıl" as icons (the microphone and hearing in flare while off, leaving in flare under the pointer). A press on the
 * strip opens the page.
 *
 * <p>The page: the room's title and crew, everyone in it ({@link VoiceList}; a press or a right click on someone opens
 * their menu, volume first) and "Mikrofon", "Sağırlaştır", "Ayrıl" along the bottom with their words. Out of the room the
 * page goes back by itself. Speaking changes move only the rings; nothing else is laid out for them.
 */
final class VoicePanel {
    private static final int RING_H = 34;
    /** OVL-SHARE: one of the player's shares on the room page (what it shows, who watches, "Paylaşımı durdur"). */
    private static final int SHARE_ROW = 24;
    /** The player's shares the page lists (Studio lets a player run up to three). */
    private static final int SHARE_ROWS = 3;
    private static final int STRIP_H = 40;
    private static final int FACE = 18;

    /** The launcher's words for a room's title, in the overlay's language. */
    static final VoiceState.Words WORDS = VoiceWords.WORDS;

    private final OverlayScreen screen;
    private final OverlayClient overlay;
    final VoiceStrip strip;
    final IconButton mic;
    final IconButton deaf;
    final IconButton leave;
    final ActionButton ringJoin;
    final IconButton ringDecline;
    final VoiceList list;
    final ActionButton pageMic;
    final ActionButton pageDeaf;
    final ActionButton pageLeave;
    /** OVL-SHARE: "Oyunu paylaş", and "Paylaşımı durdur" on each of the player's shares. */
    final ActionButton shareGame;
    final ActionButton[] shareStops = new ActionButton[SHARE_ROWS];
    private final int[] shareN = new int[SHARE_ROWS];

    private int seenRevision = -1;
    private int seenSpeaking = -1;
    // Card layout.
    private int x0;
    private int cw;
    private int ringY = -1;
    private String ringRoom;
    private Face ringFace;
    private FormattedCharSequence ringTitle;
    private FormattedCharSequence ringSub;
    private int stripY = -1;
    // Page layout.
    private int pageX;
    private int pageTitleY = -1;
    private FormattedCharSequence pageTitle;
    private FormattedCharSequence pageSub;
    /** One of the player's shares on the page: what it shows, how it stands, its row's top. */
    private record ShareLine(FormattedCharSequence title, FormattedCharSequence sub, boolean live, int y) {
    }

    private final List<ShareLine> shareLines = new ArrayList<>();
    private FormattedCharSequence shareWhy;
    private int shareWhyY = -1;

    VoicePanel(OverlayScreen screen, OverlayClient overlay) {
        this.screen = screen;
        this.overlay = overlay;
        strip = new VoiceStrip(() -> screen.showPage(OverlayScreen.Page.VOICE));
        mic = new IconButton(Icon.MIC, Texts.tr("voice.room.mute_mic"), this::toggleMic);
        deaf = new IconButton(Icon.HEADPHONES, Texts.tr("voice.room.deafen"), this::toggleDeaf);
        leave = new IconButton(Icon.PHONE_OFF, Texts.tr("voice.room.leave"), overlay::voiceLeave);
        // Quiet beside the microphone and hearing until reached for: flare is for what is off, and for leaving once
        // the pointer is on it.
        leave.setColors(Theme.ASH_300, Theme.FLARE_300);
        ringJoin = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("voice.ring.join"), Icon.PHONE,
            () -> answer(true));
        ringJoin.setIconInk(Theme.GOLD_300);
        ringDecline = new IconButton(Icon.PHONE_X, Texts.tr("voice.ring.decline"), () -> answer(false));
        ringDecline.setColors(Theme.FLARE_300, Theme.mix(Theme.FLARE_300, Theme.BONE_50, 0.35f));
        list = new VoiceList(screen::openVoiceMenu);
        pageMic = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("voice.room.mic"), Icon.MIC,
            this::toggleMic);
        pageDeaf = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("voice.room.deafen"), Icon.HEADPHONES,
            this::toggleDeaf);
        pageLeave = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("voice.room.leave"), Icon.PHONE_OFF,
            overlay::voiceLeave);
        pageLeave.setIconInk(Theme.FLARE_300);
        shareGame = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("share.game"), Icon.SCREENCAST,
            this::shareGame);
        for (int i = 0; i < SHARE_ROWS; i++) {
            int index = i;
            // Quiet like the bar's "Ayrıl": a share stops at once and starts again with one press.
            shareStops[i] = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("share.stop"), null,
                () -> overlay.voiceShareStop(shareN[index]));
        }
    }

    /** "Oyunu paylaş": once; the next state (the share starting) or a note says how it went. */
    private void shareGame() {
        shareGame.active = false;
        overlay.voiceShareGame();
    }

    private VoiceState voice() {
        return overlay.voice();
    }

    /** Every control, for the screen to add (the ring's answers first: they are what the player came for). */
    AbstractWidget[] widgets() {
        return new AbstractWidget[] {ringJoin, ringDecline, strip, mic, deaf, leave, list, shareGame, shareStops[0],
            shareStops[1], shareStops[2], pageMic, pageDeaf, pageLeave};
    }

    /** Nothing of voice chat on the panel until the next layout places it (no card or page draws where it stood). */
    void hideAll() {
        for (AbstractWidget w : widgets()) {
            w.visible = false;
        }
        ringY = -1;
        ringRoom = null;
        stripY = -1;
        pageTitleY = -1;
        shareLines.clear();
        shareWhyY = -1;
    }

    /** The room of the ring the card shows now, or null (the toasts' corner does not ring for it too). */
    String shownRing() {
        return ringY >= 0 ? ringRoom : null;
    }

    void setReducedMotion(boolean reduced) {
        list.setReducedMotion(reduced);
    }

    String title() {
        return Texts.str("voice.title");
    }

    boolean inRoom() {
        return voice().inRoom();
    }

    /** Someone has a row on the room page (their menu stays open while they are in the room). */
    boolean has(String id) {
        VoiceState.Room room = voice().room();
        return room != null && room.person(id) != null;
    }

    AbstractWidget initialFocus() {
        return voice().room() != null && !voice().room().people().isEmpty() ? list : pageMic;
    }

    /** The ring on the card stopped ringing by itself ({@code now}: Unix milliseconds): the card goes. */
    boolean ringOver(long now) {
        if (ringRoom == null) {
            return false;
        }
        VoiceState.Ring ring = voice().ring(ringRoom);
        return ring == null || ring.until() <= now;
    }

    /** Where someone's row on the room page opens their menu (the scenario driver), or null. */
    PersonMenu.Anchor anchorFor(String id) {
        return list.visible ? list.anchorFor(id) : null;
    }

    /** Whether voice chat changed since the panel was last laid out (the screen asks before each frame). */
    boolean stale() {
        return voice().revision() != seenRevision;
    }

    void seen() {
        seenRevision = voice().revision();
    }

    /** Who speaks changed: the strip's and the page's rings follow, nothing is laid out again. */
    void followSpeakers(long now) {
        VoiceState v = voice();
        if (v.speakingRevision() == seenSpeaking) {
            return;
        }
        seenSpeaking = v.speakingRevision();
        VoiceState.Room room = v.room();
        if (room == null) {
            return;
        }
        strip.speaking(stripPeople(room, v), now);
        list.speaking(room, now);
    }

    private void toggleMic() {
        VoiceState.Room room = voice().room();
        if (room != null && !room.me().mutedByModerator()) {
            // Opening the microphone ends deafening too (the launcher's rule, as Discord's).
            overlay.voiceMute(!room.me().micOff());
        }
    }

    private void toggleDeaf() {
        VoiceState.Room room = voice().room();
        if (room != null) {
            overlay.voiceDeaf(!room.me().deaf());
        }
    }

    private void answer(boolean join) {
        if (ringRoom != null) {
            overlay.voiceAnswer(ringRoom, join);
        }
    }

    private static List<VoiceStrip.Person> stripPeople(VoiceState.Room room, VoiceState live) {
        List<VoiceStrip.Person> out = new ArrayList<>(room.people().size());
        for (VoiceState.Person p : room.people()) {
            out.add(new VoiceStrip.Person(p.id(), VoiceState.shown(p.name(), WORDS), p.speaking(),
                VoiceMarks.face(p.mute(), p.deaf(), p.mutedByModerator()), live.sharing(p.id())));
        }
        return out;
    }

    /** What the microphone's and hearing's buttons say and show for the player's state now. */
    private void dressButtons(VoiceState.Me me) {
        boolean micOff = me.micOff();
        Component micWords = me.mutedByModerator() ? Texts.tr("voice.muted_by_mod")
            : Texts.tr(micOff ? "voice.room.unmute_mic" : "voice.room.mute_mic");
        int off = Theme.FLARE_300;
        int offLit = Theme.mix(Theme.FLARE_300, Theme.BONE_50, 0.35f);
        mic.setIcon(micOff ? Icon.MIC_OFF : Icon.MIC);
        mic.setColors(micOff ? off : Theme.ASH_300, micOff ? offLit : Theme.BONE_100);
        mic.setMessage(micWords);
        mic.active = !me.mutedByModerator();
        deaf.setIcon(me.deaf() ? Icon.DEAF : Icon.HEADPHONES);
        deaf.setColors(me.deaf() ? off : Theme.ASH_300, me.deaf() ? offLit : Theme.BONE_100);
        deaf.setMessage(Texts.tr(me.deaf() ? "voice.room.deafen_off" : "voice.room.deafen"));
        pageMic.setIcon(micOff ? Icon.MIC_OFF : Icon.MIC);
        pageMic.setIconInk(micOff ? off : 0);
        pageMic.setMessage(micWords);
        pageMic.active = !me.mutedByModerator();
        pageDeaf.setIcon(me.deaf() ? Icon.DEAF : Icon.HEADPHONES);
        pageDeaf.setIconInk(me.deaf() ? off : 0);
        pageDeaf.setMessage(Texts.tr(me.deaf() ? "voice.room.deafen_off" : "voice.room.deafen"));
    }

    // ---- The card ----------------------------------------------------------------------------------------------

    /**
     * Lays out the ring (on every page: it waits for no page) and, with {@code withStrip} (the chats and friends pages),
     * the room's strip under the panel's notices. Returns where the page's own rows start.
     */
    int layoutCard(int x, int y, int width, long now, boolean withStrip) {
        x0 = x;
        cw = width;
        ringY = -1;
        stripY = -1;
        ringRoom = null;
        VoiceState v = voice();
        if (!overlay.state().online()) {
            return y;
        }
        List<VoiceState.Ring> rings = v.rings(System.currentTimeMillis());
        if (!rings.isEmpty()) {
            VoiceState.Ring ring = rings.get(0);
            ringRoom = ring.room();
            ringY = y;
            String name = VoiceState.shown(ring.name(), WORDS);
            if (ringFace == null || !ringFace.is(name, ring.from())) {
                ringFace = Face.of(name, ring.from());
            }
            int jw = ringJoin.preferredWidth();
            ringJoin.visible = true;
            PanelWidget.place(ringJoin, x + width - 6 - jw, y + (RING_H - ActionButton.HEIGHT) / 2, jw,
                ActionButton.HEIGHT);
            ringDecline.visible = true;
            PanelWidget.place(ringDecline, ringJoin.getX() - 2 - IconButton.SIZE, y + (RING_H - IconButton.SIZE) / 2,
                IconButton.SIZE, IconButton.SIZE);
            int textW = Math.max(30, ringDecline.getX() - 6 - (x + 8 + FACE + 7));
            Font strong = Fonts.strong();
            String verb = Texts.str("voice.ring.verb");
            String fitted = Texts.fit(strong, name, Math.max(16, textW - strong.width(" " + verb))) + " " + verb;
            ringTitle = Texts.visual(fitted);
            List<String> others = new ArrayList<>(ring.with().size());
            for (String other : ring.with()) {
                others.add(VoiceState.shown(other, WORDS));
            }
            String sub = others.isEmpty() ? Texts.str("voice.title")
                : Texts.str("voice.ring.others", VoiceState.names(others, WORDS));
            ringSub = Texts.visual(Texts.fit(Fonts.small(), sub, textW));
            ringJoin.setMessage(Texts.tr("voice.ring.join"));
            ringDecline.setMessage(Texts.tr("voice.ring.decline"));
            y += RING_H + 6;
        }
        VoiceState.Room room = withStrip ? v.room() : null;
        if (room != null) {
            stripY = y;
            dressButtons(room.me());
            int by = y + 19;
            leave.visible = true;
            PanelWidget.place(leave, x + width - 6 - IconButton.SIZE, by, IconButton.SIZE, IconButton.SIZE);
            deaf.visible = true;
            PanelWidget.place(deaf, leave.getX() - 2 - IconButton.SIZE, by, IconButton.SIZE, IconButton.SIZE);
            mic.visible = true;
            PanelWidget.place(mic, deaf.getX() - 2 - IconButton.SIZE, by, IconButton.SIZE, IconButton.SIZE);
            boolean calling = room.people().isEmpty() && v.calling() != null;
            leave.setMessage(Texts.tr(calling ? "voice.cancel_call" : "voice.room.leave"));
            strip.visible = true;
            int sx = x + 8;
            PanelWidget.place(strip, sx, y + 7, Math.max(40, mic.getX() - 6 - sx), STRIP_H - 14);
            String title = calling ? Texts.str("voice.ringing_name", VoiceState.shown(v.calling().name(), WORDS))
                : VoiceState.title(room, v.calling(), WORDS);
            // OVL-SHARE: the player's own share where the room's state goes ("Yayında, 2 kişi izliyor"), in jade.
            String stateText = stateWords(room);
            int ink = Theme.ASH_300;
            if (stateText == null && v.live()) {
                stateText = v.watchers() > 0 ? Texts.str("share.strip.watching", v.watchers())
                    : Texts.str("share.live");
                ink = Theme.JADE_400;
            }
            strip.show(title, stateText, ink, stripPeople(room, v), now);
            y += STRIP_H + 8;
        }
        seenSpeaking = v.speakingRevision();
        return y;
    }

    /** How the room stands while it is not live ("Bağlanıyor…", "Yeniden bağlanıyor…"), else null. */
    private static String stateWords(VoiceState.Room room) {
        if (room.joining()) {
            return Texts.str("voice.room.joining");
        }
        if (room.reconnecting()) {
            return Texts.str("voice.room.reconnecting");
        }
        return null;
    }

    void drawCard(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        if (ringY >= 0) {
            Draw.frame(g, Frames.CARD, x0, ringY, cw, RING_H);
            float fx = x0 + 8 + FACE / 2f;
            float fy = ringY + RING_H / 2f;
            ringFace.draw(g, FACE, fx, fy);
            float tx = x0 + 8 + FACE + 7;
            Draw.text(g, Fonts.strong(), ringTitle, tx, Fonts.centeredY(Fonts.Face.STRONG, ringY + 12f),
                Theme.BONE_50);
            Draw.text(g, Fonts.small(), ringSub, tx, Fonts.centeredY(Fonts.Face.SMALL, ringY + 23f), Theme.ASH_300);
            render(g, ringDecline, mouseX, mouseY);
            render(g, ringJoin, mouseX, mouseY);
        }
        if (stripY >= 0) {
            Draw.frame(g, Frames.CARD, x0, stripY, cw, STRIP_H);
            render(g, strip, mouseX, mouseY);
            render(g, mic, mouseX, mouseY);
            render(g, deaf, mouseX, mouseY);
            render(g, leave, mouseX, mouseY);
        }
    }

    // ---- The room page -----------------------------------------------------------------------------------------

    /** Lays out the page between {@code top} and {@code bottom}. */
    void layoutPage(int x, int top, int width, int bottom, long now) {
        pageX = x;
        pageTitleY = -1;
        VoiceState v = voice();
        VoiceState.Room room = v.room();
        if (room == null) {
            return;
        }
        dressButtons(room.me());
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int y = top;
        pageTitleY = y;
        pageTitle = Texts.visual(Texts.fit(strong, VoiceState.title(room, v.calling(), WORDS), width));
        String sub = stateWords(room);
        if (sub == null && room.crewRoom() && room.crew() != null) {
            sub = Texts.str("voice.room.crew_of", room.crew());
        }
        pageSub = sub == null ? null : Texts.visual(Texts.fit(small, sub, width));
        y += 12 + (pageSub == null ? 0 : 10) + 6;
        int by = bottom - ActionButton.HEIGHT;
        // A third of the panel: "İptal" while the player's call rings (the strip's icon says "Aramayı iptal et").
        pageLeave.setMessage(Texts.tr(room.people().isEmpty() && v.calling() != null ? "voice.cancel"
            : "voice.room.leave"));
        // LABEL-FIT: thirds while every label fits one, else by what each needs ("Microphone" keeps its icon longer).
        int[] bw = LabelFit.row(width - 8, new int[] {pageMic.preferredWidth(), pageDeaf.preferredWidth(),
            pageLeave.preferredWidth()});
        pageMic.visible = true;
        PanelWidget.place(pageMic, x, by, bw[0], ActionButton.HEIGHT);
        pageDeaf.visible = true;
        PanelWidget.place(pageDeaf, x + bw[0] + 4, by, bw[1], ActionButton.HEIGHT);
        pageLeave.visible = true;
        PanelWidget.place(pageLeave, x + bw[0] + bw[1] + 8, by, bw[2], ActionButton.HEIGHT);
        // One size for the bar: a button that needed smaller words gives the same size to its neighbours.
        ActionButton[] bar = {pageMic, pageDeaf, pageLeave};
        for (ActionButton b : bar) {
            b.setLargestFace(Fonts.Face.UI);
        }
        Fonts.Face face = LabelFit.smallest(pageMic.fitFace(), pageDeaf.fitFace(), pageLeave.fitFace());
        for (ActionButton b : bar) {
            b.setLargestFace(face);
        }
        int listBottom = layoutShare(x, width, by - 8);
        list.visible = true;
        PanelWidget.place(list, x, y, width, Math.max(VoiceList.ROW_H, listBottom - y));
        list.show(room, overlay.state().me(), WORDS, v, now);
        seenSpeaking = v.speakingRevision();
    }

    /**
     * OVL-SHARE: the player's shares and "Oyunu paylaş" above the page's bar, up from {@code bottom}: each share's row
     * (what it shows, "Başlıyor…" or who watches, "Paylaşımı durdur"), then "Oyunu paylaş" while it shows (dim with one
     * quiet line saying why). Returns where the list above them ends.
     */
    private int layoutShare(int x, int width, int bottom) {
        shareLines.clear();
        shareWhyY = -1;
        VoiceState v = voice();
        List<VoiceState.MyShare> mine = v.myShares();
        int rows = Math.min(SHARE_ROWS, mine.size());
        String game = v.shareGame();
        if (rows == 0 && game.isEmpty()) {
            return bottom;
        }
        Font small = Fonts.small();
        String why = switch (v.shareWhy()) {
            case "not-allowed" -> Texts.str("share.note.not_allowed");
            case "full" -> Texts.str("share.note.full");
            default -> Texts.str("share.note.off");
        };
        boolean dim = game.equals("off");
        int height = rows * SHARE_ROW + (game.isEmpty() ? 0 : ActionButton.HEIGHT + 4 + (dim ? 12 : 0));
        int y = bottom - height;
        int top = y;
        for (int i = 0; i < rows; i++) {
            VoiceState.MyShare share = mine.get(i);
            shareN[i] = share.n();
            ActionButton stop = shareStops[i];
            int sw = stop.preferredWidth();
            stop.visible = true;
            stop.active = overlay.state().online();
            PanelWidget.place(stop, x + width - sw, y + (SHARE_ROW - ActionButton.HEIGHT) / 2, sw,
                ActionButton.HEIGHT);
            int textW = Math.max(30, width - sw - 8 - 14);
            String title = Texts.str(switch (share.kind()) {
                case "game" -> "share.mine.game";
                case "screen" -> "share.mine.screen";
                default -> "share.mine.window";
            });
            String sub = !share.live() ? Texts.str("share.starting")
                : share.watchers() > 0 ? Texts.str("share.watchers", share.watchers())
                : Texts.str("share.resting");
            shareLines.add(new ShareLine(Texts.visual(Texts.fit(Fonts.ui(), title, textW)),
                Texts.visual(Texts.fit(small, sub, textW)), share.live(), y));
            y += SHARE_ROW;
        }
        if (!game.isEmpty()) {
            y += 2;
            int gw = Math.min(width, shareGame.preferredWidth());
            shareGame.visible = true;
            shareGame.active = !dim && overlay.state().online();
            PanelWidget.place(shareGame, x, y, gw, ActionButton.HEIGHT);
            y += ActionButton.HEIGHT + 2;
            if (dim) {
                shareWhy = Texts.visual(Texts.fit(small, why, width));
                shareWhyY = y;
            }
        }
        return top - 6;
    }

    void drawPage(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        if (pageTitleY < 0) {
            return;
        }
        Draw.text(g, Fonts.strong(), pageTitle, pageX, Fonts.centeredY(Fonts.Face.STRONG, pageTitleY + 5f),
            Theme.BONE_50);
        if (pageSub != null) {
            Draw.text(g, Fonts.small(), pageSub, pageX, Fonts.centeredY(Fonts.Face.SMALL, pageTitleY + 16f),
                Theme.ASH_300);
        }
        list.setNote(overlay.state().personNote(System.currentTimeMillis()), now);
        render(g, list, mouseX, mouseY);
        for (ShareLine line : shareLines) {
            // A share the room took wears the jade sign; one still starting, the quiet one.
            Draw.icon(g, Icon.SCREENCAST, 10, pageX, line.y + 2, line.live ? Theme.JADE_400 : Theme.ASH_300);
            Draw.text(g, Fonts.ui(), line.title, pageX + 14, Fonts.centeredY(Fonts.Face.UI, line.y + 7f), Theme.BONE_100);
            Draw.text(g, Fonts.small(), line.sub, pageX + 14, Fonts.centeredY(Fonts.Face.SMALL, line.y + 18f),
                Theme.ASH_300);
        }
        for (ActionButton stop : shareStops) {
            render(g, stop, mouseX, mouseY);
        }
        render(g, shareGame, mouseX, mouseY);
        if (shareWhyY >= 0) {
            Draw.text(g, Fonts.small(), shareWhy, pageX, Fonts.centeredY(Fonts.Face.SMALL, shareWhyY + 5f),
                Theme.ASH_300);
        }
        render(g, pageMic, mouseX, mouseY);
        render(g, pageDeaf, mouseX, mouseY);
        render(g, pageLeave, mouseX, mouseY);
    }

    private static void render(GuiGraphicsExtractor g, AbstractWidget w, int mouseX, int mouseY) {
        if (w.visible) {
            w.extractRenderState(g, mouseX, mouseY, 0f);
        }
    }
}
