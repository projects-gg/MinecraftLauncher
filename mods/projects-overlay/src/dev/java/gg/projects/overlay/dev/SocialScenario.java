package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.people.FriendMenu;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.EmojiPanel;
import gg.projects.overlay.ui.panel.OverlayScreen;
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
    private int next;
    private long stepAt = System.nanoTime();
    private int shots;
    private boolean started;
    private boolean done;
    private int worldTicks = -1;

    @Override
    public void onInitializeClient() {
        String which = System.getProperty("projects.overlay.dev.script");
        if (!"social".equals(which) && !"social2".equals(which)) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "social-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        if ("social2".equals(which)) {
            script2();
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

    private void dev(String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
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
            Files.writeString(out.resolve("social-results.json"),
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
        String file = String.format("social-%02d-%s.png", ++shots, name);
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
