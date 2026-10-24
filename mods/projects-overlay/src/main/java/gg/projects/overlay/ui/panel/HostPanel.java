package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.host.HostController;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.PixelSprites;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * World sharing in the panel (HOST1, host1.md §4.8): the world page and the card the other pages carry.
 *
 * <p>The page: picking friends ("Arkadaşlarını çağır": the online friends, the game mode, the commands switch, "IP
 * adresin arkadaşlarına görünmez.", "Projects'te buluşun" and "Davet et"), opening ("Dünyan açılıyor"), a failure
 * that says so with "Tekrar dene", and the open world ("Dünyan açık": the friends in it, what they may do, "Paylaşımı
 * kapat" and "Davet et" for more). A change between them is the panel's page change (220 ms on the drawer curve,
 * opacity only under reduced motion); rows inside change in place.
 *
 * <p>The card, on the chats and friends pages: "Dünyan açık" with who is in it and "Yönet" while the world is open; the
 * pause menu's "Arkadaşlarını çağır" while the player is in a singleplayer world; a friend's world invite with "Katıl".
 */
final class HostPanel {
    /** What the page shows. */
    enum Mode { PICK, OPENING, FAILED, OPEN, MORE }

    private static final int SECTION_H = 12;
    private static final int GAP = 6;
    private static final int CARD_H = 32;
    private static final String[] MODES = {"survival", "creative", "adventure"};

    private final OverlayScreen screen;
    private final OverlayClient overlay;
    private final OverlayState state;
    final HostList list;
    final ChoiceWell modes;
    final ToggleRow commands;
    final ActionButton primary;
    final ActionButton meet;
    final ActionButton stop;
    final ActionButton cardOpen;
    final ActionButton cardManage;
    final ActionButton cardJoin;

    private Mode mode = Mode.PICK;
    private String failure;
    private boolean pickedModeByHand;
    private int seenRevision = -1;
    // Page layout.
    private int x0;
    private int cw;
    private int sectionY = -1;
    private String sectionLeft = "";
    private String sectionRight = "";
    private int sectionRightColor = Theme.ASH_300;
    private int modeLabelY = -1;
    private int noteY = -1;
    private List<FormattedCharSequence> noteLines = List.of();
    private int noteColor = Theme.ASH_400;
    private int noticeY = -1;
    private int noticeH;
    private List<FormattedCharSequence> noticeLines = List.of();
    private int cardNoticeY = -1;
    private int cardNoticeH;
    private List<FormattedCharSequence> cardNoticeLines = List.of();
    // Card layout.
    private enum Card { NONE, OPEN, INVITE, WORLD_INVITE }
    private Card card = Card.NONE;
    private int cardY = -1;
    private FormattedCharSequence cardTitle;
    private FormattedCharSequence cardSub;
    private String cardFriend;

    HostPanel(OverlayScreen screen, OverlayClient overlay) {
        this.screen = screen;
        this.overlay = overlay;
        this.state = overlay.state();
        list = new HostList(this::askRemove, this::picksChanged);
        Component[] modeNames = new Component[MODES.length];
        for (int i = 0; i < MODES.length; i++) {
            modeNames[i] = Texts.tr("host.mode." + MODES[i]);
        }
        modes = new ChoiceWell(Texts.tr("host.mode"), modeNames, modeIndex(host().worldMode()),
            i -> pickedModeByHand = true);
        commands = new ToggleRow(Texts.tr("host.commands"), false);
        primary = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("host.invite"), null, this::primaryPressed);
        meet = new ActionButton(ActionButton.Style.GHOST, Texts.tr("host.meet"), null, this::meetPressed);
        stop = new ActionButton(ActionButton.Style.DANGER, Texts.tr("host.stop"), null, this::stopPressed);
        cardOpen = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("host.invite_friends"), Icon.USER_PLUS,
            () -> screen.showPage(OverlayScreen.Page.WORLD));
        cardManage = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("host.manage"), null,
            () -> screen.showPage(OverlayScreen.Page.WORLD));
        cardJoin = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("friend.join"), Icon.JOIN, () -> {
            if (cardFriend != null) {
                screen.joinWorld(cardFriend);
            }
        });
    }

    private HostController host() {
        return overlay.host();
    }

    private static int modeIndex(String mode) {
        for (int i = 0; i < MODES.length; i++) {
            if (MODES[i].equals(mode)) {
                return i;
            }
        }
        return 0;
    }

    /** Every control, for the screen to add (the list first: the keyboard reaches it before the actions). */
    AbstractWidget[] widgets() {
        return new AbstractWidget[] {list, modes, commands, meet, stop, primary, cardOpen, cardManage, cardJoin};
    }

    void hideAll() {
        for (AbstractWidget w : widgets()) {
            w.visible = false;
        }
    }

    void setReducedMotion(boolean reduced) {
        list.setReducedMotion(reduced);
        modes.setReducedMotion(reduced);
        commands.setReducedMotion(reduced);
    }

    /** The page's title in the panel's header. */
    String title() {
        return Texts.str(mode == Mode.OPEN ? "host.open" : "host.invite_friends");
    }

    /** Back from "Davet et" for more goes to the open world, not out of the page. */
    boolean back() {
        if (mode == Mode.MORE) {
            setMode(Mode.OPEN);
            return true;
        }
        return false;
    }

    /** The first control the keyboard lands on. */
    AbstractWidget initialFocus() {
        return list;
    }

    // ---- What the page is ----------------------------------------------------------------------------------

    /** Works out the page's state from the session; true when it changed (the screen plays its page change). */
    boolean sync(long now) {
        Mode next = resolve();
        if (next != mode) {
            if (next == Mode.FAILED && list.picked().isEmpty()) {
                // A failed opening keeps its friends ticked (asked from the friend menu too): "Tekrar dene" works.
                list.restorePicks(host().lastInvite());
            }
            mode = next;
            return true;
        }
        return false;
    }

    private Mode resolve() {
        HostController h = host();
        OverlayState.HostView view = state.host();
        if (h.hosting()) {
            if (view != null && view.open()) {
                return mode == Mode.MORE ? Mode.MORE : Mode.OPEN;
            }
            return Mode.OPENING;
        }
        // An opening asked here or from the friend menu, and how the last one ended, are the controller's.
        if (h.requesting()) {
            return Mode.OPENING;
        }
        if (h.failure() != null) {
            failure = h.failure();
            return Mode.FAILED;
        }
        return Mode.PICK;
    }

    private void setMode(Mode next) {
        if (next != mode) {
            mode = next;
            screen.hostModeChanged();
        }
    }

    Mode mode() {
        return mode;
    }

    // ---- Actions ---------------------------------------------------------------------------------------------

    private void picksChanged() {
        screen.hostLayout();
    }

    private void primaryPressed() {
        switch (mode) {
            case PICK, MORE, FAILED -> {
                List<String> picks = list.picked();
                if (picks.isEmpty() || !state.online()) {
                    return;
                }
                boolean more = mode == Mode.MORE;
                if (!host().invite(MODES[modes.selected()], commands.on(), picks)) {
                    return;
                }
                if (more) {
                    list.clearPicks();
                    setMode(Mode.OPEN);
                } else {
                    failure = null;
                    setMode(Mode.OPENING);
                }
            }
            case OPEN -> {
                list.clearPicks();
                setMode(Mode.MORE);
            }
            default -> {
            }
        }
    }

    private void meetPressed() {
        List<String> picks = list.picked();
        if (picks.isEmpty()) {
            return;
        }
        boolean single = screen.inSingleplayer();
        screen.ask(new ConfirmScreen.Question(Texts.tr("join.title.projects"), Texts.str("host.meet.body"),
            single ? Texts.str("join.saved") : null, Texts.tr("join.connect"), Icon.JOIN, false),
            () -> overlay.meetOnProjects(picks));
    }

    private void stopPressed() {
        OverlayState.HostView view = state.host();
        if (view == null || view.inWorld() == 0) {
            host().stopSharing();
            return;
        }
        screen.ask(new ConfirmScreen.Question(Texts.tr("host.stop.title"), Texts.str("host.stop.ask"), null,
            Texts.tr("host.stop"), Icon.LOCK, true), () -> host().stopSharing());
    }

    private void askRemove(HostList.Row row) {
        if (row.guest.isEmpty()) {
            return;
        }
        String guest = row.guest;
        screen.ask(new ConfirmScreen.Question(Texts.tr("host.remove.ask", row.label), Texts.str("host.remove.body"),
            null, Texts.tr("host.remove"), Icon.USER_MINUS, true), () -> host().removeGuest(guest));
    }

    // ---- The page's layout and drawing ------------------------------------------------------------------------

    /**
     * Lays the page out between {@code top} and {@code bottom}: the section line, the list, then from the bottom up the
     * actions and what stands over them.
     */
    void layoutPage(int x, int top, int width, int bottom) {
        x0 = x;
        cw = width;
        boolean online = state.online();
        boolean busy = mode == Mode.OPENING;
        OverlayState.HostView view = state.host();
        Font small = Fonts.small();
        // Bottom up: the actions, then what stands over them.
        int by = bottom - ActionButton.HEIGHT;
        int above = by - GAP;
        noteY = -1;
        noticeY = -1;
        modeLabelY = -1;
        switch (mode) {
            case OPEN -> {
                stop.visible = true;
                stop.active = online;
                int sw = stop.preferredWidth();
                PanelWidget.place(stop, x, by, sw, ActionButton.HEIGHT);
                boolean more = online && !invitable(view).isEmpty();
                primary.visible = more;
                if (more) {
                    primary.setStyle(ActionButton.Style.PRIMARY);
                    primary.setLabel(Texts.tr("host.invite"));
                    primary.setIcon(Icon.USER_PLUS);
                    primary.active = true;
                    int pw = primary.preferredWidth();
                    PanelWidget.place(primary, x + width - pw, by, pw, ActionButton.HEIGHT);
                }
                // What the guests may do, and the voice chat note when the pack has it.
                // "Yaratıcı, komutlar kapalı" (no middle-dot chains, design.md §7).
                String meta = Texts.str("host.meta", Texts.str("host.mode." + host().sessionMode()),
                    Texts.str(host().sessionCommands() ? "host.commands.on" : "host.commands.off"));
                List<FormattedCharSequence> lines = new java.util.ArrayList<>(Texts.wrap(small, meta, width, 1));
                if (FabricLoader.getInstance().isModLoaded("voicechat")) {
                    lines.addAll(Texts.wrap(small, Texts.str("host.voice"), width, 2));
                }
                noteLines = lines;
                noteColor = Theme.ASH_400;
                noteY = above - lines.size() * 9;
                above = noteY - GAP;
            }
            case MORE -> {
                primary.visible = true;
                primary.setStyle(ActionButton.Style.PRIMARY);
                primary.setLabel(Texts.tr("host.invite"));
                primary.setIcon(null);
                primary.active = online && !list.picked().isEmpty();
                int pw = primary.preferredWidth();
                PanelWidget.place(primary, x + width - pw, by, pw, ActionButton.HEIGHT);
            }
            default -> {
                // Picking, opening, failed.
                primary.visible = !(mode == Mode.FAILED && "limit".equals(failure));
                primary.setStyle(ActionButton.Style.PRIMARY);
                primary.setIcon(null);
                // "Tekrar dene" only with friends to try again for; with none it is "Davet et" waiting for a pick.
                boolean retry = mode == Mode.FAILED && !list.picked().isEmpty();
                primary.setLabel(Texts.tr(retry ? "host.retry" : busy ? "host.opening" : "host.invite"));
                primary.active = online && !busy && !list.picked().isEmpty();
                int pw = primary.preferredWidth();
                PanelWidget.place(primary, x + width - pw, by, pw, ActionButton.HEIGHT);
                meet.visible = mode != Mode.OPENING;
                meet.active = online && !list.picked().isEmpty();
                int mw = Math.min(meet.preferredWidth(), width - (primary.visible ? pw + GAP : 0));
                PanelWidget.place(meet, primary.visible ? x + width - pw - GAP - mw : x + width - mw, by, mw,
                    ActionButton.HEIGHT);
                if (mode == Mode.FAILED) {
                    noticeLines = Texts.wrap(Fonts.ui(), HostController.failureLine(failure),
                        width - 8 - 10 - 6 - 8, 2);
                    noticeH = 8 + noticeLines.size() * 10 + 8;
                    noticeY = above - noticeH;
                    above = noticeY - GAP;
                } else {
                    noteLines = Texts.wrap(small, Texts.str("host.privacy"), width, 2);
                    noteColor = Theme.ASH_400;
                    noteY = above - noteLines.size() * 9;
                    above = noteY - GAP;
                }
                commands.visible = true;
                commands.active = !busy;
                PanelWidget.place(commands, x, above - ToggleRow.HEIGHT, width, ToggleRow.HEIGHT);
                above -= ToggleRow.HEIGHT + 4;
                if (!host().hardcore()) {
                    if (!pickedModeByHand) {
                        // The world's own mode, until the player picks another.
                        modes.select(modeIndex(host().worldMode()));
                    }
                    modes.visible = true;
                    modes.active = !busy;
                    PanelWidget.place(modes, x, above - ChoiceWell.HEIGHT, width, ChoiceWell.HEIGHT);
                    above -= ChoiceWell.HEIGHT + 3;
                    modeLabelY = above - 9;
                    above = modeLabelY - GAP;
                }
            }
        }
        // The section line and the list over what is left.
        sectionY = top;
        int listTop = top + SECTION_H;
        list.visible = true;
        list.active = !busy;
        PanelWidget.place(list, x, listTop, width, Math.max(24, above - listTop));
        if (mode == Mode.OPEN) {
            list.showGuests(view == null ? new OverlayState.HostView(true, 7, List.of(), null) : view);
            sectionLeft = Texts.str("host.guests");
            int max = view == null ? 7 : view.max();
            sectionRight = (view == null ? 1 : view.inWorld() + 1) + "/" + (max + 1);
            sectionRightColor = Theme.ASH_300;
        } else {
            Set<String> except = mode == Mode.MORE && view != null ? inSession(view) : Set.of();
            int room = mode == Mode.MORE && view != null ? Math.max(0, view.max() - view.guests().size())
                : maxGuests(view);
            list.showPick(state, except, room, mode == Mode.MORE ? "host.all_invited" : "host.empty");
            sectionLeft = Texts.str("host.friends");
            int picked = list.picked().size();
            sectionRight = picked + "/" + list.max();
            sectionRightColor = picked > 0 && picked >= list.max() ? Theme.GOLD_300 : Theme.ASH_300;
            if (list.isEmpty()) {
                // Nobody to pick: nothing to count either.
                sectionRight = "";
            }
            // The picks may have changed with the list (a friend went away).
            boolean any = !list.picked().isEmpty();
            primary.active = online && !busy && any;
            meet.active = online && any;
        }
    }

    private static int maxGuests(OverlayState.HostView view) {
        return view == null ? 7 : Math.max(1, Math.min(7, view.max()));
    }

    private static Set<String> inSession(OverlayState.HostView view) {
        Set<String> ids = new HashSet<>();
        for (OverlayState.HostGuest g : view.guests()) {
            ids.add(g.friend());
        }
        return ids;
    }

    /** Online friends not in the session yet. */
    private List<OverlayState.Friend> invitable(OverlayState.HostView view) {
        Set<String> in = view == null ? Set.of() : inSession(view);
        return state.friends().stream().filter(f -> f.online() && !in.contains(f.id())).toList();
    }

    void drawPage(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        Font small = Fonts.small();
        if (sectionY >= 0) {
            float mid = sectionY + SECTION_H / 2f - 1;
            Draw.text(g, small, sectionLeft, x0 + 1, Fonts.centeredY(Fonts.Face.SMALL, mid), Theme.ASH_400);
            if (!sectionRight.isEmpty()) {
                float tx = x0 + cw - small.width(sectionRight);
                Draw.text(g, small, sectionRight, tx, Fonts.centeredY(Fonts.Face.SMALL, mid), sectionRightColor);
                // The launcher's people count: the sign says what the figure counts.
                Draw.icon(g, Icon.CREW, 8, tx - 3 - 8, mid - 4, sectionRightColor);
            }
        }
        render(g, list, mouseX, mouseY);
        if (modeLabelY >= 0) {
            Draw.text(g, small, Texts.str("host.mode"), x0 + 1, Fonts.centeredY(Fonts.Face.SMALL, modeLabelY + 4.5f),
                Theme.ASH_400);
        }
        render(g, modes, mouseX, mouseY);
        render(g, commands, mouseX, mouseY);
        if (noticeY >= 0) {
            drawNotice(g, noticeY, noticeH, noticeLines);
        }
        if (noteY >= 0) {
            float ty = noteY;
            for (FormattedCharSequence line : noteLines) {
                Draw.text(g, small, line, x0 + 1, Fonts.centeredY(Fonts.Face.SMALL, ty + 4.5f), noteColor);
                ty += 9;
            }
        }
        render(g, meet, mouseX, mouseY);
        render(g, stop, mouseX, mouseY);
        render(g, primary, mouseX, mouseY);
    }

    /** A notice panel across the page: the warning sign and the lines (a failed opening). */
    private void drawNotice(GuiGraphicsExtractor g, int y, int h, List<FormattedCharSequence> lines) {
        Draw.frame(g, Frames.CARD, x0, y, cw, h);
        Draw.icon(g, Icon.WARNING, 10, x0 + 8, y + 8, Theme.FLARE_300);
        float ty = y + 8;
        for (FormattedCharSequence line : lines) {
            Draw.text(g, Fonts.ui(), line, x0 + 8 + 10 + 6, Fonts.centeredY(Fonts.Face.UI, ty + 5), Theme.BONE_100);
            ty += 10;
        }
    }

    // ---- The card on the other pages ------------------------------------------------------------------------

    /**
     * Lays out the card under the panel's notices and call: the open world, the invite entry or a friend's world
     * invite. Returns where the page's own rows start.
     */
    int layoutCard(int x, int y, int width) {
        x0 = x;
        cw = width;
        cardY = -1;
        cardNoticeY = -1;
        cardFriend = null;
        card = Card.NONE;
        HostController h = host();
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        if (!state.online()) {
            return y;
        }
        if (h.hosting() || h.requesting()) {
            // Open, or opening (asked here or from the friend menu).
            OverlayState.HostView view = state.host();
            boolean open = h.hosting() && view != null && view.open();
            card = Card.OPEN;
            cardY = y;
            int mw = cardManage.preferredWidth();
            int textW = width - 8 - PixelSprites.SIZE - 7 - mw - 8 - 4;
            cardTitle = Texts.visual(Texts.fit(strong, Texts.str(open ? "host.open" : "host.opening"), textW));
            // Who is in it ("Dünyanda: Ayşe"); while it opens the title stands alone.
            cardSub = open ? Texts.visual(Texts.fit(small, inWorldLine(view), textW)) : null;
            cardManage.visible = true;
            PanelWidget.place(cardManage, x + width - 6 - mw, y + (CARD_H - ActionButton.HEIGHT) / 2, mw,
                ActionButton.HEIGHT);
            return y + CARD_H + 8;
        }
        if (h.recentFailure() && h.inWorld()) {
            // The last opening failed (asked from the friend menu too): one line over whatever card follows.
            cardNoticeLines = Texts.wrap(Fonts.ui(), HostController.failureLine(h.failure()), width - 8 - 10 - 6 - 8,
                2);
            cardNoticeH = 8 + cardNoticeLines.size() * 10 + 8;
            cardNoticeY = y;
            y += cardNoticeH + 4;
        }
        // A friend waiting in their world comes before opening one's own (the pause menu still has that).
        String friend = screen.worldInviteToShow();
        if (friend != null) {
            OverlayState.Friend f = state.friend(friend);
            OverlayState.WorldMark mark = state.world(friend);
            if (f != null && mark != null) {
                card = Card.WORLD_INVITE;
                cardFriend = friend;
                cardY = y;
                int jw = cardJoin.preferredWidth();
                int textW = width - 8 - PixelSprites.SIZE - 7 - jw - 8 - 4;
                cardTitle = Texts.visual(Texts.fit(strong, f.name().isEmpty() ? Texts.str("toast.someone") : f.name(),
                    textW));
                cardSub = Texts.visual(Texts.fit(small, Texts.str("host.invite.line"), textW));
                cardJoin.visible = true;
                PanelWidget.place(cardJoin, x + width - 6 - jw, y + (CARD_H - ActionButton.HEIGHT) / 2, jw,
                    ActionButton.HEIGHT);
                return y + CARD_H + 8;
            }
        }
        if (h.canInvite()) {
            card = Card.INVITE;
            cardY = y;
            cardOpen.visible = true;
            PanelWidget.place(cardOpen, x, y, width, ActionButton.HEIGHT);
            return y + ActionButton.HEIGHT + 8;
        }
        // The line alone keeps the cards' gap to the page's rows.
        return cardNoticeY >= 0 ? y + 4 : y;
    }

    /** "Dünyanda: Mert, Ayşe" for the friends in the world now, or "Henüz kimse yok.". */
    private static String inWorldLine(OverlayState.HostView view) {
        String names = guestNames(view);
        return names.isEmpty() ? Texts.str("host.none") : Texts.str("host.card.in", names);
    }

    /** "Mert, Ayşe" for the friends in the world now, or empty. */
    private static String guestNames(OverlayState.HostView view) {
        if (view == null) {
            return "";
        }
        StringBuilder b = new StringBuilder();
        for (OverlayState.HostGuest g : view.guests()) {
            if (g.in()) {
                if (b.length() > 0) {
                    b.append(", ");
                }
                b.append(g.label().isEmpty() ? g.name() : g.label());
            }
        }
        return b.toString();
    }

    void drawCard(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        if (cardNoticeY >= 0) {
            drawNotice(g, cardNoticeY, cardNoticeH, cardNoticeLines);
        }
        if (cardY < 0) {
            return;
        }
        switch (card) {
            case INVITE -> render(g, cardOpen, mouseX, mouseY);
            case OPEN, WORLD_INVITE -> {
                Draw.frame(g, Frames.CARD, x0, cardY, cw, CARD_H);
                float sx = x0 + 8;
                float sy = cardY + (CARD_H - PixelSprites.SIZE) / 2f;
                PixelSprites.draw(g, PixelSprites.Sprite.WORLD, sx, sy);
                float tx = sx + PixelSprites.SIZE + 7;
                Draw.text(g, Fonts.strong(), cardTitle, tx,
                    Fonts.centeredY(Fonts.Face.STRONG, cardY + (cardSub == null ? CARD_H / 2f : 11f)), Theme.BONE_100);
                if (cardSub != null) {
                    Draw.text(g, Fonts.small(), cardSub, tx, Fonts.centeredY(Fonts.Face.SMALL, cardY + 21f),
                        Theme.ASH_300);
                }
                render(g, card == Card.OPEN ? cardManage : cardJoin, mouseX, mouseY);
            }
            default -> {
            }
        }
    }

    private static void render(GuiGraphicsExtractor g, AbstractWidget w, int mouseX, int mouseY) {
        if (w.visible) {
            w.extractRenderState(g, mouseX, mouseY, 0f);
        }
    }

    /** Whether the state changed since the page was last laid out (the screen asks before each frame). */
    boolean stale() {
        return state.revision() != seenRevision;
    }

    void seen() {
        seenRevision = state.revision();
    }
}
