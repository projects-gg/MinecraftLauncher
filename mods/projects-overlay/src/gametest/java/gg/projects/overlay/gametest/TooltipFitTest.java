package gg.projects.overlay.gametest;

import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.tweaks.TooltipFit;
import java.util.ArrayList;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.gui.components.AbstractButton;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.events.ContainerEventHandler;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.dialog.DialogScreen;
import net.minecraft.network.chat.Component;

/**
 * TOOLTIP-FIT in a real game, on the jar players get with the launcher's tweaks property (build.gradle): the owner's
 * server menu (six games in two columns, each button's tooltip a page of text) on the owner's window, where the game
 * puts the tooltip above the button and off the top of the screen. With the tweak it shows whole, at full size; one
 * taller than the whole screen shows whole too, drawn smaller.
 */
@SuppressWarnings("UnstableApiUsage")
public final class TooltipFitTest implements FabricClientGameTest {
    private static final int SETTLE_TICKS = 5;
    /** The owner's window, at the game's own scale 2 (the scale the window gets by itself). */
    private static final int WINDOW_W = 1578;
    private static final int WINDOW_H = 700;

    /** A game's page, as the owner's server writes it: a text, then a list of short lines. */
    private static final String PAGE = "Zephyr, Projects'in MMO-Survival sunucusudur. Klasik hayatta kalma deneyimini "
        + "sınıflar, yetenekler, zindanlar ve boss savaşlarıyla birleştirir ve her zaman geliştirme aşamasındadır.\\n\\n"
        + "Düzenli olarak yeni etkinlikler, yeni taktiksel eşyalar ve hayatta kalma kuralları geliştirir. Sezonlar "
        + "boyunca oyuncular kendi kasabalarını kurar, takımlar halinde savaşır ve sunucunun hikâyesini birlikte "
        + "yazar.\\n\\nNitelikler:\\n * Mevsimler\\n * Kanlı Ay\\n * Ünvanlar\\n * Arkeoloji\\n * Takımlar\\n * Turf\\n"
        + " * MMO-Survival\\n\\n> Oyuna katılmak için tıklayın!";

    /** A page longer than the window's screen: fifty lines, shown whole at two thirds of its size. */
    private static final String SCROLL = " * Satır\\n".repeat(50);

    @Override
    public void runTest(ClientGameTestContext context) {
        context.waitForScreen(TitleScreen.class);
        int[] size = context.computeOnClient(mc -> new int[] {
            mc.getWindow().getScreenWidth(), mc.getWindow().getScreenHeight()});
        try (TestSingleplayerContext world = context.worldBuilder().create()) {
            context.waitFor(mc -> mc.player != null && mc.level != null);
            context.waitTicks(20);
            context.getInput().resizeWindow(WINDOW_W, WINDOW_H);
            context.waitTicks(SETTLE_TICKS);

            show(context, world, menu(PAGE));
            AbstractWidget zephyr = button(context, "Zephyr");
            ScreenRectangle at = zephyr.getRectangle();
            int screenH = context.computeOnClient(mc -> mc.getWindow().getGuiScaledHeight());
            int gameHeight = context.computeOnClient(mc -> mc.font.split(Component.literal(PAGE.replace("\\n", "\n")), 170)
                .size()) * 10 - 2;
            expect(gameHeight > at.top() - 16 && gameHeight > screenH - 11 - at.bottom() - at.height(),
                "the page fits neither above nor below its button at the game's width (" + gameHeight + " high, button "
                    + at.top() + " to " + at.bottom() + " on " + screenH + ")");
            TooltipFit.Placed placed = hover(context, zephyr);
            expect(placed.place().scale() == 1F, "the page shows at full size, not " + placed.place().scale());
            onScreen(placed);
            context.takeScreenshot("overlay-tooltip");

            // Taller than the whole screen: smaller, and whole.
            show(context, world, menu(SCROLL));
            TooltipFit.Placed scroll = hover(context, button(context, "Zephyr"));
            expect(scroll.place().scale() < 1F, "a page taller than the screen is drawn smaller");
            onScreen(scroll);
            context.takeScreenshot("overlay-tooltip-scaled");

            context.getInput().setCursorPos(0, 0);
            context.getInput().resizeWindow(size[0], size[1]);
            context.waitTicks(SETTLE_TICKS);
            context.setScreen(() -> null);
        }
    }

    /** The owner's menu: a text and six games in two columns, the first with {@code page} as its tooltip. */
    private static String menu(String page) {
        StringBuilder actions = new StringBuilder();
        String[] games = {"Zephyr [MMO-Survival]", "ProSMP [Vanilla]", "Aurora [SkyBlock]", "Açlık Oyunları [MG]",
            "Aergia [PvP]", "[Survival] TestServer"};
        for (int i = 0; i < games.length; i++) {
            actions.append(i == 0 ? "" : ",").append("{label:\"").append(games[i]).append("\",width:200,tooltip:\"")
                .append(i == 0 ? page : "Kısa bir açıklama.").append("\"}");
        }
        return "{type:\"minecraft:multi_action\",title:\"PROJECTS 2026\",pause:false,columns:2,"
            + "body:{type:\"minecraft:plain_message\",contents:\"Oynamak için bir oyun seçiniz:\"},"
            + "actions:[" + actions + "]}";
    }

    /** Puts the pointer on {@code widget} and answers the tooltip the tweak placed for it. */
    private static TooltipFit.Placed hover(ClientGameTestContext context, AbstractWidget widget) {
        context.getInput().setCursorPos(0, 0);
        context.waitTicks(2);
        TooltipFit.Placed before = TooltipFit.last();
        double[] point = context.computeOnClient(mc -> {
            double scale = mc.getWindow().getScreenWidth() / (double) mc.getWindow().getGuiScaledWidth();
            return new double[] {(widget.getX() + 12) * scale, (widget.getY() + widget.getHeight() / 2.0) * scale};
        });
        context.getInput().setCursorPos(point[0], point[1]);
        context.waitTicks(SETTLE_TICKS);
        TooltipFit.Placed placed = TooltipFit.last();
        expect(placed != null && placed != before, "the button's tooltip was drawn through the tweak");
        return placed;
    }

    /** The tooltip's frame shows whole on the screen. */
    private static void onScreen(TooltipFit.Placed placed) {
        TooltipFit.Place place = placed.place();
        int w = Math.round(placed.w() * place.scale());
        int h = Math.round(placed.h() * place.scale());
        expect(place.x() - 4 >= 0 && place.x() + w + 4 <= placed.screenW(),
            "the tooltip shows whole across: " + place.x() + "+" + w + " on " + placed.screenW());
        expect(place.y() - 4 >= 0 && place.y() + h + 4 <= placed.screenH(),
            "the tooltip shows whole down: " + place.y() + "+" + h + " on " + placed.screenH());
    }

    /** Shows {@code dialog} from the world's server and waits for its window (a new one). */
    private static void show(ClientGameTestContext context, TestSingleplayerContext world, String dialog) {
        Screen before = context.computeOnClient(Compat::screen);
        world.getServer().runCommand("dialog show @a " + dialog);
        context.waitFor(mc -> Compat.screen(mc) instanceof DialogScreen<?> shown && shown != before);
        context.waitTicks(SETTLE_TICKS);
    }

    /** The window's button whose label starts with {@code label}. */
    private static AbstractWidget button(ClientGameTestContext context, String label) {
        List<AbstractWidget> found = context.computeOnClient(mc -> {
            List<AbstractWidget> buttons = new ArrayList<>();
            collect(Compat.screen(mc), buttons, 0);
            return buttons;
        });
        for (AbstractWidget widget : found) {
            if (widget.getMessage().getString().startsWith(label)) {
                return widget;
            }
        }
        throw new AssertionError("TOOLTIP-FIT: the menu has no " + label + " button");
    }

    private static void collect(GuiEventListener node, List<AbstractWidget> found, int depth) {
        if (!(node instanceof ContainerEventHandler holder) || depth > 12) {
            return;
        }
        for (GuiEventListener child : holder.children()) {
            if (child instanceof AbstractButton button) {
                found.add(button);
            } else {
                collect(child, found, depth + 1);
            }
        }
    }

    private static void expect(boolean holds, String what) {
        if (!holds) {
            throw new AssertionError("TOOLTIP-FIT: expected " + what);
        }
    }
}
