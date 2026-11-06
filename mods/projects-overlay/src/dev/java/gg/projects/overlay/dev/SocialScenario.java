package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.people.FriendMenu;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.Avatars;
import gg.projects.overlay.ui.LabelFit;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.AddScreen;
import gg.projects.overlay.ui.panel.CrewBar;
import gg.projects.overlay.ui.panel.EmojiPanel;
import gg.projects.overlay.ui.panel.FriendList;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.PanelInput;
import gg.projects.overlay.ui.panel.PlanScreen;
import gg.projects.overlay.voice.VoiceState;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for OVL-SOC, enabled with {@code -Dprojects.overlay.dev.script=social} and the mock launcher's
 * {@code --voice --people --people2 --chat2 --lang tr}: the friend list's presence dots (in game, in the launcher,
 * "Rahatsız etmeyin", offline), a friend's menu led by "Sesli arama başlat" and its question, the call it starts, the
 * conversation chips' dots, crew chat's "Sesli" (the crew's rooms, a press joining "Oda 2", its speaking ring), the
 * picker's emoji tab (groups, a search, a pick into the message) and its sticker tab. Everything through the overlay's own
 * paths (frames from the mock launcher, the overlay's presses); no OS input. Results and screenshots go to
 * {@code -Dprojects.overlay.dev.out}.
 *
 * <p>OVL-SOC 2 ({@code -Dprojects.overlay.dev.script=social2}, the mock's {@code --next} besides): "İstekler" and a
 * request's answers, the friend filter (a nickname, a real name, nothing), a nickname's real name over the friend's
 * menu, the status switcher (its durations, "Rahatsız etmeyin" for an hour and its end), crew chat's folded lines of
 * someone ignored and a mention's gold ring, a full room's "1/1" and its press, and streamer mode's friend list.
 *
 * <p>OVL-AVATARS ({@code -Dprojects.overlay.dev.script=avatars}, the mock's {@code --avatars <dir>} besides, with
 * {@code -Dprojects.overlay.perf=true}): people's pictures on the friend list, the room page, crew chat's rooms, the
 * HUD's speakers, a message toast and a ring; what their first coming costs (the panel opened with letters only, then
 * with the pictures), and pictures and letters taking turns in slices on the friend list, the room page and the HUD:
 * frame times and the overlay's own drawing time for each.
 *
 * <p>FIX-209 ({@code -Dprojects.overlay.dev.script=invite}, the mock's {@code --avatars <dir>}): "Birlikte oyna" cards
 * with the friend's face, a picture and a letter.
 *
 * <p>OVL-SOC2 ({@code -Dprojects.overlay.dev.script=soc2}, the mock's {@code --voice --people --people2 --chat2 --next
 * --soc2}): every new card, the friends page's management, "Ekle", crew chat's own line, the plans and the settings'
 * new sections, one screenshot each ({@code soc2-NN-<name>.png}, results in {@code soc2-results.json}).
 *
 * <p>OVL-SHARE ({@code -Dprojects.overlay.dev.script=share}, the mock's {@code --voice --people --people2 --share}):
 * PixelAvci live in the room (the strip's sign, the room page's row), "Oyunu paylaş", the player's share starting and
 * live with two watching, "Paylaşımı durdur", "Oyunu paylaş" dim with why, a share note, and a message and an invite
 * as they come while the game is shared ({@code share-NN-<name>.png}).
 */
public final class SocialScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL-SOC scenario");
    private static final String WORLD = "social-scenario";

    private record Step(double after, String name, Consumer<Minecraft> action) {
    }

    private final List<Step> steps = new ArrayList<>();
    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    /** The screenshots' and the results' first word ({@code social}, or the script's own: {@code soc2}). */
    private String prefix = "social";
    private int next;
    private long stepAt = System.nanoTime();
    private int shots;
    private boolean started;
    private boolean done;
    private int worldTicks = -1;

    @Override
    public void onInitializeClient() {
        String which = System.getProperty("projects.overlay.dev.script");
        if (!"social".equals(which) && !"social2".equals(which) && !"avatars".equals(which) && !"fit".equals(which)
            && !"invite".equals(which) && !"soc2".equals(which) && !"share".equals(which) && !"live".equals(which)) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "social-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        if ("soc2".equals(which)) {
            prefix = "soc2";
            script6();
        } else if ("share".equals(which)) {
            prefix = "share";
            script7();
        } else if ("live".equals(which)) {
            prefix = "live";
            script8();
        } else if ("social2".equals(which)) {
            script2();
        } else if ("avatars".equals(which)) {
            script3();
        } else if ("fit".equals(which)) {
            script4();
        } else if ("invite".equals(which)) {
            script5();
        } else {
            script();
        }
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL-SOC scenario driver on; results in {}", out);
    }

    private void step(double wait, String name, Consumer<Minecraft> action) {
        steps.add(new Step(wait, name, action));
    }

    private void tick(Minecraft mc) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay == null || done) {
            return;
        }
        mc.options.pauseOnLostFocus = false;
        if (!started) {
            started = startWorld(mc, overlay);
            stepAt = System.nanoTime();
            return;
        }
        if (next >= steps.size()) {
            finish(mc);
            return;
        }
        Step s = steps.get(next);
        if ((System.nanoTime() - stepAt) / 1e9 < s.after()) {
            return;
        }
        event("step", s.name());
        try {
            s.action().accept(mc);
        } catch (RuntimeException e) {
            LOG.error("step {} failed", s.name(), e);
            results.addProperty("failed", s.name() + ": " + e);
            finish(mc);
            return;
        }
        next++;
        stepAt = System.nanoTime();
    }

    private boolean startWorld(Minecraft mc, OverlayClient overlay) {
        Screen screen = Compat.screen(mc);
        if (worldTicks < 0) {
            boolean ready = screen instanceof TitleScreen && !Compat.overlayShowing(mc);
            if (ready && overlay.bridge().status() == BridgeClient.Status.ONLINE && overlay.voice().heard()) {
                openWorld(mc);
                worldTicks = 0;
            }
            return false;
        }
        if (mc.level == null || mc.player == null || screen != null) {
            return false;
        }
        if (++worldTicks < 100) {
            return false;
        }
        results.addProperty("minecraft", Compat.VERSION);
        results.addProperty("guiScale", mc.getWindow().getGuiScale());
        mc.player.setXRot(20f);
        return true;
    }

    private static OverlayScreen need(Minecraft mc) {
        if (Compat.screen(mc) instanceof OverlayScreen p) {
            return p;
        }
        throw new IllegalStateException("the panel is not open (" + Compat.screen(mc) + ")");
    }

    private void script() {
        // The friend list: PixelAvci in a game, NetherKral in the launcher, YildizTozu on "Rahatsız etmeyin".
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.6, "friends", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.6, "shot friends", mc -> {
            results.addProperty("zeynepPresence",
                String.valueOf(OverlayClient.get().state().friend("f-zeynep").presence()));
            shot(mc, "friends-presence");
        });
        // A friend's menu: "Sesli arama başlat" first, then its question (out of the mock's room first: someone in the
        // player's room gets volume rows instead).
        step(0.1, "leave the room", mc -> OverlayClient.get().voiceLeave());
        step(0.5, "menu", mc -> need(mc).openFriendMenu("f-ayse", true));
        step(0.5, "shot menu", mc -> {
            results.addProperty("callRow", need(mc).menuHas(FriendMenu.Action.CALL));
            shot(mc, "menu-call-first");
        });
        step(0.1, "call row", mc -> need(mc).pressMenu(FriendMenu.Action.CALL));
        step(0.5, "shot question", mc -> {
            results.addProperty("asking", need(mc).menuAsking());
            shot(mc, "call-question");
        });
        step(0.1, "call", mc -> need(mc).answerMenu(true));
        step(0.8, "shot calling", mc -> {
            VoiceState.Named calling = OverlayClient.get().voice().calling();
            results.addProperty("calling", calling == null ? "none" : calling.id());
            shot(mc, "calling");
        });
        step(0.1, "hang up", mc -> OverlayClient.get().voiceLeave());
        // The conversation chips' dots, and crew chat's "Sesli".
        step(0.4, "chats", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.3, "crew chat", mc -> need(mc).showConversation("crew:gece"));
        step(0.6, "shot crew voice", mc -> shot(mc, "crew-voice"));
        step(0.1, "join Oda 2", mc -> results.addProperty("pressedRoom", need(mc).pressCrewRoom("c1111111111111111")));
        step(0.8, "shot joined", mc -> {
            results.addProperty("roomAfterJoin", OverlayClient.get().voice().inRoom("c1111111111111111"));
            shot(mc, "crew-voice-joined");
        });
        step(0.1, "speak", mc -> devIds("voice-speak", "u-4"));
        step(0.4, "shot speaking", mc -> shot(mc, "crew-voice-speaking"));
        step(0.1, "quiet", mc -> dev("voice-quiet"));
        // The picker's emoji tab: groups, a search, a pick into the message; then its sticker tab.
        step(0.5, "direct chat", mc -> need(mc).showConversation("dm:ayse"));
        step(0.3, "emoji", mc -> need(mc).toggleEmoji());
        step(0.6, "shot emoji", mc -> {
            EmojiPanel panel = need(mc).emojiPicker();
            results.addProperty("emojiCells", panel == null ? -1 : panel.count());
            shot(mc, "emoji-tab");
        });
        step(0.1, "search", mc -> {
            EmojiPanel panel = need(mc).emojiPicker();
            if (panel != null) {
                panel.search("kalp");
            }
        });
        step(0.4, "shot search", mc -> {
            EmojiPanel panel = need(mc).emojiPicker();
            results.addProperty("kalpFirst", panel == null ? "none" : String.valueOf(panel.focusedEmoji()));
            shot(mc, "emoji-search");
        });
        step(0.1, "pick", mc -> {
            EmojiPanel panel = need(mc).emojiPicker();
            results.addProperty("picked", panel != null && panel.pickFocused());
        });
        step(0.4, "shot picked", mc -> {
            results.addProperty("draft", need(mc).input().getValue());
            shot(mc, "emoji-picked");
        });
        step(0.1, "stickers", mc -> {
            need(mc).toggleEmoji();
            need(mc).toggleStickers();
        });
        step(0.5, "shot stickers", mc -> shot(mc, "sticker-tab"));
        step(0.1, "close", mc -> need(mc).close());
    }

    /** OVL-SOC 2's scenario (the mock's {@code --next}). */
    private void script2() {
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.6, "friends", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.6, "shot requests", mc -> {
            results.addProperty("requests", OverlayClient.get().state().requests().size());
            results.addProperty("nickname", OverlayClient.get().state().friend("f-ayse").name());
            results.addProperty("realName", OverlayClient.get().state().realName("f-ayse"));
            shot(mc, "friends-requests");
        });
        step(0.1, "request menu", mc -> results.addProperty("requestMenu",
            need(mc).openRequestMenuForDriver("r-blok")));
        step(0.5, "shot request menu", mc -> shot(mc, "request-menu"));
        step(0.1, "accept", mc -> need(mc).pressMenu(FriendMenu.Action.ACCEPT_REQUEST));
        step(0.8, "shot accepted", mc -> {
            results.addProperty("requestsLeft", OverlayClient.get().state().requests().size());
            results.addProperty("newFriend", OverlayClient.get().state().friend("r-blok") != null);
            shot(mc, "request-accepted");
        });
        // The filter (eight friends now): a nickname, the real name under it, nothing.
        step(0.1, "filter nickname", mc -> results.addProperty("filter", need(mc).filterFriends("kan")));
        step(0.4, "shot filter nickname", mc -> shot(mc, "filter-nickname"));
        step(0.1, "filter real name", mc -> need(mc).filterFriends("pixel"));
        step(0.4, "shot filter real name", mc -> shot(mc, "filter-real-name"));
        step(0.1, "filter nothing", mc -> need(mc).filterFriends("zzz"));
        step(0.4, "shot filter nothing", mc -> shot(mc, "filter-none"));
        step(0.1, "filter off", mc -> need(mc).filterFriends(""));
        // A nickname's real name over the friend's menu.
        step(0.3, "menu", mc -> need(mc).openFriendMenu("f-ayse", true));
        step(0.5, "shot menu", mc -> shot(mc, "menu-real-name"));
        // OVL-FIX 2.0.7: a right click near the row's left end, "Sesli arama başlat": the question stays over the
        // panel (BATCH-206 saw it half over the game).
        step(0.1, "leave the room", mc -> OverlayClient.get().voiceLeave());
        step(0.4, "menu at the pointer", mc -> {
            float[] row = need(mc).friendRow("f-ayse");
            if (row != null) {
                need(mc).openFriendMenuAt("f-ayse", row[0] - 30, row[1]);
            }
        });
        step(0.4, "call row", mc -> {
            results.addProperty("menuOverPanel", need(mc).menuOverPanel());
            need(mc).pressMenu(FriendMenu.Action.CALL);
        });
        step(0.5, "shot question", mc -> {
            results.addProperty("askOverPanel", need(mc).menuOverPanel());
            shot(mc, "call-ask-pointer");
        });
        step(0.1, "cancel", mc -> need(mc).answerMenu(false));
        // The status switcher: its durations, an hour of "Rahatsız etmeyin", its end.
        step(0.1, "status menu", mc -> results.addProperty("statusMenu", need(mc).openStatusMenuForDriver()));
        step(0.5, "shot status menu", mc -> shot(mc, "status-menu"));
        step(0.1, "dnd row", mc -> need(mc).pressMenu(FriendMenu.Action.STATUS_DND));
        step(0.5, "shot durations", mc -> {
            results.addProperty("durations", String.valueOf(need(mc).menuListing()));
            shot(mc, "status-durations");
        });
        step(0.1, "an hour", mc -> results.addProperty("hour", need(mc).chooseMenu("hour")));
        step(0.8, "shot dnd", mc -> {
            results.addProperty("presence", OverlayClient.get().state().presence());
            results.addProperty("until", OverlayClient.get().state().presenceUntil() > 0);
            shot(mc, "status-dnd");
        });
        step(0.1, "status menu again", mc -> need(mc).openStatusMenuForDriver());
        step(0.5, "shot until", mc -> shot(mc, "status-until"));
        step(0.1, "online", mc -> need(mc).pressMenu(FriendMenu.Action.STATUS_ONLINE));
        // Crew chat: the folded lines of someone ignored, a mention, a full room.
        step(0.6, "chats", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.3, "crew chat", mc -> need(mc).showConversation("crew:gece"));
        step(0.8, "shot crew", mc -> {
            results.addProperty("presenceAfter", OverlayClient.get().state().presence());
            results.addProperty("folds", need(mc).ignoredFolds());
            shot(mc, "crew-fold-mention");
        });
        step(0.1, "full room", mc -> need(mc).pressCrewRoom("c1111111111111111"));
        step(0.4, "shot full", mc -> {
            results.addProperty("inFullRoom", OverlayClient.get().voice().inRoom("c1111111111111111"));
            shot(mc, "room-full");
        });
        step(0.1, "unfold", mc -> results.addProperty("unfolded", need(mc).unfoldIgnored()));
        step(0.5, "shot unfolded", mc -> {
            results.addProperty("foldsAfter", need(mc).ignoredFolds());
            shot(mc, "fold-open");
        });
        // Streamer mode: no address in the friend list.
        step(0.1, "streamer on", mc -> dev("streamer-on"));
        step(0.6, "friends again", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.6, "shot streamer", mc -> {
            results.addProperty("streamer", OverlayClient.get().state().streamer());
            shot(mc, "streamer-friends");
        });
        step(0.1, "close", mc -> need(mc).close());
    }

    /** OVL-AVATARS' scenario (the mock's {@code --avatars <dir>}). */
    private void script3() {
        // The panel's first opening with letters only (nothing asked yet), then again with the pictures: what their
        // first coming costs (the asks, the decoding off the render thread, the uploads).
        step(0.5, "open with letters", mc -> {
            Avatars.shown = false;
            FrameClock.clear();
            FrameClock.record(1);
            OverlayClient.get().togglePanel();
        });
        step(0.3, "friends with letters", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(1.5, "shot letters", mc -> {
            FrameClock.record(-1);
            shot(mc, "friends-letters");
            need(mc).close();
        });
        step(0.8, "open with pictures", mc -> {
            Avatars.shown = true;
            FrameClock.record(0);
            OverlayClient.get().togglePanel();
        });
        step(0.3, "friends with pictures", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(1.5, "shot pictures", mc -> {
            FrameClock.record(-1);
            JsonObject first = new JsonObject();
            first.add("pictures", FrameClock.summary(0));
            first.add("letters", FrameClock.summary(1));
            results.add("firstOpen", first);
            results.addProperty("peopleKept", Avatars.kept());
            shot(mc, "friends-pictures");
        });
        compare("friends", 6);
        step(0.1, "room page", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
        step(1.0, "shot room", mc -> shot(mc, "room-pictures"));
        compare("room", 6);
        step(0.1, "chats", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.3, "crew chat", mc -> need(mc).showConversation("crew:gece"));
        step(1.0, "shot crew rooms", mc -> shot(mc, "crew-rooms-pictures"));
        step(0.1, "close", mc -> need(mc).close());
        step(0.3, "speak", mc -> devIds("voice-speak", "f-ayse", "u-gk"));
        step(1.2, "shot hud", mc -> shot(mc, "hud-speakers"));
        compare("hud", 6);
        step(0.1, "quiet", mc -> dev("voice-quiet"));
        step(0.5, "message", mc -> dev("one"));
        step(1.2, "shot toast", mc -> shot(mc, "toast-picture"));
        step(5.0, "ring", mc -> dev("voice-ring"));
        step(1.2, "shot ring", mc -> {
            results.addProperty("peopleKeptAfter", Avatars.kept());
            shot(mc, "ring-picture");
        });
    }

    /**
     * LABEL-FIT's scenario: the voice room's bar (Microphone, Deafen, Leave) laid out as before (every label at its own
     * size) and as now, at GUI 2 and 3; then every page the panel has (friends, chats, a crew chat with its rooms, the
     * room, settings) at GUI 1 to 4 (as many as the window allows), keeping each label that had to get smaller, lose its
     * icon or be cut ({@code labelFits} in the results: none cut is the aim).
     */
    private void script4() {
        step(0.5, "audit on", mc -> {
            LabelFit.recordForDriver(true);
            OverlayClient.get().togglePanel();
        });
        for (int scale : new int[] {2, 3}) {
            step(0.1, "gui " + scale, mc -> {
                mc.options.guiScale().set(scale);
                mc.resizeGui();
            });
            step(0.4, "room page", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
            // The panel lays itself out again (as on a resize) for each: the bar's shares are part of LABEL-FIT.
            step(0.3, "as before", mc -> {
                LabelFit.fittingForDriver(false);
                mc.resizeGui();
            });
            step(0.6, "shot before", mc -> shot(mc, "voice-bar-before-gui" + scale));
            step(0.1, "as now", mc -> {
                LabelFit.fittingForDriver(true);
                mc.resizeGui();
            });
            step(0.6, "shot after", mc -> shot(mc, "voice-bar-after-gui" + scale));
        }
        for (int scale = 1; scale <= 4; scale++) {
            int s = scale;
            step(0.1, "audit gui " + s, mc -> {
                mc.options.guiScale().set(s);
                mc.resizeGui();
                results.addProperty("guiScaleAsked" + s, mc.getWindow().getGuiScale());
            });
            step(0.4, "friends " + s, mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
            step(0.4, "chats " + s, mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
            step(0.4, "crew chat " + s, mc -> need(mc).showConversation("crew:gece"));
            step(0.4, "room " + s, mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
            step(0.4, "settings " + s, mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
            step(0.4, "shot settings " + s, mc -> shot(mc, "settings-gui" + s));
        }
        step(0.1, "audit done", mc -> {
            JsonObject fits = new JsonObject();
            LabelFit.notesForDriver().forEach(fits::addProperty);
            results.add("labelFits", fits);
            LabelFit.recordForDriver(false);
            mc.options.guiScale().set(2);
            mc.resizeGui();
            need(mc).close();
        });
    }

    /**
     * FIX-209's scenario ({@code -Dprojects.overlay.dev.script=invite}, the mock's {@code --avatars <dir>}): a friend's
     * "Birlikte oyna" card with their picture (PixelAvci), then a second one from a friend without a picture (KorAtesi:
     * the letter) above it.
     */
    private void script5() {
        step(0.5, "invite with a picture", mc -> devFriend("invite-in", "f-ayse"));
        step(1.8, "shot invite picture", mc -> shot(mc, "invite-picture"));
        step(0.3, "invite with a letter", mc -> devFriend("invite-in", "f-can"));
        step(1.8, "shot invites", mc -> shot(mc, "invite-letter"));
    }

    /**
     * OVL-SOC2's scenario ({@code -Dprojects.overlay.dev.script=soc2}, the mock's {@code --soc2}): the new cards three at
     * a time (only while the panel is closed; each three push the last ones out); the friends page's "Arkadaş ekle" and
     * "Ekibe katıl", "İstekler" (a friend request, a crew invite, a join request) and "Gönderilenler" at the list's end;
     * "Arkadaş ekle" with the player's code, a code that leads nowhere and a friend's code; "Ekibe katıl" with a crew's
     * code; crew chat's own line ("Ekibi çağır", "Bugün 21:00 · 2 +1"), a call and a second one too soon; the crew's
     * plans and "Plan yap"; the settings' card kinds, friend notifications and blocked people.
     */
    /** OVL-SHARE: screen sharing in the room. */
    private void script7() {
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.6, "chats", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.8, "shot strip", mc -> {
            results.addProperty("pixelLive", OverlayClient.get().voice().sharing("f-ayse"));
            results.addProperty("game", OverlayClient.get().voice().shareGame());
            shot(mc, "strip-live");
        });
        step(0.1, "room page", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
        step(0.8, "shot page", mc -> shot(mc, "page-ready"));
        step(0.1, "share the game", mc -> OverlayClient.get().voiceShareGame());
        step(0.3, "shot starting", mc -> shot(mc, "page-starting"));
        step(1.4, "shot live", mc -> {
            results.addProperty("mine", OverlayClient.get().voice().myShares().toString());
            shot(mc, "page-live");
        });
        step(0.1, "chats again", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.8, "shot strip live", mc -> shot(mc, "strip-mine"));
        // The cards while the game is in the share: nobody's, nothing said.
        step(0.1, "close", mc -> need(mc).close());
        step(0.3, "private cards", mc -> dev("share-private"));
        step(1.2, "shot private", mc -> shot(mc, "cards-private"));
        step(5.0, "open again", mc -> OverlayClient.get().togglePanel());
        step(0.6, "room page again", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
        step(0.1, "stop", mc -> OverlayClient.get().voiceShareStop(1));
        step(0.8, "shot stopped", mc -> {
            results.addProperty("afterStop", OverlayClient.get().voice().myShares().size());
            shot(mc, "page-stopped");
        });
        step(0.1, "dim", mc -> dev("share-dim"));
        step(0.8, "shot dim", mc -> shot(mc, "page-dim"));
        step(0.1, "note", mc -> dev("share-note"));
        step(0.8, "shot note", mc -> shot(mc, "page-note"));
        step(0.1, "close at last", mc -> need(mc).close());
    }

    /**
     * SHARE-LIVE ({@code -Dprojects.overlay.dev.script=live}, the mock's {@code --voice --people --people2 --share
     * --thumb <png>}): "YAYINDA" after PixelAvci's name on the room page and the card the driver's pointer opens on it
     * (the stream picture, "Oyun, 2 kişi izliyor"), then NetherKral in the friend list, live in a call of their own:
     * the card locked ({@code live-NN-<name>.png}).
     */
    private void script8() {
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.6, "room page", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
        step(0.9, "shot tags", mc -> {
            results.addProperty("pixelLive", OverlayClient.get().voice().sharing("f-ayse"));
            shot(mc, "page-tag");
        });
        step(0.1, "point at the tag", mc -> livePoint(need(mc).voiceLiveChip("f-ayse")));
        step(1.4, "shot card", mc -> {
            VoiceState.Thumb thumb = OverlayClient.get().voice().thumb("f-ayse");
            results.addProperty("thumb", thumb == null ? "none yet" : thumb.kind());
            shot(mc, "page-card");
        });
        step(0.1, "friends", mc -> {
            PanelInput.clearPointer();
            need(mc).showPage(OverlayScreen.Page.FRIENDS);
        });
        step(0.9, "shot friends", mc -> shot(mc, "friends-tag"));
        step(0.1, "point at the friend's tag", mc -> livePoint(need(mc).friendLiveChip("f-mert")));
        step(1.4, "shot friend card", mc -> shot(mc, "friends-card"));
        step(0.1, "close", mc -> {
            PanelInput.clearPointer();
            need(mc).close();
        });
    }

    private void livePoint(float[] at) {
        if (at == null) {
            results.addProperty("point", "nothing there");
            return;
        }
        PanelInput.pointAt(Math.round(at[0]), Math.round(at[1]));
    }

    private void script6() {
        step(0.5, "cards 1", mc -> devCards("request", "friend", "online"));
        step(1.6, "shot cards 1", mc -> shot(mc, "cards-request-friend-online"));
        step(0.1, "cards 2", mc -> devCards("crew-invite", "crew-ask", "crew-in"));
        step(1.6, "shot cards 2", mc -> shot(mc, "cards-crew"));
        step(0.1, "cards 3", mc -> devCards("plan", "missed", "projects"));
        step(1.6, "shot cards 3", mc -> shot(mc, "cards-plan-missed-projects"));
        // The friends page (once the cards are gone): the pills, "İstekler", then the list's end.
        step(5.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.6, "friends", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.8, "shot friends", mc -> {
            OverlayState st = OverlayClient.get().state();
            results.addProperty("requests", st.requests().size());
            results.addProperty("invites", st.invites().size());
            results.addProperty("asks", st.asks().size());
            results.addProperty("outgoing", st.outgoing().size());
            results.addProperty("blocked", st.blocked().size());
            results.addProperty("code", st.myCode());
            shot(mc, "friends-requests");
        });
        step(0.1, "to the end", mc -> {
            FriendList list = widget(mc, FriendList.class);
            list.mouseScrolled(list.getX() + list.getWidth() / 2.0, list.getY() + list.getHeight() / 2.0, 0, -100);
        });
        step(0.8, "shot sent", mc -> shot(mc, "friends-sent"));
        // "Arkadaş ekle": the player's code, a code that leads nowhere, a friend's code.
        step(0.1, "add friend", mc -> need(mc).openAddForDriver(false));
        step(0.6, "shot add", mc -> shot(mc, "add-friend"));
        step(0.1, "find nobody", mc -> dialog(mc, AddScreen.class).findForDriver("PRJ-0000-0000"));
        step(0.8, "shot not found", mc -> shot(mc, "add-not-found"));
        step(0.1, "find friend", mc -> dialog(mc, AddScreen.class).findForDriver("PRJ-4HT9-2WQC"));
        step(0.8, "shot found", mc -> {
            OverlayState.Found found = OverlayClient.get().state().found();
            results.addProperty("foundFriend", found == null ? "none" : found.kind() + " " + found.name());
            shot(mc, "add-friend-found");
        });
        step(0.1, "close add", mc -> dialog(mc, AddScreen.class).onClose());
        // "Ekibe katıl": a crew's code.
        step(0.4, "join crew", mc -> need(mc).openAddForDriver(true));
        step(0.5, "find crew", mc -> dialog(mc, AddScreen.class).findForDriver("EKP-YLD7-3QK2"));
        step(0.8, "shot crew found", mc -> {
            OverlayState.Found found = OverlayClient.get().state().found();
            results.addProperty("foundCrew", found == null ? "none" : found.kind() + " " + found.name());
            shot(mc, "add-crew-found");
        });
        step(0.1, "close join", mc -> dialog(mc, AddScreen.class).onClose());
        // Crew chat's own line: "Ekibi çağır" and the next plan; a call, then a second one too soon.
        step(0.4, "chats", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.3, "crew chat", mc -> need(mc).showConversation("crew:gece"));
        step(0.8, "shot crew bar", mc -> {
            OverlayState st = OverlayClient.get().state();
            results.addProperty("crewOfChat", String.valueOf(st.crewOf("crew:gece")));
            results.addProperty("mayCall", st.mayCall("gece"));
            results.addProperty("plans", st.plans("gece", System.currentTimeMillis()).size());
            shot(mc, "crew-bar");
        });
        step(0.1, "call", mc -> results.addProperty("called", widget(mc, CrewBar.class).pressForDriver(true)));
        step(0.8, "shot called", mc -> shot(mc, "crew-called"));
        step(0.1, "call again", mc -> widget(mc, CrewBar.class).pressForDriver(true));
        step(0.8, "shot call wait", mc -> {
            results.addProperty("callWait",
                OverlayClient.get().state().callWaitUntil(System.currentTimeMillis()) > 0);
            shot(mc, "crew-call-wait");
        });
        // The crew's plans over the panel, then "Plan yap".
        step(0.1, "plans", mc -> results.addProperty("plansPill", widget(mc, CrewBar.class).pressForDriver(false)));
        step(0.8, "shot plans", mc -> shot(mc, "plans"));
        step(0.1, "make", mc -> results.addProperty("makeForm",
            dialog(mc, PlanScreen.class).makeForDriver("Turnuva provası, sesli gelin")));
        step(0.6, "shot make", mc -> shot(mc, "plan-make"));
        step(0.1, "close plans", mc -> dialog(mc, PlanScreen.class).onClose());
        // The settings' new sections: the card kinds, the launcher's friend notifications, the blocked people.
        step(0.4, "settings", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.4, "to the cards", mc -> results.addProperty("cardsSection",
            need(mc).settings().scrollToSectionForDriver(Texts.str("settings.cards"))));
        step(0.5, "shot cards section", mc -> shot(mc, "settings-cards"));
        step(0.1, "to the notifications", mc -> results.addProperty("notifySection",
            need(mc).settings().scrollToSectionForDriver(Texts.str("settings.friend_notify"))));
        step(0.5, "shot notifications", mc -> shot(mc, "settings-friend-notify"));
        step(0.1, "to the blocked", mc -> results.addProperty("blockedSection",
            need(mc).settings().scrollToSectionForDriver(Texts.str("settings.blocked"))));
        step(0.5, "shot blocked", mc -> shot(mc, "settings-blocked"));
        step(0.1, "close", mc -> need(mc).close());
    }

    /**
     * OVL-AVATARS: pictures (slot 0) and letters (slot 1) take turns in {@code pairs} pairs of slices on what shows now;
     * each slice settles 0.4 s before it records 0.8 s. Frame times and the overlay's own drawing time for each.
     */
    private void compare(String name, int pairs) {
        long[][] perf = new long[2][7];
        step(0.1, name + " compare", mc -> FrameClock.clear());
        for (int i = 0; i < 2 * pairs; i++) {
            int mode = i % 2;
            step(0.05, name + (mode == 0 ? " pictures" : " letters"), mc -> Avatars.shown = mode == 0);
            step(0.4, name + " record", mc -> {
                Perf.drain();
                FrameClock.record(mode);
            });
            step(0.8, name + " slice end", mc -> {
                FrameClock.record(-1);
                long[] drained = Perf.drain();
                for (int k = 0; k < Math.min(drained.length, perf[mode].length); k++) {
                    perf[mode][k] += drained[k];
                }
            });
        }
        step(0.05, name + " results", mc -> {
            Avatars.shown = true;
            JsonObject o = new JsonObject();
            o.add("pictures", withPerf(FrameClock.summary(0), perf[0]));
            o.add("letters", withPerf(FrameClock.summary(1), perf[1]));
            results.add(name, o);
        });
    }

    /** A slot's frame times with the overlay's own time (the panel's, the HUD's and the toasts') per frame. */
    private static JsonObject withPerf(JsonObject frames, long[] drained) {
        frames.addProperty("toastCalls", drained[1]);
        frames.addProperty("toastAvgMicros", drained[1] == 0 ? 0 : Math.round(drained[0] / 10.0 / drained[1]) / 100.0);
        frames.addProperty("panelCalls", drained[3]);
        frames.addProperty("panelAvgMicros", drained[3] == 0 ? 0 : Math.round(drained[2] / 10.0 / drained[3]) / 100.0);
        frames.addProperty("panelBytesPerCall", drained[3] == 0 ? 0 : Math.round(drained[6] / (double) drained[3]));
        return frames;
    }

    private void dev(String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    /** A dev command about one of the mock's friends ({@code friend}: their id). */
    private void devFriend(String cmd, String friend) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        o.addProperty("friend", friend);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + " " + friend + (sent ? "" : " (not sent)"));
    }

    /** OVL-SOC2: the mock sends these new cards one after another ({@code card}). */
    private void devCards(String... kinds) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", "card");
        JsonArray list = new JsonArray();
        for (String kind : kinds) {
            list.add(kind);
        }
        o.add("kinds", list);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", "card " + String.join(" ", kinds) + (sent ? "" : " (not sent)"));
    }

    /** One of the open panel's widgets ({@code type}), as the panel holds it. */
    private static <T> T widget(Minecraft mc, Class<T> type) {
        for (GuiEventListener child : need(mc).children()) {
            if (type.isInstance(child)) {
                return type.cast(child);
            }
        }
        throw new IllegalStateException("the panel has no " + type.getSimpleName());
    }

    /** The dialog open over the panel ({@link AddScreen}, {@link PlanScreen}). */
    private static <T> T dialog(Minecraft mc, Class<T> type) {
        Screen screen = Compat.screen(mc);
        if (type.isInstance(screen)) {
            return type.cast(screen);
        }
        throw new IllegalStateException("no " + type.getSimpleName() + " (" + screen + ")");
    }

    private void devIds(String cmd, String... ids) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        JsonArray list = new JsonArray();
        for (String id : ids) {
            list.add(id);
        }
        o.add("ids", list);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        results.addProperty("runSeconds", Math.round((System.nanoTime() - runStarted) / 1e6) / 1000.0);
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve(prefix + "-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
        mc.stop();
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL-SOC", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261103L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("%s-%02d-%s.png", prefix, ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", Math.round((System.nanoTime() - runStarted) / 1e6) / 1000.0);
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }
}
