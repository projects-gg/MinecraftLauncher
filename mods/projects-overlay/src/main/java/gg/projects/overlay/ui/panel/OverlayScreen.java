package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.chat.GameFont;
import gg.projects.overlay.emoji.EmojiCatalog;
import gg.projects.overlay.host.HostController;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.people.FriendMenu;
import gg.projects.overlay.people.WorldInvites;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.ClockWords;
import gg.projects.overlay.ui.EmojiGlyphs;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import gg.projects.overlay.ui.draw.Shapes;
import gg.projects.overlay.voice.VoiceState;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.function.IntPredicate;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * The overlay panel: a drawer on the right over the running game (it never pauses, singleplayer keeps
 * ticking, the world stays visible under a light dim). Header with the connection and settings; notices
 * when there is something to say (the launcher is away, the first time, "Rahatsız etmeyin"); a crew call
 * with its answers; then chats (conversation chips, the conversation, the message field) or friends, or the
 * settings page. Voice chat (VOICE-O) puts a ring for the player and the room's strip at the top of the chats and
 * friends pages, and its room page behind the strip.
 *
 * <p>Chat extras (CHAT2): the message field's button opens the picker, "Emoji" and "Çıkartmalar" (OVL-SOC: the
 * launcher's one button and two tabs; it opens on the tab last used, "Emoji" first); "Yanıtla" puts a
 * strip over the field ("Yanıtlanıyor: Ayşe", Esc or ✕ takes it away); "Ayşe yazıyor…" floats at the bottom of
 * the conversation while someone writes (and the player's own writing is told, at most every 4 s); a reaction
 * that did not go says why in the same place for a moment. Esc closes what is open over the panel first.
 *
 * <p>Widgets live as long as the panel: launcher data only updates what they show, so focus, the draft and
 * the caret stay where they are. It slides in on the drawer curve (opacity only under reduced motion);
 * closing hands the last picture to the HUD, which fades it out while the game already has the input.
 */
public final class OverlayScreen extends Screen {
    /**
     * The panel's pages; {@code WORLD} is world sharing (HOST1), reached from its card or the pause menu; {@code VOICE}
     * is the voice room (VOICE-O), reached from its strip.
     */
    public enum Page { CHATS, FRIENDS, SETTINGS, WORLD, VOICE }

    private static final int P = 10;
    private static final int HEADER_H = 30;
    private static final int COMPOSER_H = 22;
    private static final int REPLY_H = 20;
    /** CHAT3: the one-line strip over the field while the player edits a message of theirs. */
    private static final int EDIT_H = 18;
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

    /** A card over the lists: its first {@code strong} lines (one sentence, maybe wrapped) in bone, a detail in ash. */
    private record Notice(Icon icon, int iconColor, List<FormattedCharSequence> lines, int strong,
        ActionButton button, int y, int height) {
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
    /** The picker's emoji tab (OVL-SOC), the tab it opens on, and a pick by the pointer hands the keys back. */
    private final EmojiPanel emojiPicker;
    /**
     * Where the keys go once a press with the pointer is over (OVL-SOC): the game gives them to the widget pressed after
     * its handler ran, so a handler that hands them elsewhere (the picker's button to its search, a tab to the other
     * tab, an emoji back to the message) says so here.
     */
    private AbstractWidget pendingFocus;
    /** World sharing: its page and the card the other pages carry (HOST1). */
    private final HostPanel host;
    /** Voice chat: the ring and the room's strip on the list pages, and the room page (VOICE-O). */
    private final VoicePanel voice;
    /** Crew chat's "Sesli": the crew's voice rooms under the conversation chips (OVL-SOC). */
    private final CrewVoiceStrip crewVoice;
    private int crewVoiceSpeaking = -1;
    /** OVL-SOC2: crew chat's own line ("Ekibi çağır", the next plan, "Plan yap"). */
    private final CrewBar crewBar;
    /** The screen the panel came from (the pause menu), or null: closing goes back to it. */
    private final Screen parent;
    private final Tween lineAlpha = new Tween(0f);
    private final List<Notice> notices = new ArrayList<>();
    /** OVL-UX: a friend's menu, and the player's own name and tag in the header with the tag's bubble. */
    private final PersonMenu menu = new PersonMenu(new PersonMenu.Actions() {
        @Override
        public void picked(String friendId, FriendMenu.Action action) {
            onMenuPick(friendId, action);
        }

        @Override
        public void chose(String friendId, FriendMenu.Action action, String choice) {
            onMenuChoice(friendId, action, choice);
        }

        @Override
        public void named(String friendId, String name) {
            // "Yeni kategori…" (OVL-UX3): the launcher makes the category and moves them into it.
            friends.setMenuFor(null);
            clearChipMenu();
            overlay.moveToNewCategory(friendId, name);
        }
    });
    /** Whom the open menu is about, as the place that opened it named them (OVL-UX2: a crewmate is no friend). */
    private String menuName = "";
    /** The person menu's id for the status switcher (no person's id starts so). */
    private static final String STATUS_MENU = "@status";
    /** The launcher's list shows its search from this many friends ({@code SEARCH_FROM}). */
    private static final int SEARCH_FROM = 8;
    private static final int FILTER_H = 18;
    private OverlayState.Badge menuBadge;
    /** OVL-UX3: the player's rights in the crew chat the open menu came from, while it carries crew rows. */
    private OverlayState.CrewRights menuCrew;
    /** The chip whose menu is open (OVL-UX2), or null. */
    private String menuChip;
    /** VOICE-O: voice chat's rows of the open menu as it was planned (it closes once they would differ), or null. */
    private FriendMenu.Voice menuVoice;
    private final BadgeBubble identityBubble = new BadgeBubble();
    /** OVL-SOC 2: the status switcher on the player's dot and name in the header. */
    private final StatusButton statusButton = new StatusButton(this::openStatusMenu);
    /** OVL-SOC 2: "Arkadaşlarında ara" over the friend list (from eight friends, as the launcher's). */
    private final ComposerField friendFilter = new ComposerField(Texts.tr("friends.search"));
    private int filterY = -1;
    private int identityRevision = -1;
    private int identityBadges = -1;
    private int identityPanelX = -1;
    private int identityPanelW = -1;
    private FormattedCharSequence identityName;
    private TagChip identityChip;
    private float identityX;
    /** When a row's press opened a conversation: the second press of a double click stays on the friend list. */
    private long dmOpenedAt;

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
    /** CHAT-CALM: the words the field's hint shows now (it follows the slow mode's wait). */
    private String hintShown = "";
    private long openedAt;
    private long pageChangedAt;
    private boolean closing;
    private String titleText = "";
    private String settingsTitle = "";
    private String worldTitle = "";
    private String voiceTitle = "";
    private String dndText = "";
    /** The message the player answers (CHAT2), and the strip over the field that says so. */
    private OverlayState.Reply replyTo;
    private int replyY = -1;
    private FormattedCharSequence replyTitle;
    private FormattedCharSequence replyQuote;
    /**
     * CHAT3: the player's own message being edited in the field (the strip over it says so; it and a reply exclude each
     * other), its conversation, its words as the field compares them, and the draft and reply from before the edit
     * (they come back when it ends).
     */
    private OverlayState.ChatLine editing;
    private String editConversation;
    private String editingWords = "";
    private String draftBeforeEdit = "";
    private OverlayState.Reply replyBeforeEdit;
    private FormattedCharSequence editTitle;
    /** CHAT3: "Başka tepki": the emoji picker reacts to this message instead of writing into the field. */
    private OverlayState.ChatLine reactTo;
    /** "Ayşe yazıyor…" or a note: what the line at the bottom of the conversation says now. */
    private FormattedCharSequence lineText;
    private float lineWidth;
    /** The line's words' colour: flare for a chat note or a person note that did not go, bone for one that did. */
    private int lineInk = Theme.ASH_300;
    /** A person menu's note shown in the line (OVL-UX2: an action taken from the chat page says how it went there). */
    private OverlayState.PersonNote linePerson;
    /** The line is coming (it rises 3 px as it fades in, like the launcher's pill); going, it only fades. */
    private boolean lineComing;
    private int lineTypingRevision = -1;
    private long lineChangesAt;
    private OverlayState.Note lineNote;
    private String lineConversation;
    private String lastDraft = "";
    /**
     * The overlay's language revision this panel's words were laid out in (OVL-LANG): when the launcher names another
     * language, the panel builds itself anew in it where it stood.
     */
    private final int languageRevision;
    /** A reply carried over from the panel this one stands in for (its strip is laid out once the panel is). */
    private OverlayState.Reply carriedReply;

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
        voice = new VoicePanel(this, overlay);
        crewBar = new CrewBar(new CrewBar.Actions() {
            @Override
            public void call(String crew) {
                overlay.callCrew(crew, crewNameOf(crew), conversation);
            }

            @Override
            public void plans(String crew, boolean make) {
                closeMenu();
                Compat.setScreen(minecraft, new PlanScreen(OverlayScreen.this, overlay, crew, crewNameOf(crew),
                    make));
            }
        });
        crewVoice = new CrewVoiceStrip(new CrewVoiceStrip.Actions() {
            @Override
            public void join(String crew, String room) {
                overlay.voiceJoinCrew(crew, room);
            }

            @Override
            public void open() {
                showPage(Page.VOICE);
            }

            @Override
            public void newRoom(String crew) {
                overlay.voiceNewRoom(crew);
            }

            @Override
            public void full() {
                state.personNoteText(Texts.str("voice.refused.room_full"), true, System.currentTimeMillis());
            }
        });
        retry = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("retry"), null, overlay::retry);
        for (int i = 0; i < ANSWERS.length; i++) {
            String answer = ANSWERS[i];
            answers[i] = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("call." + answer), null,
                () -> answer(answer));
        }
        callJoin = new ActionButton(ActionButton.Style.GHOST, Texts.tr("call.join"), Icon.JOIN, this::joinCall);
        tabs = new Segmented(Texts.tr("panel.tabs"), new Component[] {Texts.tr("chats"), Texts.tr("friends")}, 0,
            i -> setPage(i == 0 ? Page.CHATS : Page.FRIENDS));
        chips = new ChipRow(this::openConversation, this::openChipMenu);
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
                    if (editing != null) {
                        // Its words go again as a new message: an edit open in the field ends first (CHAT3).
                        leaveEdit(false);
                        layout();
                    }
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
            public void edit(OverlayState.ChatLine line) {
                startEdit(line);
            }

            @Override
            public void delete(OverlayState.ChatLine line) {
                askDelete(line);
            }

            @Override
            public void moreReactions(OverlayState.ChatLine line) {
                pickReaction(line);
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

            @Override
            public void person(String id, String name, String badge, PersonMenu.Anchor at, boolean keys) {
                openSenderMenu(id, name, badge, at, keys);
            }
        });
        quick = new QuickReplies(this::sendText);
        field = new ComposerField(Texts.tr("chat.field"));
        field.setMaxLength(Protocol.MAX_MESSAGE_CHARS);
        send = new SendButton(this::canSend, this::sendDraft);
        friends = new FriendList(new FriendList.Actions() {
            @Override
            public void join(String friendId) {
                joinFriend(friendId);
            }

            @Override
            public void primary(String friendId, PersonMenu.Anchor dots, boolean keys) {
                primaryFriend(friendId, dots, keys);
            }

            @Override
            public void menu(String friendId, PersonMenu.Anchor anchor, boolean keys) {
                openFriendMenu(friendId, anchor, keys);
            }

            @Override
            public void answer(String requestId, String name, boolean accept) {
                overlay.answerRequest(requestId, name, accept);
            }

            @Override
            public void requestMenu(String requestId, String name, PersonMenu.Anchor anchor, boolean keys) {
                openRequestMenu(requestId, name, anchor, keys);
            }

            @Override
            public void add(boolean crew) {
                openAdd(crew);
            }

            @Override
            public void answerRow(String key, boolean accept) {
                OverlayScreen.this.answerRow(key, accept);
            }
        });
        friendFilter.setMaxLength(32);
        friendFilter.setPlaceholder(Texts.tr("friends.search"));
        friendFilter.setResponder(text -> {
            // Typing filters at once (the list lays itself out again only in a layout otherwise).
            friends.setFilter(text);
            if (friends.visible) {
                friends.refresh(state, this::canJoin);
            }
        });
        stickerButton = new IconButton(Icon.SMILEY, Texts.tr("chat.picker"), this::togglePicker);
        replyClose = new IconButton(Icon.CLOSE, Texts.tr("chat.reply.cancel"), this::clearReply);
        picker = new StickerPicker(this::sendSticker, (tab, keys) -> openPicker(tab, true, keys));
        picker.visible = false;
        emojiPicker = new EmojiPanel(new EmojiPanel.Actions() {
            @Override
            public void pick(String emoji, boolean keys) {
                insertEmoji(emoji, keys);
            }

            @Override
            public void tab(int tab, boolean keys) {
                openPicker(tab, true, keys);
            }
        });
        emojiPicker.visible = false;
        languageRevision = Texts.language().revision();
        // "Kısayol": both keys of the Projects category in the game's Controls (the overlay's and the emoji picker's).
        Runnable controls = () -> Compat.setScreen(minecraft, new KeyBindsScreen(this, minecraft.options));
        settingsList = new SettingsList(overlay::settings, overlay::updateSettings,
            List.of(overlay.key().shortcut("settings.key.toggle", controls),
                overlay.emojiKey().shortcut("settings.key.emoji", controls)),
            Texts.language().managed() ? overlay.languages() : null,
            // CHAT-EMOJI: the launcher's emoji picker switch, once the launcher has spoken of it.
            overlay.emojis().heard() ? overlay.emojiSwitch() : null,
            // VOICE-O: "Konuşanları göster", once the launcher has spoken of voice chat.
            overlay.voice().heard(),
            // OVL-SOC2: the card kinds, the launcher's notification switches and the blocked people, as it says them.
            SettingsList.People.of(overlay::state, overlay::setNotify, overlay::unblock));
        lastListPage = state.has("chat") || !state.welcomed() ? Page.CHATS : Page.FRIENDS;
        page = overlay.lastPage() == Page.SETTINGS || overlay.lastPage() == Page.WORLD
            || (overlay.lastPage() == Page.VOICE && !overlay.voice().inRoom()) ? lastListPage : overlay.lastPage();
        if (page == Page.CHATS && state.welcomed() && !state.has("chat")) {
            page = Page.FRIENDS;
        }
        if (page != Page.SETTINGS && page != Page.WORLD && page != Page.VOICE) {
            lastListPage = page;
        }
        // OVL-SOC2: a recent social card opens the panel where its answer is (the friends page, its crew chat).
        Page fromCard = start == null ? overlay.toastPage() : null;
        if (fromCard == Page.FRIENDS || fromCard == Page.CHATS && state.has("chat")) {
            page = fromCard;
            lastListPage = fromCard;
        }
        if (start == Page.WORLD) {
            page = Page.WORLD;
            host.sync(now());
        }
        // The tabs open on the page the panel opens on (the plate stood on "Sohbetler" over the friend list).
        tabs.select(lastListPage == Page.FRIENDS ? 1 : 0);
        conversation = pickConversation();
    }

    /**
     * The panel again in the overlay's new language (OVL-LANG), standing where {@code from} stood: the same page,
     * conversation, draft, reply and settings row. No entrance: only the page's words fade in, as on a page change.
     */
    private OverlayScreen(OverlayClient overlay, OverlayScreen from) {
        this(overlay, from.page == Page.WORLD ? Page.WORLD : null, from.parent);
        page = from.page;
        lastListPage = from.lastListPage;
        tabs.select(lastListPage == Page.FRIENDS ? 1 : 0);
        if (from.conversation != null) {
            conversation = from.conversation;
        }
        // An edit does not carry over (CHAT3): its words would go as a new message; the draft from before it does.
        boolean edit = from.editing != null;
        field.setValue(edit ? from.draftBeforeEdit : from.field.getValue());
        lastDraft = field.getValue();
        carriedReply = edit ? from.replyBeforeEdit : from.replyTo;
        settingsList.carry(from.settingsList);
        long now = now();
        openedAt = now - 10 * ENTER_MS;
        pageChangedAt = now;
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
        if (screen instanceof ReportScreen report) {
            // The report dialog (OVL-UX) shows the panel behind it too.
            return report.panel().hudRight();
        }

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
            // As a mouse would: most players click here, and a key press would leave the keyboard ring on.
            b.onClick(new MouseButtonEvent(b.getX() + b.getWidth() / 2.0, b.getY() + b.getHeight() / 2.0,
                new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0)), false);
        }
    }

    /** The world page's friend list (the scenario driver ticks rows with the keys). */
    public void focusHostList() {
        PanelInput.usedKeyboard();
        setFocused(host.list);
    }

    /** What the world page shows now ({@code PICK}, {@code OPENING}, {@code FAILED}, {@code OPEN}, {@code MORE}). */
    public String hostMode() {
        return host.mode().name();
    }

    /** The friends ticked on the world page (the scenario driver checks them). */
    public List<String> hostPicks() {
        return host.list.picked();
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

    /** Opens the picker on its sticker tab, or closes it (the scenario driver; the button opens the last tab). */
    public void toggleStickers() {
        if (pickerOpen()) {
            togglePicker();
        } else if (composerOpen() && stickersOn()) {
            openPicker(PickerTabs.STICKERS, false, false);
        }
    }

    /** Opens the picker on its emoji tab, or closes it (the scenario driver). */
    public void toggleEmoji() {
        if (pickerOpen()) {
            togglePicker();
        } else if (composerOpen()) {
            openPicker(PickerTabs.EMOJI, false, false);
        }
    }

    /** The emoji tab, while it is open (the scenario driver types into its search), or null. */
    public EmojiPanel emojiPicker() {
        return emojiPicker.isOpen() ? emojiPicker : null;
    }

    /** Presses a pill of crew chat's "Sesli" as a click would (the scenario driver; "+": "Yeni oda"); false without it. */
    public boolean pressCrewRoom(String room) {
        return crewVoice.visible && crewVoice.pressRoom(room);
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

    // ---- The friend menu and the player's own tag (OVL-UX) -------------------------------------------------

    /** A friend row's own press (a click, Enter): their conversation, or their menu where none opens. */
    private void primaryFriend(String friendId, PersonMenu.Anchor dots, boolean keys) {
        if (!messageFriend(friendId)) {
            openFriendMenu(friendId, dots, keys);
        }
    }

    /** Shows a friend's conversation (a new one too) with the field ready to write; false when none can open. */
    private boolean messageFriend(String friendId) {
        String conv = state.openDirect(friendId);
        if (conv == null || !state.has("chat")) {
            return false;
        }
        closeMenu();
        dmOpenedAt = now();
        setPage(Page.CHATS);
        openConversation(conv);
        setFocused(composerOpen() ? field : chips);
        return true;
    }

    /** A friend's menu, at the pointer or under the row's "…" (the launcher's person menu, FIX2-D). */
    private void openFriendMenu(String friendId, PersonMenu.Anchor anchor, boolean keys) {
        OverlayState.Friend f = state.friend(friendId);
        if (f == null || anchor == null) {
            return;
        }
        if (openPersonMenu(f.id(), f.name(), state.badge(f.badge()), anchor, keys, null, null)) {
            friends.setMenuFor(f.id(), !anchor.pointer());
        }
    }

    /**
     * A friend's chip: their menu, at the pointer or under the chip (OVL-UX2; the launcher's conversation list). "Mesaj
     * gönder" is left out on the conversation on screen, as the launcher's chat header leaves it out.
     */
    private void openChipMenu(String conv, PersonMenu.Anchor anchor, boolean keys) {
        String friendId = state.friendOfDirect(conv);
        OverlayState.Friend f = friendId == null ? null : state.friend(friendId);
        if (f == null || anchor == null) {
            return;
        }
        if (openPersonMenu(f.id(), f.name(), state.badge(f.badge()), anchor, keys, conv, null)) {
            menuChip = conv;
            chips.setMenuFor(conv);
        }
    }

    /**
     * A sender's menu in a conversation (OVL-UX2): crew chat names a sender by id (a friend, or a crewmate who is not
     * one); a direct conversation's sender is its friend.
     */
    private void openSenderMenu(String id, String name, String badge, PersonMenu.Anchor anchor, boolean keys) {
        String who = id != null ? id : state.friendOfDirect(conversation);
        if (who == null || anchor == null) {
            return;
        }
        OverlayState.Friend f = state.friend(who);
        String shown = f != null ? f.name() : name;
        // OVL-UX3: a sender in crew chat gets the crew rows the player's rights there allow.
        openPersonMenu(who, shown, state.badge(f != null ? f.badge() : badge), anchor, keys,
            id == null ? conversation : null, id != null ? state.crewRightsIn(conversation) : null);
    }

    /**
     * Someone's menu (the launcher's person menu, FIX2-D): a friend's whole menu, or a crewmate's from crew chat
     * ({@code conv}: the conversation it opened from, whose "Mesaj gönder" means nothing there; {@code crew}: the
     * player's rights in the crew chat it opened from, OVL-UX3). False when nothing could be offered.
     */
    private boolean openPersonMenu(String id, String name, OverlayState.Badge badge, PersonMenu.Anchor anchor,
        boolean keys, String conv, OverlayState.CrewRights crew) {
        OverlayState.Friend f = state.friend(id);
        FriendMenu.Crew rows3 = crewRows(id, crew);
        FriendMenu.Input input = (f != null ? menuInput(f, conv, rows3) : crewmateInput(id, rows3))
            .withVoice(voiceFor(id));
        return openMenu(id, name, badge, anchor, keys, crew, rows3, input);
    }

    /**
     * Someone's menu from the room page (VOICE-O): a friend's whole menu, a crewmate's in a crew room (everyone there is
     * one), else only voice chat's rows (a friend's friend in a friend room: the lists do not know them, and the
     * launcher takes no other action about them).
     */
    void openVoiceMenu(String id, String name, PersonMenu.Anchor anchor, boolean keys) {
        OverlayState.Friend f = state.friend(id);
        VoiceState.Room room = overlay.voice().room();
        FriendMenu.Voice rows = voiceFor(id);
        if (anchor == null || rows == null) {
            return;
        }
        FriendMenu.Input input;
        if (f != null) {
            input = menuInput(f, null, null);
        } else if (room != null && room.crewRoom()) {
            input = crewmateInput(id, null);
        } else {
            input = new FriendMenu.Input(false, false, false, state.online(), false, false, WorldInvites.Offer.NONE,
                false, false, false, false, false, false, false, FriendMenu.Team.NONE, 0, FriendMenu.AddFriend.HIDDEN,
                null);
        }
        openMenu(id, f != null ? f.name() : name, f != null ? state.badge(f.badge()) : null, anchor, keys, null, null,
            input.withVoice(rows));
    }

    /**
     * The scenario driver: someone's menu on the room page, as Enter on their row opens it (VOICE-O); false when they
     * have no row there.
     */
    public boolean voiceMenu(String id) {
        PersonMenu.Anchor at = voice.anchorFor(id);
        VoiceState.Room room = overlay.voice().room();
        VoiceState.Person p = room == null ? null : room.person(id);
        if (at == null || p == null) {
            return false;
        }
        PanelInput.usedKeyboard();
        openVoiceMenu(id, VoiceState.shown(p.name(), VoicePanel.WORDS), at, true);
        return true;
    }

    /** VOICE-O: the room of the ring the panel's card shows now, or null (that ring is seen; another still toasts). */
    public String shownRing() {
        return voice.shownRing();
    }

    /**
     * VOICE-O: voice chat about {@code id} for their menu: in the player's room (volume, "Benim için sustur", the role's
     * rows), and for a friend what voice chat offers ("Ara", "Odaya davet et"); null when it has nothing about them.
     */
    private FriendMenu.Voice voiceFor(String id) {
        VoiceState v = overlay.voice();
        VoiceState.Room room = v.room();
        VoiceState.Person p = room == null ? null : room.person(id);
        VoiceState.Offer offer = state.friend(id) != null ? v.offer(id, state.voicePresence(id))
            : VoiceState.Offer.NONE;
        if (p == null && offer == VoiceState.Offer.NONE) {
            return null;
        }
        return p == null ? new FriendMenu.Voice(offer, false, false, false, false, false, false, false)
            : new FriendMenu.Voice(offer, true, p.mutedForMe(), p.mutedByModerator(), p.moderator(), p.canMute(),
                p.canKick(), p.canMod());
    }

    private boolean openMenu(String id, String name, OverlayState.Badge badge, PersonMenu.Anchor anchor,
        boolean keys, OverlayState.CrewRights crew, FriendMenu.Crew rows3, FriendMenu.Input input) {
        List<FriendMenu.Row> rows = FriendMenu.plan(input);
        if (rows.isEmpty()) {
            return false;
        }
        menuCrew = rows3 == null ? null : crew;
        menuVoice = input.voice();
        menu.setLeadKeepsModerator(leadKeepsModerator(crew, id));
        long now = now();
        friends.dismissBubble();
        messages.dismissBubble();
        identityBubble.dismiss(now);
        friends.setMenuFor(null);
        chips.setMenuFor(null);
        menuChip = null;
        menuName = name == null ? "" : name;
        menuBadge = badge;
        menu.setReducedMotion(reducedMotion());
        menu.setPanelLeft(panelX);
        // OVL-SOC 2 (SOC-NEXT §3.6): under a nickname, the name the service knows ("Asıl adı Kaan_07").
        String real = state.realName(id);
        menu.noteNext(real == null ? null : Texts.str("people.real_name", real));
        menu.open(id, menuName, rows, action -> choicesFor(id, action), anchor, keys, width, height, now);
        return true;
    }

    /**
     * OVL-UX3: the crew rows for {@code id} in the crew whose chat the menu opened from ({@code rights}), the
     * launcher's {@code PersonCrew}; null without rights there or when no row may be about them (the leader, the
     * player, someone who left).
     */
    private static FriendMenu.Crew crewRows(String id, OverlayState.CrewRights rights) {
        OverlayState.MateRights mate = rights == null ? null : rights.mates().get(id);
        if (mate == null) {
            return null;
        }
        return new FriendMenu.Crew(rights.roles(), rights.lead(), mate.moderator(), rights.modsFull(), mate.kick());
    }

    /**
     * OVL-UX3: whether handing the lead on to {@code to} leaves the player a moderator (the launcher's
     * {@code afterHandingOn}: the crew has roles and fewer than 4 others moderate), else a member.
     */
    private static boolean leadKeepsModerator(OverlayState.CrewRights rights, String to) {
        if (rights == null || !rights.roles()) {
            return false;
        }
        long others = rights.mates().entrySet().stream()
            .filter(e -> e.getValue().moderator() && !e.getKey().equals(to))
            .count();
        return others < 4;
    }

    private FriendMenu.Input menuInput(OverlayState.Friend f, String conv, FriendMenu.Crew crew) {
        HostController host = overlay.host();
        OverlayState.HostView session = state.host();
        WorldInvites.Offer world = WorldInvites.offer(host.invitesToWorld(), session, host.requesting(),
            host.lastInvite(), f.id(), f.online());
        // "Mesaj gönder" opens their conversation: nothing to do where it is on screen already.
        boolean message = state.has("chat") && state.dm(f.id()) != null
            && !(conv != null && page == Page.CHATS && conv.equals(conversation));
        List<FriendMenu.Team> places = new ArrayList<>();
        for (OverlayState.CrewTarget target : state.inviteCrews()) {
            places.add(teamPlace(f, target));
        }
        return new FriendMenu.Input(true, canJoin(f), state.has("people"), state.online(), state.has("invite"),
            state.inviteWaiting(f.id(), System.currentTimeMillis()), world, WorldInvites.guest(session, f.id()) != null,
            message, state.discord(f.id()), state.has("people2"), f.fav(), f.alert(), !state.groups().isEmpty(),
            FriendMenu.best(places), places.size(), FriendMenu.AddFriend.HIDDEN, crew);
    }

    /**
     * A crewmate who is not a friend, from crew chat (OVL-UX2): "Katıl" while they play on Projects (the launcher's
     * crewmate presence), "Discord'dan mesaj at", a friend request, a block, a report.
     */
    private FriendMenu.Input crewmateInput(String id, FriendMenu.Crew crew) {
        FriendMenu.AddFriend add = !state.crewmatesAsk() ? FriendMenu.AddFriend.HIDDEN
            : state.askedBy(id) ? FriendMenu.AddFriend.RECEIVED
            : state.asking(id) ? FriendMenu.AddFriend.SENT : FriendMenu.AddFriend.READY;
        return new FriendMenu.Input(false, canJoinMate(id), state.has("people"), state.online(), false, false,
            WorldInvites.Offer.NONE, false, false, state.mateDiscord(id), state.has("people2"), false, false, false,
            FriendMenu.Team.NONE, 0, add, crew);
    }

    /** A crewmate on Projects can be joined from a game that is not there yet (Q15). */
    private boolean canJoinMate(String id) {
        return state.online() && state.mateOnProjects(id) && !overlay.onProjects();
    }

    /** A friend's place in a crew the player may invite to (the launcher's {@code inviteTargets}). */
    private static FriendMenu.Team teamPlace(OverlayState.Friend f, OverlayState.CrewTarget crew) {
        String place = f.crews().get(crew.id());
        if ("member".equals(place)) {
            return FriendMenu.Team.MEMBER;
        }
        if ("invited".equals(place)) {
            return FriendMenu.Team.INVITED;
        }
        return crew.full() ? FriendMenu.Team.FULL : FriendMenu.Team.INVITABLE;
    }

    /** The choices of a row that opens a list (OVL-UX2): the categories, or the crews. */
    private List<PersonMenu.Choice> choicesFor(String id, FriendMenu.Action action) {
        OverlayState.Friend f = state.friend(id);
        List<PersonMenu.Choice> out = new ArrayList<>();
        if (action == FriendMenu.Action.VOLUME) {
            // VOICE-O: the steps up to the voice page's top, theirs checked (a value set in the launcher between two
            // steps shows as its own row).
            VoiceState.Room room = overlay.voice().room();
            VoiceState.Person p = room == null ? null : room.person(id);
            if (p != null) {
                for (int step : VoiceState.volumeChoices(p.volume(), overlay.voice().volumeMax())) {
                    out.add(new PersonMenu.Choice(Integer.toString(step), Texts.percent(step), false,
                        step == p.volume()));
                }
            }
            return out;
        }
        if (f == null) {
            return out;
        }
        if (action == FriendMenu.Action.MOVE) {
            // The launcher's list: the categories in their order, then "Arkadaşlar"; theirs checked.
            for (OverlayState.Group group : state.groups()) {
                out.add(new PersonMenu.Choice(group.id(), group.name(), false, group.id().equals(f.group())));
            }
            out.add(new PersonMenu.Choice("", Texts.str("people.move.rest"), false, f.group() == null));
            if (state.has("people3")) {
                // "Yeni kategori…" last, after a hairline; soft at the service's 20 categories (OVL-UX3).
                out.add(new PersonMenu.Choice(PersonMenu.NEW_CHOICE, Texts.str("people.move.new"),
                    state.groups().size() >= 20, false));
            }
        } else if (action == FriendMenu.Action.CREW_INVITE) {
            for (OverlayState.CrewTarget crew : state.inviteCrews()) {
                FriendMenu.Team place = teamPlace(f, crew);
                String label = place == FriendMenu.Team.FULL ? Texts.str("people.crew_full", crew.name()) : crew.name();
                out.add(new PersonMenu.Choice(crew.id(), label, place != FriendMenu.Team.INVITABLE,
                    place == FriendMenu.Team.INVITED || place == FriendMenu.Team.MEMBER));
            }
        }
        return out;
    }

    /** A row of a person's menu, picked (one that asks: after its question). */
    private void onMenuPick(String friendId, FriendMenu.Action action) {
        friends.setMenuFor(null);
        clearChipMenu();
        OverlayState.Friend f = state.friend(friendId);
        switch (action) {
            case JOIN -> {
                if (f != null) {
                    joinFriend(friendId);
                } else {
                    joinMate(friendId, menuName);
                }
            }
            case INVITE, WORLD_INVITE -> inviteFriend(friendId);
            case MESSAGE -> messageFriend(friendId);
            case DISCORD -> {
                if (f != null) {
                    overlay.openDiscord(friendId);
                } else {
                    overlay.openDiscord(friendId, menuName);
                }
            }
            case ADD_FRIEND -> overlay.addCrewmate(friendId, menuName);
            case FAVORITE -> {
                if (f != null) {
                    overlay.favorite(friendId, !f.fav());
                }
            }
            case ALERT -> {
                if (f != null) {
                    overlay.alert(friendId, !f.alert());
                }
            }
            case CREW_INVITE -> {
                // One crew to invite to: the row itself invites (several open their list).
                for (OverlayState.CrewTarget crew : state.inviteCrews()) {
                    if (f != null && teamPlace(f, crew) == FriendMenu.Team.INVITABLE) {
                        overlay.crewInvite(friendId, crew.id(), crew.name());
                        break;
                    }
                }
            }
            case MOVE -> {
                // Only from its list (onMenuChoice).
            }
            // OVL-UX3: the crew rows of the crew chat the menu opened from (the questions were asked already).
            case MAKE_MODERATOR, UNMAKE_MODERATOR -> {
                if (menuCrew != null) {
                    overlay.crewModerator(menuCrew.crew(), friendId, menuName,
                        action == FriendMenu.Action.MAKE_MODERATOR);
                }
            }
            case MAKE_FOUNDER -> {
                if (menuCrew != null) {
                    overlay.crewLead(menuCrew.crew(), friendId, menuName);
                }
            }
            case REMOVE_MEMBER -> {
                if (menuCrew != null) {
                    overlay.crewRemove(menuCrew.crew(), friendId, menuName);
                }
            }
            case REMOVE_FROM_WORLD -> removeFromWorld(friendId);
            // VOICE-O: voice chat's rows (the kick's question was asked already).
            case VOLUME -> {
                // Only from its list (onMenuChoice).
            }
            case MUTE_FOR_ME -> {
                VoiceState.Person p = voicePerson(friendId);
                if (p != null) {
                    overlay.voiceMuteForMe(friendId, menuName, !p.mutedForMe());
                }
            }
            case VOICE_MUTE -> overlay.voiceModerate(friendId, menuName, "mute");
            case VOICE_UNMUTE -> overlay.voiceModerate(friendId, menuName, "unmute");
            case VOICE_KICK -> overlay.voiceModerate(friendId, menuName, "kick");
            case VOICE_MOD -> overlay.voiceModerate(friendId, menuName, "mod");
            case VOICE_UNMOD -> overlay.voiceModerate(friendId, menuName, "unmod");
            case CALL -> overlay.voiceCall(friendId, menuName);
            case CALL_INVITE -> overlay.voiceInvite(friendId, menuName);
            case UNFRIEND -> overlay.unfriend(friendId);
            case BLOCK -> {
                if (f != null) {
                    overlay.block(friendId);
                } else {
                    overlay.block(friendId, menuName);
                }
            }
            case REPORT -> openReport(friendId);
            // OVL-SOC 2: a friend request's answers and the status switcher (without SOC-NEXT's durations).
            case ACCEPT_REQUEST, DECLINE_REQUEST, CANCEL_REQUEST -> {
                String name = requestName(friendId);
                boolean accept = action == FriendMenu.Action.ACCEPT_REQUEST;
                if (name != null && isAnswerKey(friendId)) {
                    // OVL-SOC2: a crew invite, a join request, a request the player sent.
                    answerRow(friendId, accept);
                } else if (name != null) {
                    overlay.answerRequest(friendId, name, accept);
                }
            }
            case STATUS_ONLINE -> overlay.chooseStatus("online", null);
            case STATUS_DND -> overlay.chooseStatus("dnd", null);
            case STATUS_INVISIBLE -> overlay.chooseStatus("invisible", null);
        }
    }

    /** A choice of a row's list (OVL-UX2): the category to move them to, or the crew to invite them to. */
    private void onMenuChoice(String friendId, FriendMenu.Action action, String choice) {
        friends.setMenuFor(null);
        clearChipMenu();
        if (action == FriendMenu.Action.VOLUME) {
            VoiceState.Person p = voicePerson(friendId);
            int percent;
            try {
                percent = Integer.parseInt(choice);
            } catch (NumberFormatException e) {
                return;
            }
            if (p != null && percent != p.volume()) {
                overlay.voiceVolume(friendId, menuName, percent);
            }
            return;
        }
        if (action == FriendMenu.Action.STATUS_DND || action == FriendMenu.Action.STATUS_INVISIBLE) {
            // SOC-NEXT: a status for a while (the launcher keeps the timer).
            overlay.chooseStatus(action == FriendMenu.Action.STATUS_DND ? "dnd" : "invisible", choice);
            return;
        }
        if (action == FriendMenu.Action.MOVE) {
            OverlayState.Friend f = state.friend(friendId);
            if (f != null && choice.equals(f.group() == null ? "" : f.group())) {
                // The category they are in (its check): nothing to move.
                return;
            }
            String name = Texts.str("people.move.rest");
            for (OverlayState.Group group : state.groups()) {
                if (group.id().equals(choice)) {
                    name = group.name();
                }
            }
            overlay.moveFriend(friendId, choice, name);
        } else if (action == FriendMenu.Action.CREW_INVITE) {
            for (OverlayState.CrewTarget crew : state.inviteCrews()) {
                if (crew.id().equals(choice)) {
                    overlay.crewInvite(friendId, crew.id(), crew.name());
                }
            }
        }
    }

    private void clearChipMenu() {
        menuChip = null;
        chips.setMenuFor(null);
    }

    /** VOICE-O: someone in the player's room, or null. */
    private VoiceState.Person voicePerson(String id) {
        VoiceState.Room room = overlay.voice().room();
        return room == null ? null : room.person(id);
    }

    /**
     * "Dünyana çağır" in a singleplayer world: an invite to it, which opens it to friends first when it is not open yet
     * (world sharing, HOST1; the world page's card says "Dünyan açılıyor"); elsewhere "Birlikte oyna", the launcher's
     * play-together invite. A world that can't open says why, and nothing goes out.
     */
    private void inviteFriend(String friendId) {
        OverlayState.Friend f = state.friend(friendId);
        String name = f == null ? "" : f.name();
        long wall = System.currentTimeMillis();
        HostController.WorldInvite sent = overlay.host().inviteFriend(friendId);
        switch (sent) {
            case NOT_HERE -> overlay.inviteFriend(friendId);
            case INVITED -> state.personNote("invited", name, false, wall);
            case OPENING -> state.personNote("world_opening", name, false, wall);
            default -> state.personNoteText(sent.line(), true, wall);
        }
    }

    /** "Dünyadan çıkar", answered: the guest is sent off now (world sharing; never let back in this session). */
    private void removeFromWorld(String friendId) {
        OverlayState.HostGuest guest = WorldInvites.guest(state.host(), friendId);
        if (guest == null) {
            return;
        }
        overlay.host().removeGuest(guest.guest());
        OverlayState.Friend f = state.friend(friendId);
        state.personNote("removed_from_world", f == null ? guest.label() : f.name(), false, System.currentTimeMillis());
    }

    private void openReport(String friendId) {
        OverlayState.Friend f = state.friend(friendId);
        if (f != null) {
            Compat.setScreen(minecraft, new ReportScreen(this, f.id(), f.name(), state.badge(f.badge())));
        } else {
            // A crewmate from crew chat (OVL-UX2): the name and tag the conversation showed.
            overlay.reportingAbout(friendId, menuName);
            Compat.setScreen(minecraft, new ReportScreen(this, friendId, menuName, menuBadge));
        }
    }

    /** Closes a friend's menu at once (the page changed, a conversation opened). */
    private void closeMenu() {
        menu.dismiss();
        friends.setMenuFor(null);
        clearChipMenu();
    }

    /** The list (and a chip) keeps its menu's row lit only while the menu is open. */
    private void syncMenu() {
        if (!menu.isOpen()) {
            friends.setMenuFor(null);
            if (menuChip != null) {
                clearChipMenu();
            }
        }
    }

    /** The player's own name with the tag they wear, laid out when it changes (left of the gear in the header). */
    private void layoutIdentity() {
        if (identityRevision == state.revision() && identityBadges == state.badgeRevision()
            && identityPanelX == panelX && identityPanelW == panelW) {
            return;
        }
        identityRevision = state.revision();
        identityBadges = state.badgeRevision();
        identityPanelX = panelX;
        identityPanelW = panelW;
        identityName = null;
        identityChip = null;
        String me = state.me();
        if (!state.signedIn() || me.isEmpty()) {
            return;
        }
        OverlayState.Badge badge = state.badge(state.meBadge());
        // The bubble of the player's own tag leaves out "Sen de bu ekiptesin": it is theirs.
        TagChip chip = TagChip.of(badge == null ? null : new OverlayState.Badge(badge.crew(), badge.tag(), badge.color(),
            badge.frame(), badge.emblem(), badge.name(), badge.motto(), badge.since(), badge.members(), false),
            TagChip.ROW);
        Font ui = Fonts.ui();
        float right = panelX + panelW - P - IconButton.SIZE - 1;
        float titleRight = panelX + P + Fonts.display().width(titleText) + 12;
        // OVL-SOC 2: the status switcher's dot before the tag and the name (a launcher that takes `status`).
        int dot = state.people4() ? StatusButton.DOT_ROOM : 0;
        int room = Math.round(right - titleRight - TagChip.room(chip) - dot);
        if (room < 28) {
            // A very narrow panel keeps its title and the gear.
            return;
        }
        String fitted = Texts.fit(ui, me, room);
        identityName = Texts.visual(fitted);
        identityChip = chip;
        identityX = Draw.snap(right - ui.width(fitted) - TagChip.room(chip));
        statusX = dot > 0 ? identityX - dot : -1;
        statusRight = right;
    }

    /** OVL-SOC 2: where the status switcher starts (its dot), -1 without it; and where the name ends. */
    private float statusX = -1;
    private float statusRight;
    /** The status (and its end) the switcher was last worded for; null: word it again (a layout, a language). */
    private String statusShown;
    private long statusShownUntil;

    /** The status as the header says it: "Rahatsız etmeyin", with SOC-NEXT's end "…, 17:30'a kadar". */
    private String statusWords() {
        String status = Texts.str("status." + state.presence());
        long until = state.presenceUntil();
        return until > 0 ? Texts.str("status.until", status, ClockWords.until(until, System.currentTimeMillis()))
            : status;
    }

    /**
     * The status switcher (OVL-SOC 2): "Çevrimiçi", "Rahatsız etmeyin", "Görünmez" in the person menu under the
     * header's name, the one now checked; with SOC-NEXT the last two open the crew mute's durations ("1 saat", "8
     * saat", "Yarın sabaha kadar", "Ben kapatana kadar"), and a timed one says until when over the rows.
     */
    private void openStatusMenu() {
        if (!state.people4() || !state.signedIn()) {
            return;
        }
        boolean timed = state.socNext();
        String now = state.presence();
        List<FriendMenu.Row> rows = List.of(
            new FriendMenu.Row(FriendMenu.Action.STATUS_ONLINE, false, false, now.equals("online"), false, false),
            new FriendMenu.Row(FriendMenu.Action.STATUS_DND, false, false, now.equals("dnd"), timed, false),
            new FriendMenu.Row(FriendMenu.Action.STATUS_INVISIBLE, false, false, now.equals("invisible"), timed,
                false));
        // "Rahatsız etmeyin, 17:30'a kadar": the row of a status with durations has a caret, not the check.
        menu.noteNext(now.equals("online") ? null : statusWords());
        float top = statusButton.getY() + StatusButton.HEIGHT + 1;
        PersonMenu.Anchor at = new PersonMenu.Anchor(statusButton.getX(), statusButton.getY(),
            statusButton.getWidth(), top - statusButton.getY(), false);
        openRows(STATUS_MENU, Texts.str("status.title"), rows, action -> switch (action) {
            case STATUS_DND, STATUS_INVISIBLE -> timed ? presets() : List.of();
            default -> List.of();
        }, at, PanelInput.keyboardMode());
    }

    /** SOC-NEXT's durations for a status, the crew mute's (one vocabulary; "Ben kapatana kadar" for a status). */
    private static List<PersonMenu.Choice> presets() {
        return List.of(
            new PersonMenu.Choice("hour", Texts.str("status.preset.hour"), false, false),
            new PersonMenu.Choice("eightHours", Texts.str("status.preset.eight_hours"), false, false),
            new PersonMenu.Choice("morning", Texts.str("status.preset.morning"), false, false),
            new PersonMenu.Choice("forever", Texts.str("status.preset.forever"), false, false));
    }

    /**
     * OVL-SOC 2: a friend request's two answers ("Kabul et" first) at {@code anchor}. OVL-SOC2: a crew invite's and a
     * join request's too, with their own line on top; a request the player sent has "İsteği geri çek" alone.
     */
    private void openRequestMenu(String id, String name, PersonMenu.Anchor anchor, boolean keys) {
        if (requestName(id) == null) {
            return;
        }
        List<FriendMenu.Row> rows = id.startsWith("out:")
            ? List.of(new FriendMenu.Row(FriendMenu.Action.CANCEL_REQUEST, false, false))
            : List.of(new FriendMenu.Row(FriendMenu.Action.ACCEPT_REQUEST, false, false),
                new FriendMenu.Row(FriendMenu.Action.DECLINE_REQUEST, false, false));
        menu.noteNext(answerLine(id));
        if (openRows(id, name, rows, action -> List.of(), anchor, keys)) {
            friends.setMenuFor(id, !anchor.pointer());
        }
    }

    /** The quiet line over an answer row's menu: what it is about. */
    private String answerLine(String id) {
        if (id.startsWith("invite:")) {
            OverlayState.Invite invite = inviteOf(id);
            return invite == null ? "" : Texts.str("friend.crew_invite", invite.name());
        }
        if (id.startsWith("ask:")) {
            OverlayState.Ask ask = askOf(id);
            return ask == null ? "" : Texts.str("friend.crew_ask", ask.crewName());
        }
        if (id.startsWith("out:")) {
            return Texts.str("friend.request_waiting");
        }
        return Texts.str("friend.wants_to_add");
    }

    /** OVL-SOC2: the friend list's answer rows other than friend requests are keyed by what they are. */
    private static boolean isAnswerKey(String id) {
        return id.startsWith("invite:") || id.startsWith("ask:") || id.startsWith("out:");
    }

    private OverlayState.Invite inviteOf(String key) {
        String crew = key.substring("invite:".length());
        for (OverlayState.Invite i : state.invites()) {
            if (i.crew().equals(crew)) {
                return i;
            }
        }
        return null;
    }

    private OverlayState.Ask askOf(String key) {
        String rest = key.substring("ask:".length());
        int cut = rest.lastIndexOf(':');
        if (cut < 0) {
            return null;
        }
        String crew = rest.substring(0, cut);
        String id = rest.substring(cut + 1);
        for (OverlayState.Ask a : state.asks()) {
            if (a.crew().equals(crew) && a.id().equals(id)) {
                return a;
            }
        }
        return null;
    }

    /**
     * OVL-SOC2: the chip ({@code accept}) or the × of an answer row: "Katıl" / "Reddet" on a crew invite, "Kabul et" /
     * "Reddet" on a join request, "İsteği geri çek" on a request the player sent.
     */
    private void answerRow(String key, boolean accept) {
        if (key.startsWith("invite:")) {
            OverlayState.Invite invite = inviteOf(key);
            if (invite != null) {
                overlay.answerInvite(invite.crew(), invite.name(), accept);
            }
        } else if (key.startsWith("ask:")) {
            OverlayState.Ask ask = askOf(key);
            if (ask != null) {
                overlay.answerAsk(ask.crew(), ask.id(), ask.name(), accept);
            }
        } else if (key.startsWith("out:") && !accept) {
            String id = key.substring("out:".length());
            for (OverlayState.Request r : state.outgoing()) {
                if (r.id().equals(id)) {
                    overlay.cancelRequest(id, r.name());
                }
            }
        }
    }

    /**
     * The name of whoever asked to be friends with {@code id}, or null when no such request is listed. OVL-SOC2: a crew
     * invite's crew, a join request's or a sent request's person, by their row's key.
     */
    private String requestName(String id) {
        if (id.startsWith("invite:")) {
            OverlayState.Invite invite = inviteOf(id);
            return invite == null ? null : invite.name();
        }
        if (id.startsWith("ask:")) {
            OverlayState.Ask ask = askOf(id);
            return ask == null ? null : ask.name();
        }
        if (id.startsWith("out:")) {
            String person = id.substring("out:".length());
            for (OverlayState.Request r : state.outgoing()) {
                if (r.id().equals(person)) {
                    return r.name();
                }
            }
            return null;
        }
        for (OverlayState.Request r : state.requests()) {
            if (r.id().equals(id)) {
                return r.name();
            }
        }
        return null;
    }

    /** OVL-SOC2: "Arkadaş ekle" ({@code crew} false) or "Ekibe katıl": the code's dialog over the panel. */
    private void openAdd(boolean crew) {
        closeMenu();
        Compat.setScreen(minecraft, new AddScreen(this, overlay, crew));
    }

    /** OVL-SOC2: the scenario driver presses the friend list's "Arkadaş ekle" ({@code crew} false) or "Ekibe katıl". */
    public void openAddForDriver(boolean crew) {
        openAdd(crew);
    }

    /** Opens a menu of {@code rows} that is no person's plan (the status switcher, a request's answers). */
    private boolean openRows(String id, String name, List<FriendMenu.Row> rows,
        java.util.function.Function<FriendMenu.Action, List<PersonMenu.Choice>> lists, PersonMenu.Anchor anchor,
        boolean keys) {
        menuCrew = null;
        menuVoice = null;
        long now = now();
        friends.dismissBubble();
        messages.dismissBubble();
        identityBubble.dismiss(now);
        friends.setMenuFor(null);
        chips.setMenuFor(null);
        menuChip = null;
        menuName = name == null ? "" : name;
        menuBadge = null;
        menu.setReducedMotion(reducedMotion());
        menu.setPanelLeft(panelX);
        menu.open(id, menuName, rows, lists, anchor, keys, width, height, now);
        return true;
    }

    /** The header's "[GK] Ece" on the list pages; the chip opens its bubble as every chip does. */
    private void drawIdentity(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        if (page != Page.CHATS && page != Page.FRIENDS) {
            identityBubble.track(null, false, now);
            return;
        }
        layoutIdentity();
        if (identityName == null) {
            identityBubble.track(null, false, now);
            return;
        }
        float mid = HEADER_H / 2f;
        if (statusX >= 0) {
            // OVL-SOC 2: the switcher under the dot, the tag and the name (its plate under the pointer).
            statusButton.visible = true;
            PanelWidget.place(statusButton, Math.round(statusX) - 3, Math.round(mid - StatusButton.HEIGHT / 2f),
                Math.round(statusRight - statusX) + 6, StatusButton.HEIGHT);
            if (!state.presence().equals(statusShown) || state.presenceUntil() != statusShownUntil) {
                // Worded only when the status or its end changes (nothing is made per frame).
                statusShown = state.presence();
                statusShownUntil = state.presenceUntil();
                statusButton.show(statusShown, statusWords());
            }
            render(g, statusButton, mouseX, mouseY);
        }
        float x = identityX;
        BadgeBubble.Target target = null;
        if (identityChip != null) {
            identityChip.draw(g, x, mid);
            float cy = identityChip.top(mid);
            if (!menu.isOpen() && mouseX >= x && mouseX < x + identityChip.width && mouseY >= cy
                && mouseY < cy + identityChip.height) {
                target = new BadgeBubble.Target("me", identityChip.badge, x, cy, identityChip.width,
                    identityChip.height, 0, HEADER_H, false);
            }
            x += TagChip.room(identityChip);
        }
        identityBubble.track(target, identityBubble.contains(mouseX, mouseY), now);
        Draw.text(g, Fonts.ui(), identityName, x, Fonts.centeredY(Fonts.Face.UI, mid), Theme.ASH_300);
    }

    /** Whose menu is open, or null (the scenario driver checks it). */
    public String menuFriend() {
        return menu.friend();
    }

    /** Whether a friend's menu shows its question (the scenario driver checks it). */
    public boolean menuAsking() {
        return menu.asking();
    }

    /** A friend row's press, as a click would (the scenario driver uses it). */
    public void pressFriend(String friendId) {
        PanelInput.usedMouse();
        primaryFriend(friendId, friends.dotsOf(friendId), false);
    }

    /** A friend's menu under their "…" (the scenario driver uses it; keys: as Shift+F10 would). */
    public void openFriendMenu(String friendId, boolean keys) {
        if (keys) {
            PanelInput.usedKeyboard();
        } else {
            PanelInput.usedMouse();
        }
        openFriendMenu(friendId, friends.dotsOf(friendId), keys);
    }

    /** Whether the open menu has the row of {@code action} (the scenario driver checks it). */
    public boolean menuHas(FriendMenu.Action action) {
        return menu.hasRow(action);
    }

    /** A friend's menu at a point, as a right click there would (the scenario driver uses it). */
    public void openFriendMenuAt(String friendId, double x, double y) {
        PanelInput.usedMouse();
        openFriendMenu(friendId, PersonMenu.Anchor.at(x, y), false);
    }

    /** Where a friend's row is on screen (the scenario driver points at it), or null. */
    public float[] friendRow(String friendId) {
        return friends.visible ? friends.rowCenter(friendId) : null;
    }

    /** Puts the keyboard on a friend's row (the scenario driver uses it). */
    public void pickFriend(String friendId) {
        PanelInput.usedKeyboard();
        setFocused(friends);
        friends.select(friendId);
    }

    /** Presses a row of the open friend's menu as a click would (the scenario driver); false when it is not there. */
    public boolean pressMenu(FriendMenu.Action action) {
        boolean pressed = menu.press(action, width, height, now());
        syncMenu();
        return pressed;
    }

    /** Answers the open menu's question as a click would (the scenario driver): the action, or "Vazgeç". */
    public void answerMenu(boolean yes) {
        menu.answer(yes, now());
        syncMenu();
    }

    /** Lights a row of the open menu as the arrow keys would (the scenario driver). */
    public boolean focusMenuRow(FriendMenu.Action action) {
        PanelInput.usedKeyboard();
        return menu.focusForDriver(action);
    }

    /** The row whose list the open menu shows (OVL-UX2), or null (the scenario driver checks it). */
    public FriendMenu.Action menuListing() {
        return menu.listing();
    }

    /** Picks a choice of the open menu's list as a click would (the scenario driver); false when it is not there. */
    public boolean chooseMenu(String choice) {
        boolean chosen = menu.choose(choice, width, height, now());
        syncMenu();
        return chosen;
    }

    /** Types into the open menu's "Yeni kategori…" field as the keys would (OVL-UX3; the scenario driver). */
    public boolean typeMenu(String name) {
        return menu.typeForDriver(name);
    }

    /** Whether the open menu shows "Yeni kategori…"'s field (OVL-UX3; the scenario driver checks it). */
    public boolean menuNaming() {
        return menu.naming();
    }

    /**
     * The open menu (or its question) lies over the panel, whole (the scenario driver checks it at every GUI scale);
     * false while none is open.
     */
    public boolean menuOverPanel() {
        if (!menu.isOpen()) {
            return false;
        }
        float[] b = menu.bounds();
        return b[0] >= panelX - 0.5f && b[0] + b[2] <= width + 0.5f && b[1] >= 0 && b[1] + b[3] <= height + 0.5f;
    }

    /** OVL-SOC 2 (the scenario driver): the status switcher's menu, as Enter on it would open it. */
    public boolean openStatusMenuForDriver() {
        if (!statusButton.visible) {
            return false;
        }
        PanelInput.usedKeyboard();
        openStatusMenu();
        return menu.isOpen();
    }

    /** OVL-SOC 2 (the scenario driver): a friend request's answers, as Enter on its row would open them. */
    public boolean openRequestMenuForDriver(String id) {
        String name = requestName(id);
        PersonMenu.Anchor at = friends.dotsOf(id);
        if (name == null || at == null) {
            return false;
        }
        PanelInput.usedKeyboard();
        openRequestMenu(id, name, at, true);
        return menu.isOpen();
    }

    /** OVL-SOC 2 (the scenario driver): types into "Arkadaşlarında ara" as the keys would; false while it is hidden. */
    public boolean filterFriends(String text) {
        if (!friendFilter.visible) {
            return false;
        }
        setFocused(friendFilter);
        friendFilter.setValue(text);
        return true;
    }

    /** OVL-SOC 2 (the scenario driver): "Göster" on the first fold of ignored lines; false without one. */
    public boolean unfoldIgnored() {
        return messages.visible && messages.unfoldFirst();
    }

    /** OVL-SOC 2 (the scenario driver checks it): the folds of ignored lines the conversation shows. */
    public int ignoredFolds() {
        return messages.visible ? messages.folds() : 0;
    }

    /** A friend's chip menu as a right click on it (or, {@code keys}, Shift+F10) would open it (the scenario driver). */
    public void openChipMenu(String conv, boolean keys) {
        PersonMenu.Anchor chip = chips.anchorOf(conv);
        if (chip == null) {
            return;
        }
        if (keys) {
            PanelInput.usedKeyboard();
            openChipMenu(conv, chip, true);
        } else {
            PanelInput.usedMouse();
            openChipMenu(conv, PersonMenu.Anchor.at(chip.x() + chip.w() / 2f, chip.y() + chip.h() / 2f), false);
        }
    }

    /**
     * A sender's menu in the open conversation as a right click on their name over message {@code messageId} would
     * open it (the scenario driver; the driver's pointer rests on the name). A name above the newest messages is
     * scrolled into view first, as a player would with the wheel; false when the conversation does not have it.
     */
    public boolean openSenderMenuAt(String messageId) {
        float[] at = messages.senderNameAt(messageId);
        if (at == null && messages.revealForDriver(messageId)) {
            at = messages.senderNameAt(messageId);
        }
        if (at == null) {
            return false;
        }
        PanelInput.usedMouse();
        PanelInput.pointAt(Math.round(at[0]), Math.round(at[1]));
        messages.rightClickForDriver(at[0], at[1]);
        return menu.isOpen();
    }

    /** Where the header's own tag chip is (the scenario driver points at it), or null. */
    public float[] identityChip() {
        return identityChip == null ? null : new float[] {identityX + identityChip.width / 2f, HEADER_H / 2f};
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
        for (AbstractWidget w : new AbstractWidget[] {emojiPicker, picker, gear, back, retry, answers[0], answers[1],
            answers[2],
            callJoin, tabs, chips, crewVoice, crewBar, messages, quick, replyClose, field, stickerButton, send,
            statusButton, friendFilter, friends, settingsList}) {
            addWidget(w);
        }
        for (AbstractWidget w : host.widgets()) {
            addWidget(w);
        }
        for (AbstractWidget w : voice.widgets()) {
            addWidget(w);
        }
        Shapes.prepare();
        if (conversation != null) {
            messages.open(conversation, isCrew(conversation), unreadOf(conversation));
            // Only the chat page shows it: opened on the friends, its messages stay unread until the player looks.
            if (page == Page.CHATS) {
                overlay.openConversation(conversation);
            }
        }
        sync(true);
        if (carriedReply != null) {
            OverlayState.Reply quote = carriedReply;
            carriedReply = null;
            if (composerOpen() && state.has("replies")) {
                setReply(quote);
            }
        }
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
            case VOICE -> voice.initialFocus();
        };
    }

    /** OVL-SOC2: a crew's name as its conversation is titled, else empty. */
    private String crewNameOf(String crew) {
        OverlayState.Conversation c = state.conversation(state.conversationOfCrew(crew));
        return c == null ? "" : c.title();
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
        if (!force && !hostMoved && state.revision() == seenRevision && !voice.stale()) {
            return;
        }
        seenRevision = state.revision();
        if (page == Page.VOICE && !voice.inRoom()) {
            // The room ended (left, closed, removed): its page goes with it.
            setPage(lastListPage);
        }
        if (conversation == null || state.conversation(conversation) == null) {
            String next = pickConversation();
            if (next != null && !next.equals(conversation)) {
                conversation = next;
                messages.open(next, isCrew(next), unreadOf(next));
                if (page == Page.CHATS) {
                    overlay.openConversation(next);
                }
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
        if (page != Page.SETTINGS && page != Page.WORLD && page != Page.VOICE) {
            lastListPage = page;
        }
        if (next != Page.SETTINGS && next != Page.WORLD && next != Page.VOICE) {
            lastListPage = next;
            tabs.select(next == Page.CHATS ? 0 : 1);
        }
        if (next == Page.WORLD) {
            host.sync(now());
        }
        boolean leftChats = page == Page.CHATS;
        page = next;
        overlay.rememberPage(next);
        pageChangedAt = now();
        closePickers();
        if (page == Page.CHATS && conversation != null && unreadOf(conversation) > 0) {
            // What came while another page was on screen is still unread: the list opens at it, under
            // "Yeni mesajlar", as when the panel opens.
            messages.open(conversation, isCrew(conversation), unreadOf(conversation));
        }
        layout();
        setFocused(initialFocus());
        if (page == Page.CHATS && conversation != null) {
            overlay.openConversation(conversation);
        } else if (leftChats) {
            overlay.conversationHidden();
        }
    }

    private void openConversation(String id) {
        if (id == null || id.equals(conversation)) {
            return;
        }
        conversation = id;
        overlay.rememberConversation(id);
        clearReply();
        closePickers();
        messages.open(id, isCrew(id), unreadOf(id));
        if (page == Page.CHATS) {
            overlay.openConversation(id);
        }
        chips.update(state, id);
        layout();
    }

    private boolean composerOpen() {
        OverlayState.Conversation c = state.conversation(conversation);
        return c != null && c.writable() && state.online() && state.has("chat");
    }

    private boolean canSend() {
        if (editing != null) {
            // CHAT3: an edit waits out no slow mode, and words left as they were have nothing to save (the launcher's).
            String text = Protocol.plain(field.getValue(), Protocol.MAX_MESSAGE_CHARS);
            return composerOpen() && !text.isEmpty() && !text.equals(editingWords);
        }
        return composerOpen() && slowLeft() == 0
            && !Protocol.plain(field.getValue(), Protocol.MAX_MESSAGE_CHARS).isEmpty();
    }

    /** CHAT-CALM: whole seconds before the player may write again in this crew's chat (its slow mode), 0 now. */
    private int slowLeft() {
        long until = conversation == null ? 0 : state.slowUntil(conversation);
        long left = until - System.currentTimeMillis();
        return left <= 0 ? 0 : (int) Math.min(3600, (left + 999) / 1000);
    }

    /**
     * CHAT-CALM: the field's hint: "Ekibine yaz (yavaş mod: 30 sn)" in a crew with a slow mode, and after a message
     * "Göndermek için 12 saniye bekle" (writing goes on; only sending waits).
     */
    private Component composerHint() {
        Component plain = Texts.tr(isCrew(conversation) ? "chat.hint.crew" : "chat.hint");
        OverlayState.Conversation open = state.conversation(conversation);
        if (open == null || open.slow() <= 0) {
            return plain;
        }
        int wait = slowLeft();
        if (wait == 1) {
            return Texts.tr("chat.slow.wait_one");
        }
        if (wait > 0) {
            return Texts.tr("chat.slow.wait", wait);
        }
        return Texts.tr("chat.slow.hint", plain, Texts.str("chat.slow.seconds", open.slow()));
    }

    private void sendDraft() {
        if (editing != null) {
            saveEdit();
            return;
        }
        String text = Protocol.plain(field.getValue(), Protocol.MAX_MESSAGE_CHARS);
        // In slow mode the text waits in the field until it may go.
        if (text.isEmpty() || !composerOpen() || slowLeft() > 0) {
            return;
        }
        field.setValue("");
        lastDraft = "";
        sendText(text);
    }

    private void sendText(String text) {
        if (conversation == null || !composerOpen() || slowLeft() > 0) {
            return;
        }
        overlay.sendChat(conversation, text, replying() ? replyTo : null);
        clearReply();
        messages.toBottom();
    }

    private void sendSticker(String sticker) {
        if (conversation == null || !composerOpen() || !state.has("stickers") || slowLeft() > 0) {
            return;
        }
        closePickers();
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
        // A reply and an edit exclude each other (CHAT3): the edit ends, its draft comes back for the reply.
        leaveEdit(false);
        setReply(OverlayState.quoteOf(line));
        setFocused(field);
    }

    private void setReply(OverlayState.Reply quote) {
        replyTo = quote;
        replyWords(quote);
        layout();
    }

    /** The reply strip's words for {@code quote} ("Yanıtlanıyor: Ayşe" over what was said). */
    private void replyWords(OverlayState.Reply quote) {
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
    }

    /**
     * Takes the strip over the field away (its ✕, another conversation): the reply, or an edit (CHAT3), whose draft from
     * before it comes back.
     */
    private void clearReply() {
        if (editing != null) {
            leaveEdit(true);
            layout();
            return;
        }
        if (replyTo != null) {
            replyTo = null;
            layout();
        }
    }

    // ---- Editing and deleting the player's own messages, any reaction (CHAT3) ---------------------------

    /**
     * "Düzenle" on the player's own message: the field takes its words and the strip over it says "Mesajı
     * düzenliyorsun"; Enter or the send button saves, ✕ or Esc leaves the edit. A reply waits meanwhile (it and the
     * draft come back when the edit ends).
     */
    private void startEdit(OverlayState.ChatLine line) {
        if (line == null || conversation == null || !composerOpen() || !state.chat3()
            || !MessageList.editable(line, System.currentTimeMillis())) {
            return;
        }
        if (editing == null) {
            draftBeforeEdit = field.getValue();
            replyBeforeEdit = replyTo;
        }
        replyTo = null;
        editing = line;
        editConversation = conversation;
        String words = line.text == null ? "" : line.text;
        editingWords = Protocol.plain(words, Protocol.MAX_MESSAGE_CHARS);
        field.setValue(words);
        lastDraft = field.getValue();
        int room = Math.max(40, panelW - 2 * P - 8 - 6 - IconButton.SIZE - 2);
        editTitle = Texts.visual(Texts.fit(Fonts.small(), Texts.str("chat.editing"), room));
        layout();
        focusAfterPress(field);
        // The strip is drawn only: the narrator says it (the field then reads its words, the send button "Kaydet").
        minecraft.getNarrator().saySystemQueued(Texts.tr("chat.editing"));
    }

    /**
     * The edit ends (saved, left, its conversation or its message gone): the draft from before it comes back, and its
     * reply with {@code restoreReply} while the same conversation is open. The caller lays out.
     */
    private void leaveEdit(boolean restoreReply) {
        if (editing == null) {
            return;
        }
        boolean same = Objects.equals(conversation, editConversation);
        editing = null;
        editConversation = null;
        editingWords = "";
        editTitle = null;
        field.setValue(draftBeforeEdit == null ? "" : draftBeforeEdit);
        lastDraft = field.getValue();
        OverlayState.Reply quote = replyBeforeEdit;
        draftBeforeEdit = "";
        replyBeforeEdit = null;
        if (restoreReply && same && quote != null && quote.sid() > 0 && state.has("replies")) {
            replyTo = quote;
            replyWords(quote);
        }
    }

    /** Enter or the send button during an edit: the new words go (the message comes back "düzenlendi"). */
    private void saveEdit() {
        OverlayState.ChatLine line = editing;
        String conv = editConversation;
        String text = Protocol.plain(field.getValue(), Protocol.MAX_MESSAGE_CHARS);
        if (line == null || text.isEmpty() || !composerOpen()) {
            // Empty words do not go: the edit stays (✕ or Esc leaves it).
            return;
        }
        boolean changed = !text.equals(editingWords);
        leaveEdit(true);
        layout();
        if (changed && conv != null) {
            // The service decides (15 minutes): a refusal comes back as the chat's note.
            overlay.editMessage(conv, line.sid, text);
        }
    }

    /** "Sil" on the player's own message: asked first, the keys on "Vazgeç"; then it goes for everyone. */
    private void askDelete(OverlayState.ChatLine line) {
        String conv = conversation;
        if (line == null || conv == null || !state.online() || !state.chat3()
            || !MessageList.deletable(line, System.currentTimeMillis())) {
            return;
        }
        long sid = line.sid;
        ask(new ConfirmScreen.Question(Texts.tr("chat.delete.ask"), null, null, Texts.tr("chat.delete"), null, true),
            () -> {
                if (editing != null && editing.sid == sid) {
                    // The message being edited goes, and its edit with it.
                    leaveEdit(true);
                    layout();
                }
                overlay.deleteMessage(conv, sid);
            });
    }

    /** "Başka tepki": the emoji picker opens to react to this message (a pick reacts and closes it; Esc closes it). */
    private void pickReaction(OverlayState.ChatLine line) {
        if (line == null || line.sid <= 0 || line.deleted || !composerOpen() || !state.has("reactions")) {
            return;
        }
        reactTo = line;
        openPicker(PickerTabs.EMOJI, false, false);
        if (!emojiPicker.isOpen()) {
            reactTo = null;
        }
    }

    /** An emoji picked for {@link #reactTo}: the same reaction the quick ones give (again: taken back). */
    private void reactWith(String emoji) {
        OverlayState.ChatLine line = reactTo;
        // The message's own conversation (the panel may have moved on since the picker opened).
        String conv = line == null ? null : state.conversationOf(line);
        closePickers();
        layout();
        if (line == null || conv == null || emoji == null || emoji.isEmpty() || line.sid <= 0
            || !state.has("reactions")) {
            return;
        }
        String shown = Protocol.plain(emoji, 8);
        if (shown.isEmpty()) {
            return;
        }
        OverlayState.Reaction given = line.reaction(shown);
        overlay.react(conv, line, given != null ? given.emoji() : emoji, shown, given == null || !given.mine());
        overlay.updateSettings(s -> s.withChatEmojiUsed(emoji));
    }

    private void togglePicker() {
        if (pickerOpen()) {
            closePickers();
            setFocused(field);
        } else if (composerOpen()) {
            openPicker(stickersOn() ? overlay.pickerTab() : PickerTabs.EMOJI, false, false);
        }
    }

    /** Stickers in the picker: while the service takes them, never for an edit or a reaction (CHAT3). */
    private boolean stickersOn() {
        return state.has("stickers") && !StickerSet.get().isEmpty() && editing == null && reactTo == null;
    }

    private boolean pickerOpen() {
        return picker.isOpen() || emojiPicker.isOpen();
    }

    private void closePickers() {
        boolean hadKeys = getFocused() == picker || getFocused() == emojiPicker;
        // A reaction picker closed without a pick gives nothing (CHAT3).
        reactTo = null;
        picker.close();
        emojiPicker.close();
        if (hadKeys) {
            // The keys never stay with a closed picker (Enter there would still send or write).
            setFocused(field.visible ? field : null);
        }
    }

    /** Hands the keys to {@code w} now, and again once a press with the pointer that led here is over. */
    private void focusAfterPress(AbstractWidget w) {
        pendingFocus = w;
        setFocused(w);
    }

    /** The open tab of the picker (its place on screen), or null. */
    private PanelWidget openPicker() {
        return picker.isOpen() ? picker : emojiPicker.isOpen() ? emojiPicker : null;
    }

    /** The emoji the panel's font draws now: the game's own at GUI scale 1, which lacks some (OVL-SOC). */
    private static IntPredicate drawableEmoji() {
        return Fonts.bundled() ? cp -> true : GameFont::draws;
    }

    /**
     * Opens the picker on {@code tab} (OVL-SOC): stickers only while the service takes them, else emoji. {@code swap}:
     * in place of the other tab, so it fades without rising; {@code keysOnTabs}: the arrows on the tabs asked, so the
     * keys stay there. The emoji data is read on the emoji tab's first open only.
     */
    private void openPicker(int tab, boolean swap, boolean keysOnTabs) {
        if (!composerOpen()) {
            return;
        }
        long now = now();
        messages.dismiss();
        String lang = Texts.turkish() ? "tr" : "en";
        boolean stickers = stickersOn();
        boolean both = stickers && EmojiCatalog.available(lang);
        if (stickers && (tab == PickerTabs.STICKERS || !EmojiCatalog.available(lang))) {
            emojiPicker.close();
            picker.open(overlay.settings().recentStickers(), both, swap, keysOnTabs, now);
            overlay.rememberPickerTab(PickerTabs.STICKERS);
            layout();
            focusAfterPress(picker);
            return;
        }
        EmojiCatalog emoji = EmojiCatalog.load(lang, EmojiGlyphs::colour);
        if (emoji.entries().isEmpty()) {
            return;
        }
        picker.close();
        emojiPicker.open(emoji, drawableEmoji(), overlay.settings().recentChatEmojis(), both, swap, keysOnTabs, now);
        // CHAT3: "Başka tepki" opens it for a reaction; neither that nor an edit is the player's choice of tab.
        emojiPicker.setReacting(reactTo != null);
        if (reactTo == null && editing == null) {
            overlay.rememberPickerTab(PickerTabs.EMOJI);
        }
        layout();
        // As the launcher's picker: its search has the keys (letters find, the arrows pick, Enter writes).
        focusAfterPress(emojiPicker);
    }

    /**
     * An emoji picked in the picker (OVL-SOC): written where the caret is, the picker stays open (emoji come in runs).
     * A pointer's pick hands the keys back to the message; Enter's keeps them in the picker.
     */
    private void insertEmoji(String emoji, boolean keys) {
        if (reactTo != null) {
            // CHAT3: opened by "Başka tepki": the pick is a reaction, nothing goes into the field.
            reactWith(emoji);
            return;
        }
        if (!composerOpen() || emoji == null || emoji.isEmpty()) {
            return;
        }
        String before = field.getValue();
        field.insertText(emoji);
        if (!field.getValue().equals(before)) {
            // Only a written one counts as used (a full field writes nothing).
            overlay.updateSettings(s -> s.withChatEmojiUsed(emoji));
        }
        draftChanged();
        if (!keys) {
            focusAfterPress(field);
        }
    }

    /** The field changed: the player writes, so "yazıyor…" may go (only with text in it). */
    private void draftChanged() {
        String value = field.getValue();
        if (value.equals(lastDraft)) {
            return;
        }
        lastDraft = value;
        // An edit (CHAT3) is no new message on its way: no "yazıyor…".
        if (editing == null && !value.isBlank() && conversation != null && composerOpen() && state.has("typing")) {
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

    /**
     * "Katıl" on a crewmate who is not a friend (Q15): only Projects (the launcher never names another server of a
     * crewmate). The game connects after its own question; the launcher has nothing to answer for a crewmate, so no
     * {@code join} goes.
     */
    private void joinMate(String id, String name) {
        if (!canJoinMate(id)) {
            return;
        }
        Compat.setScreen(minecraft,
            new JoinScreen(this, new JoinScreen.Target(null, name, OverlayClient.PROJECTS_HOST, true)));
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
        voiceTitle = voice.title();
        dndText = Texts.str("dnd");
        statusShown = null;
        tabs.setReducedMotion(reduced);
        chips.setReducedMotion(reduced);
        messages.setReducedMotion(reduced);
        friends.setReducedMotion(reduced);
        settingsList.setReducedMotion(reduced);
        host.setReducedMotion(reduced);
        voice.setReducedMotion(reduced);
        crewVoice.setReducedMotion(reduced);
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
        boolean voicePage = page == Page.VOICE;
        gear.visible = !settingsPage && !worldPage && !voicePage;
        back.visible = settingsPage || worldPage || voicePage;
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
        voice.seen();
        // A ring for the player on every page, and the room's strip on the chats and friends pages (VOICE-O): what is
        // live now comes first.
        y = voice.layoutCard(x0, y, cw, now, !settingsPage && !worldPage && !voicePage);
        call = settingsPage || worldPage || voicePage ? null : state.activeCall(System.currentTimeMillis());
        callY = -1;
        if (voicePage) {
            voice.layoutPage(x0, y, cw, height - P, now);
            return;
        }
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
            // OVL-SOC 2: "Arkadaşlarında ara" from eight friends (the launcher's list); fewer, nobody is filtered.
            if (state.signedIn() && state.friends().size() >= SEARCH_FROM) {
                friendFilter.visible = true;
                filterY = y;
                PanelWidget.place(friendFilter, x0 + 18, y + (FILTER_H - 12) / 2 + 1, cw - 18 - 6, 12);
                y += FILTER_H + 6;
            } else if (!friendFilter.getValue().isEmpty()) {
                friendFilter.setValue("");
            }
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
        // Crew chat's "Sesli" (OVL-SOC): the crew's voice rooms, while voice chat may be used and the launcher says them.
        VoiceState.CrewRooms crewRooms = isCrew(conversation) && state.online()
            ? overlay.voice().crewRooms(conversation) : null;
        if (crewRooms != null && !crewRooms.rooms().isEmpty()) {
            crewVoice.visible = true;
            PanelWidget.place(crewVoice, x0, y, cw, CrewVoiceStrip.HEIGHT);
            crewVoice.show(crewRooms, overlay.voice(), VoicePanel.WORDS, now);
            crewVoiceSpeaking = overlay.voice().speakingRevision();
            y += CrewVoiceStrip.HEIGHT + 6;
        }
        // OVL-SOC2: crew chat's own line: "Ekibi çağır" and the crew's next plan (or "Plan yap").
        String barCrew = isCrew(conversation) && state.online() && (state.crews2() || state.plansOn())
            ? state.crewOf(conversation) : null;
        if (barCrew != null) {
            PanelWidget.place(crewBar, x0, y, cw, CrewBar.HEIGHT);
            if (crewBar.show(state, barCrew, System.currentTimeMillis())) {
                crewBar.visible = true;
                y += CrewBar.HEIGHT + 6;
            }
        }
        composerY = height - P - COMPOSER_H;
        int listBottom = composerY - 6;
        composerNote = null;
        if (editing != null && (!composerOpen() || !state.chat3() || editing.deleted
            || !Objects.equals(conversation, editConversation))) {
            // CHAT3: an edit ends with what made it possible (the field, its conversation, the message itself).
            leaveEdit(composerOpen());
        }
        if (conversation == null) {
            listBottom = height - P;
        } else if (composerOpen()) {
            field.visible = true;
            field.setEditable(true);
            send.visible = true;
            send.setMessage(Texts.tr(editing != null ? "chat.edit.save" : "chat.send"));
            // The picker's button: emoji always (the launcher's set is in the jar), stickers when the service takes them.
            boolean stickers = true;
            int stickerRoom = stickers ? IconButton.SIZE + 2 : 0;
            PanelWidget.place(field, x0 + 8, composerY + (COMPOSER_H - 12) / 2,
                cw - 8 - SendButton.SIZE - 6 - stickerRoom, 12);
            PanelWidget.place(send, x0 + cw - SendButton.SIZE - 2, composerY + (COMPOSER_H - SendButton.SIZE) / 2,
                SendButton.SIZE, SendButton.SIZE);
            stickerButton.visible = stickers;
            if (stickers) {
                PanelWidget.place(stickerButton, send.getX() - 2 - IconButton.SIZE,
                    composerY + (COMPOSER_H - IconButton.SIZE) / 2, IconButton.SIZE, IconButton.SIZE);
                boolean open = pickerOpen();
                // Open, it is bone, never a second gold beside the send button (a chosen option is never gold).
                stickerButton.setColors(open ? Theme.BONE_50 : Theme.ASH_400, Theme.BONE_100);
                if (emojiPicker.isOpen() && emojiPicker.hasTabs() != stickersOn()) {
                    emojiPicker.setTabs(stickersOn());
                }
                stickerButton.setMessage(Texts.tr(stickersOn() ? "chat.picker" : "chat.emoji"));
                if (picker.isOpen() && !stickersOn()) {
                    // The service stopped taking stickers: the sticker tab goes.
                    picker.close();
                }
            } else {
                closePickers();
            }
            Component hint = composerHint();
            field.setPlaceholder(hint);
            hintShown = hint.getString();
            int above = composerY;
            replyY = -1;
            if (editing != null) {
                // CHAT3: "Mesajı düzenliyorsun" over the field in the reply strip's place; its ✕ leaves the edit.
                replyY = composerY - 4 - EDIT_H;
                replyClose.visible = true;
                replyClose.setMessage(Texts.tr("join.cancel"));
                PanelWidget.place(replyClose, x0 + cw - IconButton.SIZE - 2, replyY + (EDIT_H - IconButton.SIZE) / 2,
                    IconButton.SIZE, IconButton.SIZE);
                above = replyY;
                listBottom = replyY - 6;
            } else if (replying()) {
                replyY = composerY - 4 - REPLY_H;
                replyClose.visible = true;
                replyClose.setMessage(Texts.tr("chat.reply.cancel"));
                PanelWidget.place(replyClose, x0 + cw - IconButton.SIZE - 2, replyY + (REPLY_H - IconButton.SIZE) / 2,
                    IconButton.SIZE, IconButton.SIZE);
                above = replyY;
                listBottom = replyY - 6;
            }
            // Crew chat's quick replies (CHAT2-O2 follow-up: they always took a row): not while a reply or an edit is
            // written (one would go as a new message; the launcher hides them too), not under the sticker picker, and
            // only while the conversation keeps room for a few messages.
            int qy = above - 6 - QuickReplies.HEIGHT;
            if (isCrew(conversation) && !replying() && editing == null && !pickerOpen()
                && qy - 6 - y >= MIN_LIST_WITH_QUICK) {
                quick.visible = true;
                PanelWidget.place(quick, x0, qy, cw, QuickReplies.HEIGHT);
                quick.setHidden(!field.getValue().isEmpty(), now);
                listBottom = qy - 6;
            }
            PanelWidget open = openPicker();
            if (open != null) {
                // Over the conversation, right above the field. When it needs the room it rises over the conversation
                // chips and the tabs (they step aside while it is open), never over a card above them (a call, a
                // notice) and never over its field: what does not fit scrolls (CHAT2-O2 follow-up).
                boolean both = stickersOn();
                int ph = open == picker ? StickerPicker.height(cw, !overlay.settings().recentStickers().isEmpty(), both)
                    : EmojiPanel.height(cw, both);
                int py = Math.max(chatTop, above - 4 - ph);
                PanelWidget.place(open, x0, py, cw, Math.max(24, above - 4 - py));
                // The conversation ends above it (its newest message and edge fade stay in view): no sliver of a
                // bubble shows over the picker's edge.
                listBottom = Math.min(listBottom, py - 6);
            }
        } else {
            composerNote = composerReason();
            closePickers();
            replyY = -1;
        }
        messages.visible = true;
        PanelWidget.place(messages, x0, y, cw, Math.max(20, listBottom - y));
        // "Yazıyor…" and the reaction notes float over the list's bottom: the newest message keeps clear of them.
        messages.setFloatingLine(state.has("typing") || state.has("reactions") ? LINE_PLATE_H : 0);
        messages.refresh(state, now);
        picker.setReducedMotion(reduced);
        emojiPicker.setReducedMotion(reduced);
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
            chips, crewVoice, crewBar, messages, quick, field, send, friends, settingsList, stickerButton,
            replyClose,
            statusButton, friendFilter}) {
            w.visible = false;
        }
        filterY = -1;
        host.hideAll();
        voice.hideAll();
        if (page != Page.CHATS || conversation == null) {
            closePickers();
        }

        String menuFor = menu.friend();
        boolean keep = menuFor != null && (page == Page.FRIENDS ? state.friend(menuFor) != null
            : page == Page.VOICE && voice.has(menuFor)) && Objects.equals(menuVoice, voiceFor(menuFor));
        // OVL-SOC 2: the status switcher's menu on the list pages, a request's while it is still asked.
        keep |= STATUS_MENU.equals(menuFor) && (page == Page.CHATS || page == Page.FRIENDS) && state.people4();
        keep |= menuFor != null && page == Page.FRIENDS && requestName(menuFor) != null;
        if (menuFor != null && !keep) {
            // A friend's menu belongs to the friend list (and to a friend still on it); a room's to its page (and to
            // someone still in the room); neither outlives the voice chat rows it was planned with (VOICE-O: a
            // moderator's "Herkes için sustur" is no row once someone else did it).
            closeMenu();
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
            List<FormattedCharSequence> head = Texts.wrap(Fonts.ui(), line, textW, 3);
            List<FormattedCharSequence> lines = new ArrayList<>(head);
            if (detail != null) {
                lines.addAll(Texts.wrap(ui, detail, textW, 2));
            }
            y = addNotice(x0, cw, y, status == BridgeClient.Status.CONNECTING ? Icon.CLOCK : Icon.UNPLUGGED,
                status == BridgeClient.Status.CONNECTING ? Theme.ASH_300 : Theme.FLARE_300, lines, head.size(),
                retryable ? retry : null);
        } else if (!state.signedIn()) {
            // One sentence: all of it in bone when it wraps (GAME-BATCH: its second line was ash).
            List<FormattedCharSequence> lines = Texts.wrap(ui, Texts.str("detail.signed_out"), textW, 3);
            y = addNotice(x0, cw, y, Icon.INFO, Theme.ASH_300, lines, lines.size(), null);
        }
        // No first-run tip (CHAT2-O2 follow-up): the gear says where the settings are, and the card took ~60 GUI
        // pixels of a 240 px tall panel from the conversation.
        return y;
    }

    private int addNotice(int x0, int cw, int y, Icon icon, int color, List<FormattedCharSequence> lines, int strong,
        ActionButton button) {
        int h = 8 + lines.size() * 10 + 8;
        if (button != null) {
            button.visible = true;
            int bw = button.preferredWidth();
            h += 2 + ActionButton.HEIGHT;
            PanelWidget.place(button, x0 + cw - 8 - bw, y + h - 8 - ActionButton.HEIGHT, bw, ActionButton.HEIGHT);
        }
        notices.add(new Notice(icon, color, lines, strong, button, y, h));
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
        // A name typed into the menu's "Yeni kategori…" field types too (OVL-UX3), and the friend filter (OVL-SOC 2).
        boolean filtering = friendFilter.visible && friendFilter.isFocused();
        boolean typing = field.isFocused() && field.visible || menu.naming() || filtering;
        // The emoji search types too (OVL-SOC): the panel's key bound to a letter types there.
        if (overlay.key().closes(event, typing || emojiPicker.isFocused() && emojiPicker.isOpen())) {
            // Closes, unless this is the held press that opened the panel repeating (F7-OPEN).
            overlay.keyInPanel(this);
            return true;
        }
        if (menu.isOpen()) {
            // A friend's menu takes the keys while it is open (OVL-UX); Tab leaves it and moves on.
            boolean taken = menu.keyPressed(event, width, height, now());
            syncMenu();
            if (taken) {
                return true;
            }
        }
        if (event.key() == InputConstants.KEY_ESCAPE && dismissInner()) {
            // Esc closes what is open over the panel first: the picker, the quick reactions, the reply.
            return true;
        }
        if (filtering && (event.isConfirmation() || event.isDown())) {
            // Enter or ↓ from the filter goes on to the first friend it left (the launcher's field).
            PanelInput.usedKeyboard();
            setFocused(friends);
            friends.selectFirst();
            return true;
        }
        if (friendFilter.visible && !typing && event.key() == InputConstants.KEY_F && event.hasControlDown()
            && !event.hasAltDown()) {
            // Ctrl+F finds a friend (not with Alt: Windows reports AltGr as Ctrl+Alt).
            PanelInput.usedKeyboard();
            setFocused(friendFilter);
            return true;
        }
        if (typing && !filtering && event.isConfirmation()) {
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

    @Override
    public boolean keyReleased(KeyEvent event) {
        if (overlay.key().released(event)) {
            // Its next press closes the panel (F7-OPEN: the press that opened it held, repeats did nothing).
            overlay.keyReleased();
        }
        return super.keyReleased(event);
    }

    /** Closes the innermost thing open over the panel; false when there is none. */
    private boolean dismissInner() {
        if (menu.close(now())) {
            // A friend's menu (OVL-UX) is the topmost thing when it is open.
            syncMenu();
            return true;
        }
        if (friendFilter.visible && friendFilter.isFocused() && !friendFilter.getValue().isEmpty()) {
            // Esc in the friend filter empties it first (OVL-SOC 2).
            friendFilter.setValue("");
            return true;
        }
        if ((friends.visible && friends.closeBubble()) || (messages.visible && messages.closeBubble())
            || identityBubble.close(now())) {
            // A tag's bubble first: it is the topmost thing.
            return true;
        }
        if (pickerOpen()) {
            closePickers();
            layout();
            setFocused(field.visible ? field : chips);
            return true;
        }
        if (messages.dismiss()) {
            return true;
        }
        if (editing != null && page == Page.CHATS) {
            // CHAT3: Esc leaves an edit before the panel (the draft from before it comes back).
            clearReply();
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
        if (menu.isOpen()) {
            // Nothing types under an open menu; "Yeni kategori…"'s field in it does (OVL-UX3).
            menu.charTyped(event.codepoint());
            return true;
        }
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
        if (doubleClick && now() - dmOpenedAt < 500) {
            // A double click on a friend row: its first press opened the conversation, the second lands nowhere.
            return true;
        }
        if (menu.isOpen()) {
            // A press on a friend's menu does its row; beside it, it closes only the menu (a right click on another
            // friend opens theirs).
            boolean onMenu = menu.contains(event.x(), event.y());
            menu.mouseClicked(event.x(), event.y(), width, height, now());
            syncMenu();
            // A right click on someone else (a row, a chip, a sender's name) opens theirs.
            boolean onPeople = (friends.visible && friends.isMouseOver(event.x(), event.y()))
                || (chips.visible && chips.isMouseOver(event.x(), event.y()))
                || (messages.visible && messages.isMouseOver(event.x(), event.y()))
                || (voice.list.visible && voice.list.isMouseOver(event.x(), event.y()));
            if (onMenu || event.button() != InputConstants.MOUSE_BUTTON_RIGHT || !onPeople) {
                return true;
            }
        }
        if (friends.bubbleContains(event.x(), event.y()) || messages.bubbleContains(event.x(), event.y())
            || identityBubble.contains(event.x(), event.y())) {
            // A tag's bubble beside the panel is the panel's: a press there does not go back to the game.
            return true;
        }
        if (event.x() < panelX) {
            // A click on the world goes back to the game.
            close();
            return true;
        }
        PanelWidget open = openPicker();
        if (open != null && !open.isMouseOver(event.x(), event.y()) && !stickerButton.isMouseOver(event.x(), event.y())
            && !field.isMouseOver(event.x(), event.y())) {
            // A click beside the picker closes it, and only that (the message field keeps it: emoji come in runs).
            closePickers();
            layout();
            return true;
        }
        pendingFocus = null;
        boolean handled = super.mouseClicked(event, doubleClick);
        // The game handed the keys to the widget pressed; a handler that moved them on says where (OVL-SOC), and a
        // widget the press hid (a closed picker, the crew strip under the room page) never keeps them.
        AbstractWidget want = pendingFocus;
        pendingFocus = null;
        if (want != null && want.visible) {
            setFocused(want);
        } else if (getFocused() instanceof AbstractWidget w && !w.visible) {
            setFocused(initialFocus());
        }
        return handled;
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (menu.isOpen()) {
            // The rows would slide away from their menu: scrolling closes it first.
            menu.close(now());
            syncMenu();
            return true;
        }
        return super.mouseScrolled(mouseX, mouseY, scrollX, scrollY);
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
        closePickers();
        overlay.panelClosed();
    }

    @Override
    public boolean isPauseScreen() {
        // From the pause menu the game stays paused as it was; from the game the panel never pauses it.
        return parent != null && parent.isPauseScreen();
    }

    @Override
    public void tick() {
        if (!closing && languageRevision != Texts.language().revision()) {
            // OVL-LANG: the launcher named another language (the player's pick here, or its own while there is none):
            // the panel builds itself anew in it, where it stood.
            overlay.swapPanel(new OverlayScreen(overlay, this));
            return;
        }
        if (page == Page.CHATS && quick.visible) {
            quick.setHidden(!field.getValue().isEmpty(), now());
        }
        if (call != null && !call.liveAt(System.currentTimeMillis())) {
            layout();
        }
        if (voice.ringOver(System.currentTimeMillis())) {
            layout();
        }
        // OVL-SOC2: crew chat's plan says "Başladı" on time, and goes when the plan is over.
        if (crewBar.visible && crewBar.stale(System.currentTimeMillis())) {
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

    /**
     * The panel as it stands, without its entrance (the dialog over it draws this). The keys are the dialog's: no focus
     * ring in the panel meanwhile (PanelInput); the friend the dialog is about keeps a quiet plate (FriendList).
     */
    void drawStill(GuiGraphicsExtractor g, long now) {
        drawScrim(g, 1f);
        PanelInput.drawingBehind(true);
        try {
            drawPanel(g, -1, -1, now, 1f, 0f);
        } finally {
            PanelInput.drawingBehind(false);
        }
    }

    private void drawPanel(GuiGraphicsExtractor g, int mouseX, int mouseY, long now, float alpha, float dx) {
        if (alpha <= 0.004f) {
            return;
        }
        // Under an open person menu nothing else answers the pointer, as under the launcher's menus: no hover and no
        // message tools beside it (a sender's menu opens over their message). The menu itself still has the pointer.
        int menuMouseX = mouseX;
        int menuMouseY = mouseY;
        if (menu.isOpen()) {
            mouseX = -1;
            mouseY = -1;
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
        drawIdentity(g, mouseX, mouseY, now);
        for (Notice n : notices) {
            drawNotice(g, n, mouseX, mouseY);
        }
        if (dndY >= 0) {
            float x0 = panelX + P;
            Draw.icon(g, Icon.BELL, 8, x0, dndY + 2, Theme.GOLD_300);
            Draw.text(g, Fonts.small(), dndText, x0 + 12, Fonts.centeredY(Fonts.Face.SMALL, dndY + 6), Theme.ASH_300);
        }
        voice.followSpeakers(now);
        // On every page: the card draws what the layout placed (a ring anywhere, the strip on the list pages).
        voice.drawCard(g, mouseX, mouseY);
        if (callY >= 0 && call != null) {
            drawCall(g, mouseX, mouseY);
        }
        if (page != Page.WORLD && page != Page.SETTINGS && page != Page.VOICE) {
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
        float underPicker = picker.visible ? picker.opacity(now) : emojiPicker.visible ? emojiPicker.opacity(now) : 0f;
        Draw.stratum(g);
        renderUnder(g, tabs, underPicker, mouseX, mouseY);
        renderUnder(g, chips, underPicker, mouseX, mouseY);
        if (crewVoice.visible && overlay.voice().speakingRevision() != crewVoiceSpeaking) {
            // Who speaks moved: only the rings of the player's own room follow, nothing is laid out again.
            crewVoiceSpeaking = overlay.voice().speakingRevision();
            crewVoice.speaking(overlay.voice(), now);
        }
        renderUnder(g, crewVoice, underPicker, mouseX, mouseY);
        renderUnder(g, crewBar, underPicker, mouseX, mouseY);
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
        if (emojiPicker.visible) {
            Draw.stratum(g);
            render(g, emojiPicker, mouseX, mouseY);
        }
        Draw.stratum(g);
        if (filterY >= 0) {
            drawFilter(g, mouseX, mouseY);
        }
        render(g, friends, mouseX, mouseY);
        render(g, settingsList, mouseX, mouseY);
        if (page == Page.WORLD) {
            Draw.stratum(g);
            host.drawPage(g, mouseX, mouseY);
        }
        if (page == Page.VOICE) {
            Draw.stratum(g);
            voice.drawPage(g, mouseX, mouseY, now);
        }
        // A crew tag's bubble over everything in the panel (TEAM6), inside it.
        Draw.stratum(g);
        float left = panelX + P;
        float right = panelX + panelW - P;
        friends.drawBubble(g, now, panelX, left, HEADER_H + 4, right, height - 4);
        messages.drawBubble(g, now, panelX, left, HEADER_H + 4, right, height - 4);
        g.pose().popMatrix();
        Draw.popAlpha();

        // Over everything else, outside the page's own fade: the header tag's bubble and a friend's menu (OVL-UX).
        Draw.stratum(g);
        // Beside the panel its first line stays level with the header's chip (as every tag's bubble does with its own).
        identityBubble.draw(g, now, panelX, left, 4, right, height - 4);
        friends.setNote(state.personNote(System.currentTimeMillis()), now);
        menu.draw(g, menuMouseX, menuMouseY, now);
        if (!menu.isOpen()) {
            friends.setMenuFor(null);
        }
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    /** A row the open sticker picker covers fades out with the picker's rise (only one it really covers). */
    private void renderUnder(GuiGraphicsExtractor g, AbstractWidget w, float underPicker, int mouseX, int mouseY) {
        if (!w.visible) {
            return;
        }
        PanelWidget open = openPicker();
        boolean covered = underPicker > 0f && open != null && w.getY() + w.getHeight() > open.getY();
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
        if (page == Page.SETTINGS || page == Page.WORLD || page == Page.VOICE) {
            render(g, back, mouseX, mouseY);
            String title = page == Page.WORLD ? worldTitle : page == Page.VOICE ? voiceTitle : settingsTitle;
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
        int i = 0;
        for (FormattedCharSequence line : n.lines()) {
            Draw.text(g, Fonts.ui(), line, x0 + 8 + 10 + 6, Fonts.centeredY(Fonts.Face.UI, ty + 5),
                i < n.strong() ? Theme.BONE_100 : Theme.ASH_300);
            ty += 10;
            i++;
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

    /** OVL-SOC 2: "Arkadaşlarında ara": the composer's field box, the search sign before the words. */
    private void drawFilter(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        // Its ring only while the keys are there: an open menu takes them.
        boolean focused = friendFilter.isFocused() && !menu.isOpen();
        boolean lit = mouseX >= x0 && mouseX < x0 + cw && mouseY >= filterY && mouseY < filterY + FILTER_H;
        Draw.frame(g, focused ? Frames.FIELD_FOCUS : lit ? Frames.FIELD_HOVER : Frames.FIELD, x0, filterY, cw,
            FILTER_H);
        Draw.icon(g, Icon.SEARCH, 8, x0 + 6, filterY + (FILTER_H - 8) / 2f, focused ? Theme.BONE_100
            : Theme.ASH_400);
        render(g, friendFilter, mouseX, mouseY);
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
        // CHAT-CALM: the hint counts the slow mode's wait down; with words in the field the seconds stand where the
        // count would.
        Component hint = composerHint();
        if (!hint.getString().equals(hintShown)) {
            field.setPlaceholder(hint);
            hintShown = hint.getString();
        }
        int length = field.getValue().codePointCount(0, field.getValue().length());
        // An edit (CHAT3) waits out no slow mode.
        int wait = editing != null ? 0 : slowLeft();
        if (wait > 0 && length > 0 && length < 250) {
            String left = Texts.str("chat.slow.seconds", wait);
            Font small = Fonts.small();
            // As quiet as the launcher's seconds on its send button (ash, never a bright white).
            Draw.text(g, small, left, send.getX() - 4 - small.width(left), composerY - 10f, Theme.ASH_300);
        }
        if (length >= 250) {
            String count = length + "/" + Protocol.MAX_MESSAGE_CHARS;
            Font small = Fonts.small();
            Draw.text(g, small, count, send.getX() - 4 - small.width(count), composerY - 10f,
                length >= Protocol.MAX_MESSAGE_CHARS ? Theme.FLARE_300 : Theme.ASH_400);
        }
    }

    /** "Yanıtlanıyor: Ayşe" and the quoted line over the field, with ✕; "Mesajı düzenliyorsun" in an edit (CHAT3). */
    private void drawReplyStrip(GuiGraphicsExtractor g, int mouseX, int mouseY) {
        int x0 = panelX + P;
        int cw = panelW - 2 * P;
        if (replyY >= 0 && editing != null && editTitle != null) {
            // One quiet line, as the launcher's: the field under it holds the words, the send button saves them.
            Draw.frame(g, Frames.CARD, x0, replyY, cw, EDIT_H);
            Draw.text(g, Fonts.small(), editTitle, x0 + 8, Fonts.centeredY(Fonts.Face.SMALL, replyY + EDIT_H / 2f),
                Theme.BONE_100);
            render(g, replyClose, mouseX, mouseY);
            return;
        }
        if (replyY < 0 || replyTitle == null) {
            return;
        }
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
        // A person menu's note (OVL-UX2): the chat page says it in this line, as the friend list does under itself.
        OverlayState.PersonNote person = state.personNote(wall);
        boolean rebuild = note != lineNote || person != linePerson
            || !java.util.Objects.equals(conversation, lineConversation)
            || state.typingRevision() != lineTypingRevision || wall >= lineChangesAt;
        if (rebuild) {
            lineNote = note;
            linePerson = person;
            lineConversation = conversation;
            lineTypingRevision = state.typingRevision();
            lineChangesAt = Long.MAX_VALUE;
            String words = null;
            lineInk = Theme.ASH_300;
            if (note != null) {
                words = Texts.str("chat.note." + note.key());
                lineChangesAt = note.at() + OverlayState.NOTE_MS;
                lineInk = Theme.FLARE_300;
            } else if (person != null) {
                words = FriendList.noteWords(person);
                lineChangesAt = person.at() + OverlayState.NOTE_MS;
                lineInk = person.error() ? Theme.FLARE_300 : Theme.BONE_100;
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
            lineInk);
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
