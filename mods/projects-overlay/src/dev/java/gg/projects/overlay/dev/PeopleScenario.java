package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.people.FriendMenu;
import gg.projects.overlay.people.WorldInvites;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.PanelInput;
import gg.projects.overlay.ui.panel.PausePanelButton;
import gg.projects.overlay.ui.panel.ReportScreen;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.function.Consumer;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.InputType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for OVL-UX, enabled with {@code -Dprojects.overlay.dev.script=people} and the mock launcher's
 * {@code --chat2 --team6 --people --host --no-probe}: in a flat test world, the friend list with the player's own tag in
 * the header and its bubble, a row under the driver's pointer (never the OS pointer) with its "…", a friend's menu at the
 * pointer, from the keys (Shift+F10, arrows) and under the "…", "Birlikte oyna" opening the world to a friend in one step
 * (HOST1) and soft for an offline one, a guest of the world sent off after its question, the unfriend and block questions, the report dialog empty and filled and its note, a row's press opening an existing and a new
 * conversation (its first message makes it real), the tag taken off, English, the pause menu's Projects button at GUI
 * scales 1 to 4 and in a small window (checked clear of every other button), reached with Tab and opened with Enter, and
 * the panel's frame time with a menu open. {@code -Dprojects.overlay.dev.people=pause}: the pause menu alone (runs with
 * other mods). Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class PeopleScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL-UX scenario");
    private static final String WORLD = "people-scenario";

    /** One step: wait this long after the previous one, then do it (false: the scenario failed). */
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
    private CompletableFuture<Void> reload;

    @Override
    public void onInitializeClient() {
        if (!"people".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "people-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        script();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL-UX scenario driver on; results in {}", out);
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

    private int worldTicks = -1;

    /** The title screen, then a flat world; true once the world is in and settled. */
    private boolean startWorld(Minecraft mc, OverlayClient overlay) {
        Screen screen = Compat.screen(mc);
        if (worldTicks < 0) {
            boolean ready = screen instanceof TitleScreen && !Compat.overlayShowing(mc);
            if (ready && overlay.bridge().status() == BridgeClient.Status.ONLINE) {
                results.addProperty("people", overlay.state().has("people"));
                results.addProperty("invite", overlay.state().has("invite"));
                results.addProperty("meBadge", String.valueOf(overlay.state().meBadge()));
                openWorld(mc);
                worldTicks = 0;
            }
            return false;
        }
        if (mc.level == null || mc.player == null || screen != null) {
            return false;
        }
        if (++worldTicks < 120) {
            return false;
        }
        results.addProperty("minecraft", Compat.VERSION);
        results.addProperty("guiScale", mc.getWindow().getGuiScale());
        results.addProperty("window", mc.getWindow().getWidth() + "x" + mc.getWindow().getHeight());
        mc.player.setXRot(25f);
        return true;
    }

    private static OverlayScreen panel(Minecraft mc) {
        return Compat.screen(mc) instanceof OverlayScreen p ? p : null;
    }

    private static OverlayScreen need(Minecraft mc) {
        OverlayScreen p = panel(mc);
        if (p == null) {
            throw new IllegalStateException("the panel is not open (" + Compat.screen(mc) + ")");
        }
        return p;
    }

    private void script() {
        if ("pause".equals(System.getProperty("projects.overlay.dev.people"))) {
            // Only the pause menu (its button at every GUI scale and window size, with other mods or without).
            pauseMatrix();
            return;
        }
        // The friend list: the player's own tag in the header, every friend with theirs.
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.4, "friends", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.9, "shot friends", mc -> shot(mc, "friends"));
        step(0.1, "point at the header's chip", mc -> pointAt(need(mc).identityChip()));
        step(0.8, "shot identity bubble", mc -> shot(mc, "identity-bubble"));
        step(0.1, "point at a row", mc -> pointAt(need(mc).friendRow("f-ayse")));
        step(0.5, "shot row hover", mc -> shot(mc, "row-hover"));
        // A right click on Ayşe: her menu at the pointer.
        step(0.1, "menu at the pointer", mc -> {
            float[] at = need(mc).friendRow("f-ayse");
            need(mc).openFriendMenuAt("f-ayse", at[0] + 30, at[1]);
        });
        step(0.05, "shot menu arriving", mc -> shot(mc, "menu-arriving"));
        step(0.4, "shot menu at pointer", mc -> {
            shot(mc, "menu-pointer");
            results.addProperty("menuFor", need(mc).menuFriend());
            Perf.drain();
        });
        step(3.0, "perf with the menu open", mc -> results.add("panelWithMenu", perf(Perf.drain())));
        step(0.1, "esc closes it", mc -> key(need(mc), InputConstants.KEY_ESCAPE, 0));
        step(0.4, "esc result", mc -> results.addProperty("escClosedMenu", need(mc).menuFriend() == null));
        // From the keys: Mert picked, Shift+F10, the first row lit; the arrows move.
        step(0.1, "keys", mc -> {
            PanelInput.clearPointer();
            need(mc).pickFriend("f-mert");
        });
        step(0.3, "shift f10", mc -> key(need(mc), InputConstants.KEY_F10, 1));
        step(0.4, "shot menu from keys", mc -> shot(mc, "menu-keys"));
        step(0.1, "down down", mc -> {
            key(need(mc), InputConstants.KEY_DOWN, 0);
            key(need(mc), InputConstants.KEY_DOWN, 0);
        });
        step(0.3, "shot menu arrows", mc -> shot(mc, "menu-keys-down"));
        step(0.1, "esc", mc -> key(need(mc), InputConstants.KEY_ESCAPE, 0));
        // In this singleplayer world the menu invites to it ("Dünyana çağır", HOST1): soft for Can, who is offline.
        step(0.4, "menu under dots (Can, offline)", mc -> need(mc).openFriendMenu("f-can", false));
        step(0.4, "shot menu under dots", mc -> shot(mc, "menu-dots-offline"));
        step(0.1, "esc", mc -> key(need(mc), InputConstants.KEY_ESCAPE, 0));
        // Mert: the world opens for him in one step (its note), then his row says the invite went.
        step(0.3, "Mert's menu", mc -> need(mc).openFriendMenu("f-mert", false));
        step(0.1, "invite Mert", mc -> results.addProperty("invitePressed", need(mc).pressMenu(
            FriendMenu.Action.WORLD_INVITE)));
        step(0.5, "shot opening note", mc -> {
            shot(mc, "note-world-opening");
            results.addProperty("worldOpening", OverlayClient.get().host().requesting()
                || OverlayClient.get().host().hosting());
        });
        step(2.5, "Mert's menu again", mc -> need(mc).openFriendMenu("f-mert", false));
        step(0.4, "shot invite sent", mc -> {
            shot(mc, "menu-invite-sent");
            results.addProperty("worldOpen", OverlayClient.get().host().hosting());
        });
        step(0.1, "esc", mc -> key(need(mc), InputConstants.KEY_ESCAPE, 0));
        // Ayşe in the world (as the launcher mirrors it): her menu sends her off, asked first.
        step(0.1, "Ayşe in the world", mc -> dev("host-mirror", "guests",
            "[{\"guest\":\"g1\",\"friend\":\"f-ayse\",\"label\":\"Ayşe\",\"name\":\"Ayse\",\"state\":\"in\"},"
            + "{\"guest\":\"\",\"friend\":\"f-mert\",\"label\":\"Mert\",\"state\":\"invited\"}]"));
        step(0.6, "Ayşe's menu", mc -> need(mc).openFriendMenu("f-ayse", false));
        step(0.4, "shot guest menu", mc -> shot(mc, "menu-guest"));
        step(0.1, "send off", mc -> need(mc).pressMenu(FriendMenu.Action.REMOVE_FROM_WORLD));
        step(0.4, "shot send-off question", mc -> {
            shot(mc, "ask-remove-from-world");
            results.addProperty("removeAsked", need(mc).menuAsking());
        });
        step(0.1, "confirm", mc -> need(mc).answerMenu(true));
        step(0.8, "shot sent off", mc -> {
            shot(mc, "note-removed-from-world");
            results.addProperty("ayseSentOff",
                WorldInvites.guest(OverlayClient.get().state().host(), "f-ayse") == null);
        });
        // "Arkadaşlıktan çıkar": its question; "Vazgeç" first, then for real.
        step(0.4, "Zeynep's menu", mc -> need(mc).openFriendMenu("f-zeynep", false));
        step(0.3, "unfriend", mc -> need(mc).pressMenu(FriendMenu.Action.UNFRIEND));
        step(0.4, "shot unfriend question", mc -> {
            shot(mc, "ask-unfriend");
            results.addProperty("unfriendAsked", need(mc).menuAsking());
        });
        step(0.1, "cancel", mc -> need(mc).answerMenu(false));
        step(0.3, "Zeynep's menu again", mc -> need(mc).openFriendMenu("f-zeynep", false));
        step(0.3, "unfriend again", mc -> need(mc).pressMenu(FriendMenu.Action.UNFRIEND));
        step(0.3, "confirm", mc -> need(mc).answerMenu(true));
        step(0.7, "shot unfriended", mc -> {
            shot(mc, "note-unfriended");
            results.addProperty("zeynepGone", OverlayClient.get().state().friend("f-zeynep") == null);
        });
        // "Engelle": its question says what a block means.
        step(0.2, "Deniz's menu", mc -> need(mc).openFriendMenu("f-deniz", false));
        step(0.3, "block", mc -> need(mc).pressMenu(FriendMenu.Action.BLOCK));
        step(0.4, "shot block question", mc -> shot(mc, "ask-block"));
        step(0.1, "cancel", mc -> need(mc).answerMenu(false));
        // "Rapor et": the dialog, then filled, then sent. The driver's pointer rests in a corner: the dialog stands
        // where the game's own (unmoved) pointer would light a row.
        step(0.3, "Elif's menu", mc -> {
            PanelInput.pointAt(1, 1);
            need(mc).openFriendMenu("f-elif", false);
        });
        step(0.3, "report", mc -> need(mc).pressMenu(FriendMenu.Action.REPORT));
        step(0.5, "shot report", mc -> {
            shot(mc, "report-empty");
            results.addProperty("reportOpen", Compat.screen(mc) instanceof ReportScreen);
        });
        step(0.1, "fill report", mc -> {
            if (Compat.screen(mc) instanceof ReportScreen r) {
                r.fillForDriver("harassment", "Oyunda sürekli küfür ediyor (ğüşıöç)", true);
            }
        });
        step(0.3, "shot report filled", mc -> shot(mc, "report-filled"));
        step(0.1, "send report", mc -> {
            if (Compat.screen(mc) instanceof ReportScreen r) {
                results.addProperty("reportSent", r.sendForDriver());
            }
        });
        step(0.6, "shot report note", mc -> {
            shot(mc, "note-reported");
            PanelInput.clearPointer();
        });
        // A row's own press: Ayşe's conversation, the field ready.
        step(0.2, "press Ayşe", mc -> need(mc).pressFriend("f-ayse"));
        step(0.6, "shot conversation", mc -> {
            shot(mc, "dm-ayse");
            results.addProperty("pageAfterPress", need(mc).page().name());
            results.addProperty("conversation", need(mc).openConversation());
            results.addProperty("fieldFocused", need(mc).input().isFocused());
        });
        // A friend with no conversation yet: an empty one opens; its first message makes it real.
        step(0.2, "friends", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.4, "press Deniz", mc -> need(mc).pressFriend("f-deniz"));
        step(0.6, "shot new conversation", mc -> {
            shot(mc, "dm-new");
            results.addProperty("newConversation", need(mc).openConversation());
        });
        step(0.1, "type", mc -> need(mc).input().setValue("Akşam bedwars var mı?"));
        step(0.2, "send", mc -> key(need(mc), InputConstants.KEY_RETURN, 0));
        step(1.0, "shot sent", mc -> {
            shot(mc, "dm-new-sent");
            String conv = need(mc).openConversation();
            results.addProperty("madeReal", conv != null && OverlayClient.get().state().conversation(conv) != null);
        });
        // The tag taken off: the header shows the name alone.
        step(0.2, "badge off", mc -> dev("badge-off"));
        step(0.8, "shot header without tag", mc -> {
            need(mc).showPage(OverlayScreen.Page.FRIENDS);
            results.addProperty("meBadgeAfterOff", String.valueOf(OverlayClient.get().state().meBadge()));
        });
        step(0.5, "shot no tag", mc -> shot(mc, "friends-no-tag"));
        step(0.1, "badge on", mc -> dev("badge-on"));
        step(0.5, "close", mc -> need(mc).close());
        // English: the menu and the questions in the other language.
        step(0.8, "english", mc -> {
            mc.options.languageCode = "en_us";
            mc.getLanguageManager().setSelected("en_us");
            reload = mc.reloadResourcePacks();
        });
        step(0.5, "wait reload", mc -> {
            if (reload != null && !reload.isDone()) {
                next--;
            }
        });
        step(2.0, "open english", mc -> OverlayClient.get().togglePanel());
        step(0.5, "friends english", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.3, "menu english", mc -> need(mc).openFriendMenu("f-ayse", false));
        step(0.5, "shot english menu", mc -> shot(mc, "en-menu"));
        step(0.1, "block english", mc -> need(mc).pressMenu(FriendMenu.Action.BLOCK));
        step(0.4, "shot english block", mc -> shot(mc, "en-ask-block"));
        step(0.1, "cancel", mc -> need(mc).answerMenu(false));
        // Elif is blocked by now (her report blocked her): Mert's dialog.
        step(0.3, "report english", mc -> {
            PanelInput.pointAt(1, 1);
            need(mc).openFriendMenu("f-mert", false);
        });
        step(0.3, "press report", mc -> need(mc).pressMenu(FriendMenu.Action.REPORT));
        step(0.5, "shot english report", mc -> {
            shot(mc, "en-report");
            results.addProperty("englishReportOpen", Compat.screen(mc) instanceof ReportScreen);
        });
        step(0.1, "back", mc -> {
            if (Compat.screen(mc) instanceof ReportScreen r) {
                r.onClose();
            }
            PanelInput.clearPointer();
        });
        step(0.3, "close", mc -> need(mc).close());
        pauseMatrix();
    }

    /**
     * The pause menu at GUI scales 1 to 4 and in a small window (the game resizes its own window; no OS input): each a
     * screenshot and a check that the button stands inside the screen and clear of every other button (Mod Menu's when it
     * is in the run). Then the focused button and the panel it opens.
     */
    private void pauseMatrix() {
        JsonObject checks = new JsonObject();
        results.add("pauseChecks", checks);
        int[][] sizes = {{1280, 720, 1, 2, 3}, {1280, 960, 4}, {854, 480, 1, 2}};
        for (int[] size : sizes) {
            int w = size[0];
            int h = size[1];
            step(0.4, "window " + w + "x" + h, mc -> {
                Compat.setScreen(mc, null);
                mc.getWindow().setWindowed(w, h);
            });
            for (int i = 2; i < size.length; i++) {
                int scale = size[i];
                String label = w + "x" + h + "-gui" + scale;
                step(1.2, "gui " + label, mc -> {
                    mc.options.guiScale().set(scale);
                    mc.resizeGui();
                    Compat.setScreen(mc, new PauseScreen(true));
                });
                step(0.9, "shot " + label, mc -> {
                    shot(mc, "pause-" + label);
                    checks.add(label, checkPause(mc));
                });
            }
        }
        step(0.4, "window back", mc -> {
            Compat.setScreen(mc, null);
            mc.getWindow().setWindowed(1280, 720);
        });
        step(1.2, "gui 2", mc -> {
            mc.options.guiScale().set(2);
            mc.resizeGui();
        });
        pauseSteps();
    }

    /** Where the pause menu's button stands, and whether it is inside the screen and clear of every other button. */
    private static JsonObject checkPause(Minecraft mc) {
        JsonObject o = new JsonObject();
        Screen s = Compat.screen(mc);
        PausePanelButton b = PausePanelButton.last();
        o.addProperty("window", mc.getWindow().getWidth() + "x" + mc.getWindow().getHeight());
        o.addProperty("guiScale", mc.getWindow().getGuiScale());
        if (!(s instanceof PauseScreen) || b == null || !Screens.getWidgets(s).contains(b)) {
            o.addProperty("button", false);
            return o;
        }
        o.addProperty("screen", s.width + "x" + s.height);
        o.addProperty("button", b.visible);
        o.addProperty("at", b.getX() + "," + b.getY());
        o.addProperty("inside", b.getX() >= 0 && b.getY() >= 0 && b.getX() + b.getWidth() <= s.width
            && b.getY() + b.getHeight() <= s.height);
        JsonArray overlaps = new JsonArray();
        JsonArray widgets = new JsonArray();
        for (AbstractWidget w : Screens.getWidgets(s)) {
            if (w == b || !w.visible) {
                continue;
            }
            widgets.add(w.getMessage().getString() + " @" + w.getX() + "," + w.getY() + " " + w.getWidth() + "x"
                + w.getHeight());
            if (w.getX() < b.getX() + b.getWidth() && w.getX() + w.getWidth() > b.getX()
                && w.getY() < b.getY() + b.getHeight() && w.getY() + w.getHeight() > b.getY()) {
                overlaps.add(w.getMessage().getString());
            }
        }
        o.add("overlaps", overlaps);
        o.add("widgets", widgets);
        return o;
    }

    /** The pause menu: the Projects button in its bottom right corner, and the panel it opens. */
    private void pauseSteps() {
        step(0.8, "pause", mc -> Compat.setScreen(mc, new PauseScreen(true)));
        step(0.8, "shot pause", mc -> {
            shot(mc, "pause");
            PausePanelButton b = PausePanelButton.last();
            results.addProperty("pauseButton", b != null && b.visible);
            if (b != null) {
                results.addProperty("pauseButtonAt", b.getX() + "," + b.getY());
            }
        });
        // Tab walks the menu's buttons as the game's own keys do; how many presses reach ours (-1: never).
        step(0.1, "tab to the button", mc -> {
            PausePanelButton b = PausePanelButton.last();
            if (b != null && Compat.screen(mc) instanceof PauseScreen p) {
                // As after a real Tab: the focused button shows its tooltip.
                mc.setLastInputType(InputType.KEYBOARD_TAB);
                // 26.3's key events carry SDL's keycode as well, and the menus read Tab from it (SDLK_TAB, 9).
                int tabCode = Compat.VERSION.equals("26.3") ? 9 : 0;
                int tabs = 0;
                while (p.getFocused() != b && tabs < 24) {
                    p.keyPressed(new KeyEvent(InputConstants.KEY_TAB, tabCode, 0));
                    tabs++;
                }
                results.addProperty("tabsToButton", p.getFocused() == b ? tabs : -1);
                if (p.getFocused() != b) {
                    p.setFocused(b);
                }
            }
        });
        step(0.6, "shot pause focused", mc -> shot(mc, "pause-focused"));
        // Enter on the focused button, through the menu's own key handling.
        step(0.1, "press it", mc -> {
            if (Compat.screen(mc) instanceof PauseScreen p) {
                p.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
            }
            results.addProperty("enterOpened", panel(mc) != null);
            mc.setLastInputType(InputType.NONE);
            PausePanelButton b = PausePanelButton.last();
            if (panel(mc) == null && b != null) {
                b.pressForDriver();
            }
        });
        step(0.8, "shot panel from pause", mc -> {
            shot(mc, "pause-panel");
            results.addProperty("panelFromPause", panel(mc) != null);
        });
        step(0.3, "close", mc -> {
            OverlayScreen p = panel(mc);
            if (p != null) {
                p.close();
            }
        });
    }

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        results.addProperty("runSeconds", round((System.nanoTime() - runStarted) / 1e9));
        write();
        mc.stop();
    }

    /** Rests the driver's pointer on a point (the panel draws as if the mouse were there). */
    private void pointAt(float[] at) {
        if (at == null) {
            event("point", "nothing there");
            return;
        }
        PanelInput.pointAt(Math.round(at[0]), Math.round(at[1]));
        event("point", Math.round(at[0]) + "," + Math.round(at[1]));
    }

    /** A key as the game would send it ({@code shift}: 1 with Shift held), through the panel's own handling. */
    private static void key(OverlayScreen panel, int key, int shift) {
        // GLFW's and SDL's Shift modifier bits: 1 (GLFW) and 3 (SDL's both Shift keys) both read as Shift held.
        int modifiers = shift == 0 ? 0 : Compat.VERSION.equals("26.3") ? 0x0003 : 0x0001;
        panel.keyPressed(new KeyEvent(key, 0, modifiers));
    }

    private void dev(String cmd, String... pairs) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            o.addProperty(pairs[i], pairs[i + 1]);
        }
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL-UX scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261024L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("people-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    private static JsonObject perf(long[] drained) {
        JsonObject o = new JsonObject();
        o.addProperty("panelCalls", drained[3]);
        o.addProperty("panelAvgMicros", drained[3] == 0 ? 0 : round(drained[2] / 1e3 / drained[3]));
        return o;
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", round((System.nanoTime() - runStarted) / 1e9));
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }

    private void write() {
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("people-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
