package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import gg.projects.overlay.ui.draw.Shapes;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * The overlay panel: a drawer on the right over the running game (it never pauses, singleplayer keeps
 * ticking, the world stays visible under a light dim). Header with the connection and settings; notices
 * when there is something to say (the launcher is away, the first time, "Rahatsız etmeyin"); a crew call
 * with its answers; then chats (conversation chips, the conversation, the message field) or friends, or the
 * settings page.
 *
 * <p>Chat extras (CHAT2): a sticker button in the message field opens the sticker picker; "Yanıtla" puts a
 * strip over the field ("Yanıtlanıyor: Ayşe", Esc or ✕ takes it away); "Ayşe yazıyor…" floats at the bottom of
 * the conversation while someone writes (and the player's own writing is told, at most every 4 s); a reaction
 * that did not go says why in the same place for a moment. Esc closes what is open over the panel first.
 *
 * <p>Widgets live as long as the panel: launcher data only updates what they show, so focus, the draft and
 * the caret stay where they are. It slides in on the drawer curve (opacity only under reduced motion);
 * closing hands the last picture to the HUD, which fades it out while the game already has the input.
 */
public final class OverlayScreen extends Screen {
    public enum Page { CHATS, FRIENDS, SETTINGS }

    private static final int P = 10;
    private static final int HEADER_H = 30;
    private static final int COMPOSER_H = 22;
    private static final int REPLY_H = 20;
    private static final int LINE_PLATE_H = 14;
    private static final long ENTER_MS = 220;
    private static final long EXIT_MS = 140;
    private static final long PAGE_MS = 220;
    private static final String[] ANSWERS = {"coming", "soon", "cant"};

    private record Notice(Icon icon, int iconColor, List<FormattedCharSequence> lines, ActionButton button,
        int y, int height) {
    }

    private final OverlayClient overlay;
    private final OverlayState state;
    private final IconButton gear;
    private final IconButton back;
    private final ActionButton retry;
    private final ActionButton hintOk;
    private final ActionButton[] answers = new ActionButton[3];
    private final ActionButton callJoin;
    private final Segmented tabs;
    private final ChipRow chips;
    private final MessageList messages;
    private final QuickReplies quick;
    private final ComposerField field;
    private final SendButton send;
    private final FriendList friends;
    private final SettingsList settingsList;
    private final IconButton stickerButton;
    private final IconButton replyClose;
    private final StickerPicker picker;
    private final Tween lineAlpha = new Tween(0f);
    private final List<Notice> notices = new ArrayList<>();

    private Page page;
    private Page lastListPage;
    private String conversation;
    private int seenRevision = -1;
    private int panelX;
    private int panelW;
    private int dndY = -1;
    private int callY = -1;
    private int callH;
    private List<FormattedCharSequence> callTitle = List.of();
    private String callCounts = "";
    private OverlayState.CrewCall call;
    private String pendingAnswerCall;
    private String pendingAnswer;
    private int composerY;
    private String composerNote;
    private long openedAt;
    private long pageChangedAt;
    private boolean closing;
    private String titleText = "";
    private String settingsTitle = "";
    private String statusText = "";
    private BridgeClient.Status statusShown;
    private String dndText = "";
    /** The message the player answers (CHAT2), and the strip over the field that says so. */
    private OverlayState.Reply replyTo;
    private int replyY = -1;
    private FormattedCharSequence replyTitle;
    private FormattedCharSequence replyQuote;
    /** "Ayşe yazıyor…" or a note: what the line at the bottom of the conversation says now. */
    private FormattedCharSequence lineText;
    private float lineWidth;
    private boolean lineIsNote;
    /** The line is coming (it rises 3 px as it fades in, like the launcher's pill); going, it only fades. */
    private boolean lineComing;
    private int lineTypingRevision = -1;
    private long lineChangesAt;
    private OverlayState.Note lineNote;
    private String lineConversation;
    private String lastDraft = "";

    public OverlayScreen(OverlayClient overlay) {
        super(Texts.tr("panel.title"));
        this.overlay = overlay;
        this.state = overlay.state();
        gear = new IconButton(Icon.GEAR, Texts.tr("settings.title"), () -> setPage(Page.SETTINGS));
        back = new IconButton(Icon.BACK, Texts.tr("panel.back"), () -> setPage(lastListPage));
        retry = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("retry"), null, overlay::retry);
        hintOk = new ActionButton(ActionButton.Style.GHOST, Texts.tr("hint.ok"), null,
            () -> overlay.updateSettings(s -> s.withHintSeen(true)));
        for (int i = 0; i < ANSWERS.length; i++) {
            String answer = ANSWERS[i];
            answers[i] = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("call." + answer), null,
                () -> answer(answer));
        }
        callJoin = new ActionButton(ActionButton.Style.GHOST, Texts.tr("call.join"), Icon.JOIN, this::joinCall);
        tabs = new Segmented(Texts.tr("panel.tabs"), new Component[] {Texts.tr("chats"), Texts.tr("friends")}, 0,
            i -> setPage(i == 0 ? Page.CHATS : Page.FRIENDS));
        chips = new ChipRow(this::openConversation);
        messages = new MessageList(new MessageList.Actions() {
            @Override
            public void retry(OverlayState.ChatLine line) {
                OverlayState.ChatLine taken = state.takeFailed(line.clientId, now());
                if (taken != null && conversation != null) {
                    if (taken.sticker != null) {
                        overlay.sendSticker(conversation, taken.sticker, taken.reply);
                    } else {
                        overlay.sendChat(conversation, taken.text, taken.reply);
                    }
                    messages.toBottom();
                }
            }

            @Override
            public void rewrite(OverlayState.ChatLine line) {
                OverlayState.ChatLine taken = state.takeFailed(line.clientId, now());
                if (taken != null) {
                    field.setValue(taken.text);
                    lastDraft = field.getValue();
                    if (taken.reply != null && taken.reply.sid() > 0) {
                        setReply(taken.reply);
                    }
                    setFocused(field);
                }
            }

            @Override
            public void discard(OverlayState.ChatLine line) {
                state.takeFailed(line.clientId, now());
            }

            @Override
            public void reply(OverlayState.ChatLine line) {
                startReply(line);
            }

            @Override
            public void react(OverlayState.ChatLine line, String emoji, String shown, boolean on) {
                if (conversation != null) {
                    overlay.react(conversation, line, emoji, shown, on);
                }
            }

            @Override
            public void openLink(String url, boolean trusted) {
                LinkScreen ask = LinkScreen.of(OverlayScreen.this, url);
                if (ask != null) {
                    Compat.setScreen(minecraft, ask);
                }
            }
        });
        quick = new QuickReplies(this::sendText);
        field = new ComposerField(Texts.tr("chat.field"));
        field.setMaxLength(Protocol.MAX_MESSAGE_CHARS);
        send = new SendButton(this::canSend, this::sendDraft);
        friends = new FriendList(this::joinFriend);
        stickerButton = new IconButton(Icon.STICKER, Texts.tr("chat.stickers"), this::togglePicker);
        replyClose = new IconButton(Icon.CLOSE, Texts.tr("chat.reply.cancel"), this::clearReply);
        picker = new StickerPicker(this::sendSticker);
        picker.visible = false;
        settingsList = new SettingsList(overlay::settings, overlay::updateSettings, overlay.key()::keyName,
            () -> Compat.setScreen(minecraft, new KeyBindsScreen(this, minecraft.options)));
        lastListPage = state.has("chat") || !state.welcomed() ? Page.CHATS : Page.FRIENDS;
        page = overlay.lastPage() == Page.SETTINGS ? lastListPage : overlay.lastPage();
        if (page == Page.CHATS && state.welcomed() && !state.has("chat")) {
            page = Page.FRIENDS;
        }
        if (page != Page.SETTINGS) {
            lastListPage = page;
        }
        conversation = pickConversation();
    }

    static long now() {
        return System.nanoTime() / 1_000_000L;
    }

    /** The conversation on screen; its new messages do not raise toasts. */
    public String openConversation() {
        return page == Page.CHATS ? conversation : null;
    }

    public Page page() {
        return page;
    }

    public boolean reducedMotion() {
        return overlay.reducedMotion();
    }

    /** The message field (the scenario driver types into it through the normal input path). */
    public ComposerField input() {
        return field;
    }

    /** Shows a page as its tab or button would (the scenario driver uses it too). */
    public void showPage(Page next) {
        setPage(next);
    }

    /** "Katıl" on a friend, as the list's button does. */
    public void join(String friendId) {
        joinFriend(friendId);
    }

    /** The settings list (the scenario driver scrolls it). */
    public SettingsList settings() {
        return settingsList;
    }

    /** Shows a conversation, as its chip does (the scenario driver uses it too). */
    public void showConversation(String id) {
        openConversation(id);
    }

    /** Puts the keyboard on the conversation, as Tab would (the scenario driver picks messages with the keys). */
    public void focusMessages() {
        PanelInput.usedKeyboard();
        setFocused(messages);
    }

    /** Opens or closes the sticker picker, as its button does. */
    public void toggleStickers() {
        togglePicker();
    }

    /** Asks about a link as pressing it in a message does (only one the mod would ever open). */
    public void askLink(String url) {
        LinkScreen ask = LinkScreen.of(this, url);
        if (ask != null) {
            Compat.setScreen(minecraft, ask);
        }
    }

    private String pickConversation() {
        String wanted = overlay.preferredConversation();
        if (wanted != null && state.conversation(wanted) != null) {
            return wanted;
        }
        String first = null;
        for (OverlayState.Conversation c : state.conversations()) {
            if (first == null) {
                first = c.id();
            }
            if (c.unread() > 0) {
                return c.id();
            }
        }
        return first;
    }

    @Override
    protected void init() {
        if (openedAt == 0) {
            openedAt = now();
            pageChangedAt = 0;
        }
        // The picker floats over the conversation: it is asked about a click before the list under it.
        for (AbstractWidget w : new AbstractWidget[] {picker, gear, back, retry, hintOk, answers[0], answers[1],
            answers[2], callJoin, tabs, chips, messages, quick, replyClose, field, stickerButton, send, friends,
            settingsList}) {
            addWidget(w);
        }
        Shapes.prepare();
        if (conversation != null) {
            messages.open(conversation, isCrew(conversation), unreadOf(conversation));
            overlay.openConversation(conversation);
        }
        sync(true);
        setInitialFocus(initialFocus());
    }

    @Override
    protected void repositionElements() {
        layout();
    }

    private AbstractWidget initialFocus() {
        return switch (page) {
            case CHATS -> composerOpen() ? field : chips;
            case FRIENDS -> friends;
            case SETTINGS -> settingsList;
        };
    }

    private boolean isCrew(String conv) {
        OverlayState.Conversation c = state.conversation(conv);
        return c != null && c.crew();
    }

    private int unreadOf(String conv) {
        OverlayState.Conversation c = state.conversation(conv);
        return c == null ? 0 : c.unread();
    }

    /** Called by the overlay on the render thread after every launcher frame. */
    public void onStateChanged() {
        sync(false);
    }

    private void sync(boolean force) {
        if (!force && state.revision() == seenRevision) {
            return;
        }
        seenRevision = state.revision();
        if (conversation == null || state.conversation(conversation) == null) {
            String next = pickConversation();
            if (next != null && !next.equals(conversation)) {
                conversation = next;
                messages.open(next, isCrew(next), unreadOf(next));
                overlay.openConversation(next);
            } else if (next == null) {
                conversation = null;
                messages.open(null, false, 0);
            }
        }
        chips.update(state, conversation);
        layout();
    }

    private void setPage(Page next) {
        if (next == null || next == page) {
            return;
        }
        if (next == Page.SETTINGS && !overlay.settings().hintSeen()) {
            overlay.updateSettings(s -> s.withHintSeen(true));
        }
        if (page != Page.SETTINGS) {
            lastListPage = page;
        }
        if (next != Page.SETTINGS) {
            lastListPage = next;
            tabs.select(next == Page.CHATS ? 0 : 1);
        }
        page = next;
        overlay.rememberPage(next);
        pageChangedAt = now();
        picker.close();
        layout();
        setFocused(initialFocus());
        if (page == Page.CHATS && conversation != null) {
            overlay.openConversation(conversation);
        }
    }

    private void openConversation(String id) {
        if (id == null || id.equals(conversation)) {
            return;
        }
        conversation = id;
        overlay.rememberConversation(id);
        clearReply();
        picker.close();
        messages.open(id, isCrew(id), unreadOf(id));
        overlay.openConversation(id);
        chips.update(state, id);
        layout();
    }

    private boolean composerOpen() {
        OverlayState.Conversation c = state.conversation(conversation);
        return c != null && c.writable() && state.online() && state.has("chat");
    }

    private boolean canSend() {
        return composerOpen() && !Protocol.plain(field.getValue(), Protocol.MAX_MESSAGE_CHARS).isEmpty();
    }

    private void sendDraft() {
        String text = Protocol.plain(field.getValue(), Protocol.MAX_MESSAGE_CHARS);
        if (text.isEmpty() || !composerOpen()) {
            return;
        }
        field.setValue("");
        lastDraft = "";
        sendText(text);
    }

    private void sendText(String text) {
        if (conversation == null || !composerOpen()) {
            return;
        }
        overlay.sendChat(conversation, text, replying() ? replyTo : null);
        clearReply();
        messages.toBottom();
    }

    private void sendSticker(String sticker) {
        if (conversation == null || !composerOpen() || !state.has("stickers")) {
            return;
        }
        picker.close();
        overlay.sendSticker(conversation, sticker, replying() ? replyTo : null);
        clearReply();
        messages.toBottom();
        setFocused(field);
    }

    // ---- Replies, stickers, "yazıyor…" (CHAT2) --------------------------------------------------------

    private boolean replying() {
        return replyTo != null && state.has("replies") && conversation != null;
    }

    private void startReply(OverlayState.ChatLine line) {
        if (!composerOpen() || !state.has("replies") || line.sid <= 0) {
            return;
        }
        setReply(OverlayState.quoteOf(line));
        setFocused(field);
    }

    private void setReply(OverlayState.Reply quote) {
        replyTo = quote;
        Font small = Fonts.small();
        int room = Math.max(40, panelW - 2 * P - 8 - 12 - 6 - IconButton.SIZE - 4);
        String name = quote.mine() ? Texts.str("chat.quote.you") : quote.from().isEmpty() ? Texts.str("toast.someone")
            : quote.from();
        replyTitle = Texts.visual(Texts.fit(small, Texts.str("chat.replying", name), room));
        String said;
        if (quote.sticker() != null) {
            StickerSet.Sticker sticker = StickerSet.get().find(quote.sticker());
            said = sticker == null ? Texts.str("chat.sticker") : Texts.str("chat.sticker.named",
                sticker.name(Texts.turkish()));
        } else {
            said = quote.text() == null ? "" : quote.text();
        }
        replyQuote = Texts.visual(Texts.fit(small, said, room));
        layout();
    }

    private void clearReply() {
        if (replyTo != null) {
            replyTo = null;
            layout();
        }
    }

    private void togglePicker() {
        if (picker.isOpen()) {
            picker.close();
            setFocused(field);
        } else if (composerOpen() && state.has("stickers") && !StickerSet.get().isEmpty()) {
            messages.dismiss();
            picker.open(overlay.settings().recentStickers(), now());
            layout();
            setFocused(picker);
        }
    }

    /** The field changed: the player writes, so "yazıyor…" may go (only with text in it). */
    private void draftChanged() {
        String value = field.getValue();
        if (value.equals(lastDraft)) {
            return;
        }
        lastDraft = value;
        if (!value.isBlank() && conversation != null && composerOpen() && state.has("typing")) {
            overlay.typing(conversation);
        }
    }

    private void answer(String answer) {
        if (call == null || !state.online()) {
            return;
        }
        pendingAnswerCall = call.id();
        pendingAnswer = answer;
        overlay.answer(call.id(), answer);
        layout();
    }

    private void joinFriend(String id) {
        OverlayState.Friend f = state.friend(id);
        if (f == null || !canJoin(f)) {
            return;
        }
        boolean projects = f.onProjects();
        String host = projects ? OverlayClient.PROJECTS_HOST : f.host();
        if (host == null) {
            return;
        }
        Compat.setScreen(minecraft, new JoinScreen(this, new JoinScreen.Target(f.id(), f.name(), host, projects)));
    }

    private void joinCall() {
        if (call == null) {
            return;
        }
        boolean projects = "projects".equals(call.where());
        String host = projects ? OverlayClient.PROJECTS_HOST : call.host();
        if (host == null) {
            return;
        }
        Compat.setScreen(minecraft, new JoinScreen(this, new JoinScreen.Target(null, call.from(), host, projects)));
    }

    private boolean canJoin(OverlayState.Friend f) {
        if (!f.joinable() || !state.online()) {
            return false;
        }
        if (f.onProjects()) {
            return !overlay.onProjects();
        }
        return f.host() != null && !overlay.onServer(f.host());
    }

    private boolean callJoinable(OverlayState.CrewCall c) {
        if (c == null || c.where() == null || !state.online()) {
            return false;
        }
        if (c.where().equals("projects")) {
            return !overlay.onProjects();
        }
        return c.host() != null && !overlay.onServer(c.host());
    }

    // ---- Layout -------------------------------------------------------------------------------------

    private void layout() {
        if (font == null) {
            return;
        }
        long now = now();
        boolean reduced = reducedMotion();
        titleText = getTitle().getString();
        settingsTitle = Texts.str("settings.title");
        dndText = Texts.str("dnd");
        statusShown = state.status();
        statusText = Texts.str("status." + statusShown.name().toLowerCase(java.util.Locale.ROOT));
        tabs.setReducedMotion(reduced);
        chips.setReducedMotion(reduced);
        messages.setReducedMotion(reduced);
        friends.setReducedMotion(reduced);
        settingsList.setReducedMotion(reduced);
        panelW = Math.max(220, Math.min(300, Math.round(width * 0.38f)));
        panelW = Math.min(panelW, width - 24);
        panelX = width - panelW;
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        hideAll();
        boolean settingsPage = page == Page.SETTINGS;
        gear.visible = !settingsPage;
        back.visible = settingsPage;
        PanelWidget.place(gear, panelX + panelW - P - IconButton.SIZE + 3, (HEADER_H - IconButton.SIZE) / 2,
            IconButton.SIZE, IconButton.SIZE);
        PanelWidget.place(back, x0 - 4, (HEADER_H - IconButton.SIZE) / 2, IconButton.SIZE, IconButton.SIZE);
        int y = HEADER_H + 6;
        dndY = -1;
        if (state.online() && state.dnd()) {
            dndY = y;
            y += 14;
        }
        notices.clear();
        y = layoutNotices(x0, cw, y);
        call = settingsPage ? null : state.activeCall(System.currentTimeMillis());
        callY = -1;
        if (call != null && state.has("calls")) {
            y = layoutCall(x0, cw, y);
        }
        if (settingsPage) {
            settingsList.visible = true;
            PanelWidget.place(settingsList, x0, y, cw, Math.max(40, height - P - y));
            return;
        }
        boolean chat = state.has("chat") || !state.welcomed();
        if (chat && state.signedIn()) {
            tabs.visible = true;
            PanelWidget.place(tabs, x0, y, cw, Segmented.HEIGHT);
            tabs.setMarker(0, page != Page.CHATS && state.unreadTotal() > 0);
            y += Segmented.HEIGHT + 8;
        }
        if (page == Page.FRIENDS || !chat) {
            friends.visible = true;
            PanelWidget.place(friends, x0, y, cw, Math.max(30, height - P - y));
            friends.refresh(state, this::canJoin);
            return;
        }
        chips.visible = !chips.isEmpty();
        if (chips.visible) {
            PanelWidget.place(chips, x0, y, cw, ChipRow.HEIGHT);
            y += ChipRow.HEIGHT + 6;
        }
        composerY = height - P - COMPOSER_H;
        int listBottom = composerY - 6;
        composerNote = null;
        if (conversation == null) {
            listBottom = height - P;
        } else if (composerOpen()) {
            field.visible = true;
            field.setEditable(true);
            send.visible = true;
            boolean stickers = state.has("stickers") && !StickerSet.get().isEmpty();
            int stickerRoom = stickers ? IconButton.SIZE + 2 : 0;
            PanelWidget.place(field, x0 + 8, composerY + (COMPOSER_H - 12) / 2,
                cw - 8 - SendButton.SIZE - 6 - stickerRoom, 12);
            PanelWidget.place(send, x0 + cw - SendButton.SIZE - 2, composerY + (COMPOSER_H - SendButton.SIZE) / 2,
                SendButton.SIZE, SendButton.SIZE);
            stickerButton.visible = stickers;
            if (stickers) {
                PanelWidget.place(stickerButton, send.getX() - 2 - IconButton.SIZE,
                    composerY + (COMPOSER_H - IconButton.SIZE) / 2, IconButton.SIZE, IconButton.SIZE);
                boolean open = picker.isOpen();
                stickerButton.setColors(open ? Theme.GOLD_300 : Theme.ASH_400, open ? Theme.GOLD_200 : Theme.BONE_100);
            } else {
                picker.close();
            }
            field.setPlaceholder(Texts.tr(isCrew(conversation) ? "chat.hint.crew" : "chat.hint"));
            int above = composerY;
            replyY = -1;
            if (replying()) {
                replyY = composerY - 4 - REPLY_H;
                replyClose.visible = true;
                PanelWidget.place(replyClose, x0 + cw - IconButton.SIZE - 2, replyY + (REPLY_H - IconButton.SIZE) / 2,
                    IconButton.SIZE, IconButton.SIZE);
                above = replyY;
                listBottom = replyY - 6;
            }
            if (isCrew(conversation)) {
                quick.visible = true;
                int qy = above - 6 - QuickReplies.HEIGHT;
                PanelWidget.place(quick, x0, qy, cw, QuickReplies.HEIGHT);
                quick.setHidden(!field.getValue().isEmpty(), now);
                listBottom = qy - 6;
            }
            if (picker.isOpen()) {
                // Over the conversation, right above the field. On a short screen (GUI 3 at 720p, the recent row
                // shown) it rises over the tabs as well: it never covers the field it belongs to.
                int ph = StickerPicker.height(cw, !overlay.settings().recentStickers().isEmpty());
                int py = Math.max(HEADER_H + 6, above - 4 - ph);
                PanelWidget.place(picker, x0, py, cw, Math.max(20, Math.min(ph, above - 4 - py)));
            }
        } else {
            composerNote = composerReason();
            picker.close();
            replyY = -1;
        }
        messages.visible = true;
        PanelWidget.place(messages, x0, y, cw, Math.max(20, listBottom - y));
        // "Yazıyor…" and the reaction notes float over the list's bottom: the newest message keeps clear of them.
        messages.setFloatingLine(state.has("typing") || state.has("reactions") ? LINE_PLATE_H : 0);
        messages.refresh(state, now);
        picker.setReducedMotion(reduced);
    }

    private String composerReason() {
        if (!state.online()) {
            return Texts.str("chat.closed.offline");
        }
        if (!state.has("chat")) {
            return Texts.str("chat.closed.off");
        }
        return Texts.str("chat.closed.not_allowed");
    }

    private void hideAll() {
        for (AbstractWidget w : new AbstractWidget[] {retry, hintOk, answers[0], answers[1], answers[2], callJoin,
            tabs, chips, messages, quick, field, send, friends, settingsList, stickerButton, replyClose}) {
            w.visible = false;
        }
        if (page != Page.CHATS || conversation == null) {
            picker.close();
        }
    }

    private int layoutNotices(int x0, int cw, int y) {
        Font ui = Fonts.ui();
        int textW = cw - 8 - 10 - 6 - 8;
        BridgeClient.Status status = state.status();
        if (status != BridgeClient.Status.ONLINE) {
            boolean retryable = status == BridgeClient.Status.OFFLINE || status == BridgeClient.Status.ENDED;
            String line = switch (status) {
                case CONNECTING -> Texts.str("detail.connecting");
                case OUTDATED -> Texts.str("detail.outdated");
                case REJECTED -> Texts.str("detail.rejected");
                default -> Texts.str("detail.closed");
            };
            String detail = switch (status) {
                case OFFLINE -> Texts.str("detail.offline");
                case ENDED -> "disabled".equals(state.statusReason()) ? Texts.str("detail.disabled")
                    : "game-start-close".equals(state.statusReason()) ? Texts.str("detail.closes")
                    : Texts.str("detail.ended");
                default -> null;
            };
            List<FormattedCharSequence> lines = new ArrayList<>(Texts.wrap(Fonts.ui(), line, textW, 3));
            if (detail != null) {
                lines.addAll(Texts.wrap(ui, detail, textW, 2));
            }
            y = addNotice(x0, cw, y, status == BridgeClient.Status.CONNECTING ? Icon.CLOCK : Icon.UNPLUGGED,
                status == BridgeClient.Status.CONNECTING ? Theme.ASH_300 : Theme.FLARE_300, lines,
                retryable ? retry : null);
        } else if (!state.signedIn()) {
            y = addNotice(x0, cw, y, Icon.INFO, Theme.ASH_300,
                Texts.wrap(ui, Texts.str("detail.signed_out"), textW, 3), null);
        } else if (!overlay.settings().hintSeen()) {
            y = addNotice(x0, cw, y, Icon.GEAR, Theme.GOLD_300,
                Texts.wrap(ui, Texts.str("hint.first"), textW, 3), hintOk);
        }
        return y;
    }

    private int addNotice(int x0, int cw, int y, Icon icon, int color, List<FormattedCharSequence> lines,
        ActionButton button) {
        int h = 8 + lines.size() * 10 + 8;
        if (button != null) {
            button.visible = true;
            int bw = button.preferredWidth();
            h += 2 + ActionButton.HEIGHT;
            PanelWidget.place(button, x0 + cw - 8 - bw, y + h - 8 - ActionButton.HEIGHT, bw, ActionButton.HEIGHT);
        }
        notices.add(new Notice(icon, color, lines, button, y, h));
        return y + h + 6;
    }

    private int layoutCall(int x0, int cw, int y) {
        callY = y;
        Font strong = Fonts.strong();
        int textW = cw - 8 - 12 - 6 - 8;
        String title = call.own() ? Texts.str("call.own", call.crew())
            : Texts.str("call.title", call.from(), call.crew());
        callTitle = Texts.wrap(strong, title, textW, 2);
        callCounts = counts(call);
        int h = 8 + callTitle.size() * 10 + 2 + 9 + 8;
        boolean joinable = callJoinable(call);
        if (!call.own()) {
            h += 2 + ActionButton.HEIGHT;
            int bw = (cw - 16 - 8) / 3;
            int by = y + h - 8 - ActionButton.HEIGHT;
            String mine = call.id().equals(pendingAnswerCall) && call.mine() == null ? pendingAnswer : call.mine();
            for (int i = 0; i < answers.length; i++) {
                answers[i].visible = true;
                answers[i].active = state.online();
                answers[i].setSelected(ANSWERS[i].equals(mine));
                PanelWidget.place(answers[i], x0 + 8 + i * (bw + 4), by, bw, ActionButton.HEIGHT);
            }
        }
        if (joinable) {
            callJoin.visible = true;
            int jw = callJoin.preferredWidth();
            PanelWidget.place(callJoin, x0 + cw - 8 - jw, y + 8 + callTitle.size() * 10 - 3, jw, ActionButton.HEIGHT);
        }
        callH = h;
        return y + h + 8;
    }

    private static String counts(OverlayState.CrewCall c) {
        List<String> parts = new ArrayList<>(3);
        if (c.coming() > 0) {
            parts.add(Texts.str("call.count.coming", c.coming()));
        }
        if (c.soon() > 0) {
            parts.add(Texts.str("call.count.soon", c.soon()));
        }
        if (c.cant() > 0) {
            parts.add(Texts.str("call.count.cant", c.cant()));
        }
        return parts.isEmpty() ? Texts.str("call.count.none") : String.join(", ", parts);
    }

    // ---- Input --------------------------------------------------------------------------------------

    @Override
    public boolean keyPressed(KeyEvent event) {
        boolean typing = field.isFocused() && field.visible;
        if (overlay.key().closes(event, typing)) {
            close();
            return true;
        }
        if (event.key() == InputConstants.KEY_ESCAPE && dismissInner()) {
            // Esc closes what is open over the panel first: the picker, the quick reactions, the reply.
            return true;
        }
        if (typing && event.isConfirmation()) {
            sendDraft();
            return true;
        }
        if (event.isCycleFocus() || event.isUp() || event.isDown() || event.isLeft() || event.isRight()) {
            PanelInput.usedKeyboard();
        }
        boolean handled = super.keyPressed(event);
        if (typing) {
            // Pasting, deleting: the draft may have changed without a character typed.
            draftChanged();
        }
        return handled;
    }

    /** Closes the innermost thing open over the panel; false when there is none. */
    private boolean dismissInner() {
        if (picker.isOpen()) {
            picker.close();
            layout();
            setFocused(field.visible ? field : chips);
            return true;
        }
        if (messages.dismiss()) {
            return true;
        }
        if (replyTo != null) {
            clearReply();
            return true;
        }
        return false;
    }

    @Override
    public boolean charTyped(CharacterEvent event) {
        boolean handled = super.charTyped(event);
        if (page == Page.CHATS && quick.visible) {
            quick.setHidden(!field.getValue().isEmpty(), now());
        }
        if (field.isFocused()) {
            draftChanged();
        }
        return handled;
    }

    @Override
    public boolean mouseClicked(MouseButtonEvent event, boolean doubleClick) {
        if (overlay.key().matchesMouse(event)) {
            close();
            return true;
        }
        if (event.x() < panelX) {
            // A click on the world goes back to the game.
            close();
            return true;
        }
        if (picker.isOpen() && !picker.isMouseOver(event.x(), event.y())
            && !stickerButton.isMouseOver(event.x(), event.y())) {
            // A click beside the picker closes it, and only that.
            picker.close();
            layout();
            return true;
        }
        return super.mouseClicked(event, doubleClick);
    }

    @Override
    public void onClose() {
        close();
    }

    /** Closes to the game: input returns at once, the picture fades out on the HUD. */
    public void close() {
        if (closing) {
            return;
        }
        closing = true;
        long start = now();
        boolean reduced = reducedMotion();
        ToastLayer layer = overlay.toasts();
        layer.setAfterimage((g, now) -> {
            float q = Math.min(1f, (now - start) / (float) EXIT_MS);
            if (q >= 1f) {
                return false;
            }
            float eased = Ease.OUT.at(q);
            drawScrim(g, 1f - eased);
            drawPanel(g, -1, -1, now, 1f - eased, reduced ? 0f : eased * 12f);
            return true;
        });
        Compat.setScreen(minecraft, null);
    }

    @Override
    public void added() {
        closing = false;
        overlay.panelOpened();
    }

    @Override
    public void removed() {
        picker.close();
        overlay.panelClosed();
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public void tick() {
        if (page == Page.CHATS && quick.visible) {
            quick.setHidden(!field.getValue().isEmpty(), now());
        }
        if (call != null && !call.liveAt(System.currentTimeMillis())) {
            layout();
        }
    }

    // ---- Drawing ------------------------------------------------------------------------------------

    @Override
    public void extractBackground(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        float t = Math.min(1f, (now() - openedAt) / (float) ENTER_MS);
        drawScrim(g, Ease.OUT.at(t));
    }

    private void drawScrim(GuiGraphicsExtractor g, float alpha) {
        Draw.fill(g, 0, 0, width, height, Theme.fade(Theme.SCRIM, alpha));
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        long started = Perf.start();
        sync(false);
        Shapes.prepare();
        long now = now();
        float t = Math.min(1f, (now - openedAt) / (float) ENTER_MS);
        float alpha = Ease.OUT.at(Math.min(1f, (now - openedAt) / 160f));
        float dx = reducedMotion() ? 0f : (1f - Ease.DRAWER.at(t)) * 18f;
        drawPanel(g, mouseX, mouseY, now, alpha, dx);
        Perf.panel(started);
    }

    /** The panel as it stands, without its entrance (the dialog over it draws this). */
    void drawStill(GuiGraphicsExtractor g, long now) {
        drawScrim(g, 1f);
        drawPanel(g, -1, -1, now, 1f, 0f);
    }

    private void drawPanel(GuiGraphicsExtractor g, int mouseX, int mouseY, long now, float alpha, float dx) {
        if (alpha <= 0.004f) {
            return;
        }
        Draw.pushAlpha(alpha);
        g.pose().pushMatrix();
        g.pose().translate(dx, 0f);
        Draw.strip(g, ShapeSet.PANEL_SHADOW, panelX - ShapeSet.PANEL_SHADOW_WIDTH, 0, ShapeSet.PANEL_SHADOW_WIDTH,
            height);
        Draw.fill(g, panelX, 0, width, height, ShapeSet.PANEL_FILL);
        Draw.vhairline(g, panelX, 0, height, Theme.withAlpha(0xFFFFFF, 0.08f));
        // A layer per section: every element of the panel overlaps its fill, so in one layer placing each
        // element would scan all the others (Draw.stratum).
        Draw.stratum(g);
        drawHeader(g, mouseX, mouseY, now);
        for (Notice n : notices) {
            drawNotice(g, n, mouseX, mouseY);
        }
        if (dndY >= 0) {
            float x0 = panelX + P;
            Draw.icon(g, Icon.BELL, 8, x0, dndY + 2, Theme.GOLD_300);
            Draw.text(g, Fonts.small(), dndText, x0 + 12, Fonts.centeredY(Fonts.Face.SMALL, dndY + 6), Theme.ASH_300);
        }
        if (callY >= 0 && call != null) {
            drawCall(g, mouseX, mouseY);
        }
        if (messages.visible) {
            // Cheap unless something changed (or a message waited too long for its answer).
            messages.refresh(state, now);
        }
        float p = pageChangedAt == 0 ? 1f : Math.min(1f, (now - pageChangedAt) / (float) PAGE_MS);
        float pe = Ease.OUT.at(p);
        Draw.pushAlpha(pe);
        g.pose().pushMatrix();
        g.pose().translate(0f, reducedMotion() ? 0f : (1f - pe) * 4f);
        Draw.stratum(g);
        render(g, tabs, mouseX, mouseY);
        render(g, chips, mouseX, mouseY);
        Draw.stratum(g);
        render(g, messages, mouseX, mouseY);
        if (messages.visible) {
            drawConversationLine(g, now);
        }
        Draw.stratum(g);
        render(g, quick, mouseX, mouseY);
        if (page == Page.CHATS && conversation != null) {
            drawReplyStrip(g, mouseX, mouseY);
            drawComposer(g, mouseX, mouseY);
        }
        if (picker.visible) {
            Draw.stratum(g);
            render(g, picker, mouseX, mouseY);
        }
        Draw.stratum(g);
        render(g, friends, mouseX, mouseY);
        render(g, settingsList, mouseX, mouseY);
        g.pose().popMatrix();
        Draw.popAlpha();
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    private void render(GuiGraphicsExtractor g, AbstractWidget w, int mouseX, int mouseY) {
        if (w.visible) {
            w.extractRenderState(g, mouseX, mouseY, 0f);
        }
    }

    private void drawHeader(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        int x0 = panelX + P;
        float mid = HEADER_H / 2f;
        if (page == Page.SETTINGS) {
            render(g, back, mouseX, mouseY);
            Draw.text(g, Fonts.display(), settingsTitle, x0 + 15, Fonts.centeredY(Fonts.Face.DISPLAY, mid),
                Theme.BONE_50);
        } else {
            Draw.text(g, Fonts.display(), titleText, x0, Fonts.centeredY(Fonts.Face.DISPLAY, mid), Theme.BONE_50);
            render(g, gear, mouseX, mouseY);
            if (!overlay.settings().hintSeen() && state.online()) {
                Draw.disc(g, 5, gear.getX() + 13, gear.getY() + 3, Theme.GOLD_500);
            }
        }
        if (statusShown != state.status()) {
            layout();
        }
        BridgeClient.Status status = statusShown;
        String label = statusText;
        Font small = Fonts.small();
        float right = page == Page.SETTINGS ? panelX + panelW - P : gear.getX() - 4;
        float lx = right - small.width(label);
        int dot = switch (status) {
            case ONLINE -> Theme.JADE_400;
            case CONNECTING -> Theme.ASH_400;
            default -> Theme.FLARE_400;
        };
        Draw.disc(g, 5, lx - 6, mid, dot);
        Draw.text(g, small, label, lx, Fonts.centeredY(Fonts.Face.SMALL, mid), Theme.ASH_400);
        Draw.hairline(g, panelX + P, panelX + panelW - P, HEADER_H, Theme.withAlpha(0xFFFFFF, 0.07f));
    }

    private void drawNotice(GuiGraphicsExtractor g, Notice n, int mouseX, int mouseY) {
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        Draw.frame(g, Frames.CARD, x0, n.y(), cw, n.height());
        Draw.icon(g, n.icon(), 10, x0 + 8, n.y() + 8, n.iconColor());
        float ty = n.y() + 8;
        boolean first = true;
        for (FormattedCharSequence line : n.lines()) {
            Draw.text(g, Fonts.ui(), line, x0 + 8 + 10 + 6, Fonts.centeredY(Fonts.Face.UI, ty + 5),
                first ? Theme.BONE_100 : Theme.ASH_300);
            ty += 10;
            first = false;
        }
        if (n.button() != null) {
            render(g, n.button(), mouseX, mouseY);
        }
    }

    private void drawCall(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        Draw.frame(g, Frames.CARD_GOLD, x0, callY, cw, callH);
        Draw.icon(g, Icon.CALL, 12, x0 + 8, callY + 7, Theme.GOLD_300);
        float ty = callY + 8;
        for (FormattedCharSequence line : callTitle) {
            Draw.text(g, Fonts.strong(), line, x0 + 8 + 12 + 6, Fonts.centeredY(Fonts.Face.STRONG, ty + 5),
                Theme.BONE_50);
            ty += 10;
        }
        ty += 2;
        Draw.text(g, Fonts.small(), callCounts, x0 + 8 + 12 + 6, Fonts.centeredY(Fonts.Face.SMALL, ty + 4.5f),
            Theme.ASH_300);
        for (ActionButton b : answers) {
            render(g, b, mouseX, mouseY);
        }
        render(g, callJoin, mouseX, mouseY);
    }

    private void drawComposer(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        if (composerNote != null) {
            Draw.frame(g, Frames.CARD, x0, composerY, cw, COMPOSER_H);
            Draw.icon(g, Icon.INFO, 8, x0 + 8, composerY + (COMPOSER_H - 8) / 2f, Theme.ASH_400);
            Draw.text(g, Fonts.small(), composerNote, x0 + 20, Fonts.centeredY(Fonts.Face.SMALL, composerY
                + COMPOSER_H / 2f), Theme.ASH_300);
            return;
        }
        if (!field.visible) {
            return;
        }
        boolean focused = field.isFocused();
        boolean lit = mouseX >= x0 && mouseX < x0 + cw && mouseY >= composerY && mouseY < composerY + COMPOSER_H;
        Draw.frame(g, focused ? Frames.FIELD_FOCUS : lit ? Frames.FIELD_HOVER : Frames.FIELD, x0, composerY, cw,
            COMPOSER_H);
        render(g, field, mouseX, mouseY);
        render(g, stickerButton, mouseX, mouseY);
        render(g, send, mouseX, mouseY);
        if (stickerButton.visible && (stickerButton.isHovered() || stickerButton.isFocused())) {
            // The pictures get ready while the pointer is on the button: the picker opens with them.
            StickerArt.prepare(StickerPicker.artSize(cw));
        }
        int length = field.getValue().codePointCount(0, field.getValue().length());
        if (length >= 250) {
            String count = length + "/" + Protocol.MAX_MESSAGE_CHARS;
            Font small = Fonts.small();
            Draw.text(g, small, count, send.getX() - 4 - small.width(count), composerY - 10f,
                length >= Protocol.MAX_MESSAGE_CHARS ? Theme.FLARE_300 : Theme.ASH_400);
        }
    }

    /** "Yanıtlanıyor: Ayşe" and the quoted line over the field, with ✕. */
    private void drawReplyStrip(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        if (replyY < 0 || replyTitle == null) {
            return;
        }
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        Draw.frame(g, Frames.CARD, x0, replyY, cw, REPLY_H);
        Draw.fill(g, x0 + 4, replyY + 4, x0 + 5.5f, replyY + REPLY_H - 4, Theme.withAlpha(Theme.GOLD_400, 0.75f));
        Draw.icon(g, Icon.REPLY, 8, x0 + 9, replyY + 3, Theme.GOLD_300);
        Draw.text(g, Fonts.small(), replyTitle, x0 + 20, Fonts.centeredY(Fonts.Face.SMALL, replyY + 6.5f),
            Theme.ASH_300);
        Draw.text(g, Fonts.small(), replyQuote, x0 + 9, Fonts.centeredY(Fonts.Face.SMALL, replyY + 14.5f),
            Theme.BONE_300);
        render(g, replyClose, mouseX, mouseY);
    }

    /**
     * The line at the bottom of the conversation: a note for a moment (a reaction that did not go), else who
     * writes there ("Ayşe yazıyor…", no moving dots). It floats over the messages: nothing moves when it comes
     * or goes. Its words are made only when they change.
     */
    private void drawConversationLine(GuiGraphicsExtractor g, long now) {
        long wall = System.currentTimeMillis();
        OverlayState.Note note = state.note(wall);
        if (note != null && conversation != null && !note.conversation().equals(conversation)) {
            note = null;
        }
        boolean rebuild = note != lineNote || !java.util.Objects.equals(conversation, lineConversation)
            || state.typingRevision() != lineTypingRevision || wall >= lineChangesAt;
        if (rebuild) {
            lineNote = note;
            lineConversation = conversation;
            lineTypingRevision = state.typingRevision();
            lineChangesAt = Long.MAX_VALUE;
            String words = null;
            lineIsNote = note != null;
            if (note != null) {
                words = Texts.str("chat.note." + note.key());
                lineChangesAt = note.at() + OverlayState.NOTE_MS;
            } else if (conversation != null && state.has("typing")) {
                List<String> names = state.writers(conversation, wall);
                if (!names.isEmpty()) {
                    words = names.size() == 1 ? Texts.str("chat.typing.one", names.get(0))
                        : names.size() == 2 ? Texts.str("chat.typing.two", names.get(0), names.get(1))
                        : Texts.str("chat.typing.more", names.get(0), names.get(1), names.size() - 2);
                }
                lineChangesAt = state.typingEndsAt(conversation);
            }
            if (words != null) {
                Font small = Fonts.small();
                String fitted = Texts.fit(small, words, messages.getWidth() - 12);
                lineText = Texts.visual(fitted);
                lineWidth = small.width(fitted) + 12;
                lineComing = lineAlpha.value(now) < 0.5f;
                lineAlpha.animateTo(1f, now, 160, Ease.OUT);
            } else {
                lineComing = false;
                lineAlpha.animateTo(0f, now, 110, Ease.OUT);
            }
        }
        float a = lineAlpha.value(now);
        if (a <= 0.01f || lineText == null) {
            return;
        }
        float x = messages.getX();
        float rise = lineComing && !reducedMotion() ? (1f - a) * 3f : 0f;
        float y = messages.getY() + messages.getHeight() - LINE_PLATE_H - 2 - (messages.pillShowing() ? 18 : 0)
            + rise;
        Draw.pushAlpha(a);
        Draw.frame(g, Frames.POPOVER, x, y, lineWidth, LINE_PLATE_H);
        Draw.text(g, Fonts.small(), lineText, x + 6, Fonts.centeredY(Fonts.Face.SMALL, y + LINE_PLATE_H / 2f),
            lineIsNote ? Theme.FLARE_300 : Theme.ASH_300);
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        return getTitle();
    }

    /** Keeps the settings toggles in step after the player changed something elsewhere (key binds). */
    public void settingsChanged(OverlaySettings settings) {
        layout();
    }
}
