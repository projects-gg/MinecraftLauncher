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
    /** The panel's pages; {@code WORLD} is world sharing (HOST1), reached from its card or the pause menu. */
    public enum Page { CHATS, FRIENDS, SETTINGS, WORLD }

    private static final int P = 10;
    private static final int HEADER_H = 30;
    private static final int COMPOSER_H = 22;
    private static final int REPLY_H = 20;
    private static final int LINE_PLATE_H = 14;
    /**
     * The conversation keeps at least this much height before crew chat's quick replies take their row (a short GUI,
     * 240 px at GUI 3 and 720p, the most common: a call card and the replies left it ~30 px).
     */
    private static final int MIN_LIST_WITH_QUICK = 84;
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
    /** World sharing: its page and the card the other pages carry (HOST1). */
    private final HostPanel host;
    /** The screen the panel came from (the pause menu), or null: closing goes back to it. */
    private final Screen parent;
    private final Tween lineAlpha = new Tween(0f);
    private final List<Notice> notices = new ArrayList<>();

    private Page page;
    private Page lastListPage;
    private String conversation;
    private int seenRevision = -1;
    private int panelX;
    private int panelW;
    /** Where the chat's own rows start (under the notices and a call card): the sticker picker may cover from here. */
    private int chatTop;
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
    private String worldTitle = "";
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
        this(overlay, null, null);
    }

    /**
     * The panel opened on {@code start} (null: where it was last), from {@code parent} (the pause menu's "Arkadaşlarını
     * çağır"; null from the game): closing then goes back there, and the game stays paused as it was.
     */
    public OverlayScreen(OverlayClient overlay, Page start, Screen parent) {
        super(Texts.tr("panel.title"));
        this.overlay = overlay;
        this.state = overlay.state();
        this.parent = parent;
        gear = new IconButton(Icon.GEAR, Texts.tr("settings.title"), () -> setPage(Page.SETTINGS));
        back = new IconButton(Icon.BACK, Texts.tr("panel.back"), this::goBack);
        host = new HostPanel(this, overlay);
        retry = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("retry"), null, overlay::retry);
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
        page = overlay.lastPage() == Page.SETTINGS || overlay.lastPage() == Page.WORLD ? lastListPage
            : overlay.lastPage();
        if (page == Page.CHATS && state.welcomed() && !state.has("chat")) {
            page = Page.FRIENDS;
        }
        if (page != Page.SETTINGS && page != Page.WORLD) {
            lastListPage = page;
        }
        if (start == Page.WORLD) {
            page = Page.WORLD;
            host.sync(now());
        }
        // The tabs open on the page the panel opens on (the plate stood on "Sohbetler" over the friend list).
        tabs.select(lastListPage == Page.FRIENDS ? 1 : 0);
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

    /** Where the panel starts (GUI pixels): the game's HUD draws only left of it while the panel is open. */
    public int hudRight() {
        return closing ? 0 : panelX;
    }

    /**
     * Where the open panel starts when {@code screen} is the panel or a question over it (it shows the panel behind),
     * else 0: the game's HUD draws only left of that.
     */
    public static int hudRightOf(Screen screen) {
        OverlayScreen panel = screen instanceof OverlayScreen p ? p : screen instanceof JoinScreen j ? j.panel()
            : screen instanceof LinkScreen l ? l.panel() : screen instanceof ConfirmScreen c ? c.panel() : null;
        return panel == null ? 0 : panel.hudRight();
    }

    // ---- World sharing (HOST1) ---------------------------------------------------------------------------

    /** The world page's state changed: the content changes with the panel's page change, then lays out again. */
    void hostModeChanged() {
        pageChangedAt = now();
        layout();
        setFocused(host.initialFocus());
    }

    /** Something on the world page changed size or meaning (a pick): lay out again. */
    void hostLayout() {
        layout();
    }

    /** A question over the panel (sending a friend off, closing the world, meeting on Projects, joining a world). */
    void ask(ConfirmScreen.Question question, Runnable onConfirm) {
        Compat.setScreen(minecraft, new ConfirmScreen(this, question, onConfirm));
    }

    /** The game is in a singleplayer world (a question says it is saved before leaving). */
    boolean inSingleplayer() {
        return minecraft != null && minecraft.hasSingleplayerServer();
    }

    /**
     * "Katıl" on a friend's world invite (H15): asked once, then the launcher prepares the matching install and moves
     * the game there ({@code connect} with the world's label) or restarts it.
     */
    void joinWorld(String friendId) {
        if (state.world(friendId) == null || !state.online()) {
            return;
        }
        ask(new ConfirmScreen.Question(Texts.tr("world.join.title"), Texts.str("world.join.body"),
            inSingleplayer() ? Texts.str("join.saved") : null, Texts.tr("friend.join"), Icon.JOIN, false), () -> {
                overlay.joinWorld(friendId);
                close();
            });
    }

    /** The friend whose world invite the card shows: one the player is not in already, the first of the list. */
    String worldInviteToShow() {
        for (OverlayState.Friend f : state.friends()) {
            if (state.world(f.id()) != null && f.online() && !overlay.inWorldOf(f.id())) {
                return f.id();
            }
        }
        return null;
    }

    /** Where a guest's row is on the world page (the scenario driver points at it), or null. */
    public float[] hostRow(String friendId) {
        return host.list.visible ? host.list.rowCenter(friendId) : null;
    }

    /** The world page's buttons (the scenario driver presses them as the pointer would). */
    public void pressHost(String which) {
        ActionButton b = switch (which) {
            case "primary" -> host.primary;
            case "meet" -> host.meet;
            case "stop" -> host.stop;
            case "card-open" -> host.cardOpen;
            case "card-manage" -> host.cardManage;
            case "card-join" -> host.cardJoin;
            default -> null;
        };
        if (b != null && b.visible && b.active) {
            b.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 13, 0));
        }
    }

    /** The world page's friend list (the scenario driver ticks rows with the keys). */
    public void focusHostList() {
        PanelInput.usedKeyboard();
        setFocused(host.list);
    }

    private void goBack() {
        if (page == Page.WORLD && host.back()) {
            return;
        }
        if (parent != null && page == Page.WORLD) {
            close();
            return;
        }
        setPage(lastListPage);
    }

    /** Puts the keyboard on the friend list, as Tab would (the scenario driver picks a row with the keys). */
    public void focusFriends() {
        PanelInput.usedKeyboard();
        setFocused(friends);
    }

    /** Where a friend's tag chip is in the list (TEAM6; the scenario driver points at it), or null. */
    public float[] friendChip(String friendId) {
        return friends.visible ? friends.chipCenter(friendId) : null;
    }

    /** Where the tag chip before a sender's name is in the conversation (TEAM6), or null. */
    public float[] senderChip(String from) {
        return messages.visible ? messages.chipCenter(from) : null;
    }

    /** Whether crew chat's quick replies show now (the scenario driver checks it). */
    public boolean quickRepliesShowing() {
        return quick.visible;
    }

    /** Opens or closes the sticker picker, as its button does. */
    public void toggleStickers() {
        togglePicker();
    }

    /** Whether the conversation shows "Yeni mesajlar" (the scenario driver checks it). */
    public boolean newMessagesBelow() {
        return messages.pillShowing();
    }

    /** Whether a message is still picked with the keys (the scenario driver checks it). */
    public boolean messagePicked() {
        return messages.hasSelection();
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
        for (AbstractWidget w : new AbstractWidget[] {picker, gear, back, retry, answers[0], answers[1], answers[2],
            callJoin, tabs, chips, messages, quick, replyClose, field, stickerButton, send, friends, settingsList}) {
            addWidget(w);
        }
        for (AbstractWidget w : host.widgets()) {
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
            case WORLD -> host.initialFocus();
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

    /** World sharing changed on the game's side (opened, a guest came or went, closed): lay out again. */
    public void hostChanged() {
        sync(true);
    }

    private void sync(boolean force) {
        // World sharing moves on by itself too (the launcher's answer, an opening that never came back).
        boolean hostMoved = host.sync(now());
        if (hostMoved && page == Page.WORLD) {
            pageChangedAt = now();
        }
        if (!force && !hostMoved && state.revision() == seenRevision) {
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
        // A tag's bubble belongs to the page it opened on.
        friends.dismissBubble();
        messages.dismissBubble();
        if (page != Page.SETTINGS && page != Page.WORLD) {
            lastListPage = page;
        }
        if (next != Page.SETTINGS && next != Page.WORLD) {
            lastListPage = next;
            tabs.select(next == Page.CHATS ? 0 : 1);
        }
        if (next == Page.WORLD) {
            host.sync(now());
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
        if (state.world(id) != null) {
            // Their world invite (H13): the launcher takes the game there.
            joinWorld(id);
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
        if (state.world(f.id()) != null) {
            return !overlay.inWorldOf(f.id());
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
        worldTitle = host.title();
        dndText = Texts.str("dnd");
        tabs.setReducedMotion(reduced);
        chips.setReducedMotion(reduced);
        messages.setReducedMotion(reduced);
        friends.setReducedMotion(reduced);
        settingsList.setReducedMotion(reduced);
        host.setReducedMotion(reduced);
        // 38 % of the screen between 220 and 300 GUI px, but never past the middle (GUI 3 at 720p and GUI 6 at 1440p
        // are 426 px wide): the aim point stays in the world, down to 200 px (CHAT2-O2 follow-up).
        panelW = Math.max(220, Math.min(300, Math.round(width * 0.38f)));
        panelW = Math.max(200, Math.min(panelW, width / 2 - 6));
        panelW = Math.min(panelW, width - 24);
        panelX = width - panelW;
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        hideAll();
        boolean settingsPage = page == Page.SETTINGS;
        boolean worldPage = page == Page.WORLD;
        gear.visible = !settingsPage && !worldPage;
        back.visible = settingsPage || worldPage;
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
        call = settingsPage || worldPage ? null : state.activeCall(System.currentTimeMillis());
        callY = -1;
        if (call != null && state.has("calls")) {
            y = layoutCall(x0, cw, y);
        }
        if (worldPage) {
            host.layoutPage(x0, y, cw, height - P);
            host.seen();
            return;
        }
        if (!settingsPage) {
            // World sharing's card: the open world, "Arkadaşlarını çağır" in a singleplayer world, a friend's invite.
            y = host.layoutCard(x0, y, cw);
            host.seen();
        }
        chatTop = y;
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
            // Crew chat's quick replies (CHAT2-O2 follow-up: they always took a row): not while a reply is written
            // (one would go as the reply; the launcher hides them too), not under the sticker picker, and only while
            // the conversation keeps room for a few messages.
            int qy = above - 6 - QuickReplies.HEIGHT;
            if (isCrew(conversation) && !replying() && !picker.isOpen() && qy - 6 - y >= MIN_LIST_WITH_QUICK) {
                quick.visible = true;
                PanelWidget.place(quick, x0, qy, cw, QuickReplies.HEIGHT);
                quick.setHidden(!field.getValue().isEmpty(), now);
                listBottom = qy - 6;
            }
            if (picker.isOpen()) {
                // Over the conversation, right above the field. When it needs the room it rises over the conversation
                // chips and the tabs (they step aside while it is open), never over a card above them (a call, a
                // notice) and never over its field: what does not fit scrolls (CHAT2-O2 follow-up).
                int ph = StickerPicker.height(cw, !overlay.settings().recentStickers().isEmpty());
                int py = Math.max(chatTop, above - 4 - ph);
                PanelWidget.place(picker, x0, py, cw, Math.max(24, above - 4 - py));
                // The conversation ends above it (its newest message and edge fade stay in view): no sliver of a
                // bubble shows over the picker's edge.
                listBottom = Math.min(listBottom, py - 6);
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
        for (AbstractWidget w : new AbstractWidget[] {retry, answers[0], answers[1], answers[2], callJoin, tabs,
            chips, messages, quick, field, send, friends, settingsList, stickerButton, replyClose}) {
            w.visible = false;
        }
        host.hideAll();
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
        }
        // No first-run tip (CHAT2-O2 follow-up): the gear says where the settings are, and the card took ~60 GUI
        // pixels of a 240 px tall panel from the conversation.
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
        if ((friends.visible && friends.closeBubble()) || (messages.visible && messages.closeBubble())) {
            // A tag's bubble first: it is the topmost thing.
            return true;
        }
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
        if (friends.bubbleContains(event.x(), event.y()) || messages.bubbleContains(event.x(), event.y())) {
            // A tag's bubble beside the panel is the panel's: a press there does not go back to the game.
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
        if (parent != null) {
            // Back where it was opened from (the pause menu), which draws itself.
            Compat.setScreen(minecraft, parent);
            return;
        }
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
        // From the pause menu the game stays paused as it was; from the game the panel never pauses it.
        return parent != null && parent.isPauseScreen();
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
        if (PanelInput.pointerSet()) {
            // The scenario driver points at something (never the player's own pointer, never the OS's).
            mouseX = PanelInput.pointerX();
            mouseY = PanelInput.pointerY();
        }
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
        if (page != Page.WORLD && page != Page.SETTINGS) {
            host.drawCard(g, mouseX, mouseY);
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
        // Under an open sticker picker the tabs and the conversation chips step aside as it rises, so its edge never
        // cuts through them.
        float underPicker = picker.visible ? picker.opacity(now) : 0f;
        Draw.stratum(g);
        renderUnder(g, tabs, underPicker, mouseX, mouseY);
        renderUnder(g, chips, underPicker, mouseX, mouseY);
        Draw.stratum(g);
        messages.setFloatingShown(lineText == null ? 0f : lineAlpha.value(now));
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
        if (page == Page.WORLD) {
            Draw.stratum(g);
            host.drawPage(g, mouseX, mouseY);
        }
        // A crew tag's bubble over everything in the panel (TEAM6), inside it.
        Draw.stratum(g);
        float left = panelX + P;
        float right = panelX + panelW - P;
        friends.drawBubble(g, now, panelX, left, HEADER_H + 4, right, height - 4);
        messages.drawBubble(g, now, panelX, left, HEADER_H + 4, right, height - 4);
        g.pose().popMatrix();
        Draw.popAlpha();
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    /** A row the open sticker picker covers fades out with the picker's rise (only one it really covers). */
    private void renderUnder(GuiGraphicsExtractor g, AbstractWidget w, float underPicker, int mouseX, int mouseY) {
        if (!w.visible) {
            return;
        }
        boolean covered = underPicker > 0f && w.getY() + w.getHeight() > picker.getY();
        if (covered && underPicker >= 0.99f) {
            return;
        }
        Draw.pushAlpha(covered ? 1f - underPicker : 1f);
        render(g, w, mouseX, mouseY);
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
        if (page == Page.SETTINGS || page == Page.WORLD) {
            render(g, back, mouseX, mouseY);
            String title = page == Page.WORLD ? worldTitle : settingsTitle;
            Draw.text(g, Fonts.display(), title, x0 + 15, Fonts.centeredY(Fonts.Face.DISPLAY, mid), Theme.BONE_50);
        } else {
            Draw.text(g, Fonts.display(), titleText, x0, Fonts.centeredY(Fonts.Face.DISPLAY, mid), Theme.BONE_50);
            render(g, gear, mouseX, mouseY);
        }
        // No "● Bağlı" here (CHAT2-O2 follow-up: a status dot is the banned pattern, and connected is the normal
        // state): when the launcher is away the notice under the header says so, with "Yeniden dene".
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
        // As the launcher's strip: the gold arrow beside both lines, who is answered over what was said (no
        // colored bar at the edge).
        Draw.frame(g, Frames.CARD, x0, replyY, cw, REPLY_H);
        Draw.icon(g, Icon.REPLY, 8, x0 + 7, replyY + (REPLY_H - 8) / 2f, Theme.GOLD_300);
        Draw.text(g, Fonts.small(), replyTitle, x0 + 20, Fonts.centeredY(Fonts.Face.SMALL, replyY + 6.5f),
            Theme.BONE_100);
        Draw.text(g, Fonts.small(), replyQuote, x0 + 20, Fonts.centeredY(Fonts.Face.SMALL, replyY + 14.5f),
            Theme.ASH_300);
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
