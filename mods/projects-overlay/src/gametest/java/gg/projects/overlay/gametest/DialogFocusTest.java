package gg.projects.overlay.gametest;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import java.util.ArrayList;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Checkbox;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.MultiLineEditBox;
import net.minecraft.client.gui.components.events.ContainerEventHandler;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.dialog.DialogScreen;

/**
 * DIALOG-FOCUS in a real game, on the jar players get with the launcher's {@code -Dprojects.overlay.tweaks=dialog-focus}
 * (build.gradle) and no launcher listening on the bridge: the world's server shows windows the way a server does
 * ({@code /dialog show}), and the test looks where the focus went. A form's first text box takes the typing at once (not
 * its check box); a box of several lines counts; a window without a text box, and one whose box is below the fold, are
 * left alone; the box the player Tabs to keeps the focus through a resize. Only Fabric's test calls, the game's own
 * classes and the mod's {@link Compat}, so one source serves every version.
 */
@SuppressWarnings("UnstableApiUsage")
public final class DialogFocusTest implements FabricClientGameTest {
    /** Ticks a window gets to arrive and settle. */
    private static final int SETTLE_TICKS = 5;

    /** A form as a server's "Klan Kur" window: a line of text, a check box, then two labelled text boxes. */
    private static final String FORM = "{type:\"minecraft:notice\",title:\"Klan Kur\",pause:false,"
        + "body:{type:\"minecraft:plain_message\",contents:\"Klanına bir ad ver.\"},"
        + "inputs:[{type:\"minecraft:boolean\",key:\"open\",label:\"Herkese Açık\"},"
        + "{type:\"minecraft:text\",key:\"name\",label:\"Ad\"},"
        + "{type:\"minecraft:text\",key:\"tag\",label:\"Etiket\"}]}";

    /** A box of several lines first, a one-line box after it. */
    private static final String NOTE = "{type:\"minecraft:notice\",title:\"Bildir\",pause:false,"
        + "inputs:[{type:\"minecraft:text\",key:\"note\",label:\"Not\",multiline:{max_lines:3}},"
        + "{type:\"minecraft:text\",key:\"name\",label:\"Oyuncu\"}]}";

    /** No text box: a check box alone. */
    private static final String RULES = "{type:\"minecraft:notice\",title:\"Kurallar\",pause:false,"
        + "body:{type:\"minecraft:plain_message\",contents:\"Sohbette saygılı ol.\"},"
        + "inputs:[{type:\"minecraft:boolean\",key:\"agree\",label:\"Okudum\"}]}";

    /** A text box under a text far longer than the window: it does not show without scrolling. */
    private static final String LONG = "{type:\"minecraft:notice\",title:\"Kurallar\",pause:false,"
        + "body:{type:\"minecraft:plain_message\",contents:\"" + "Kurallar burada. ".repeat(160) + "\"},"
        + "inputs:[{type:\"minecraft:text\",key:\"code\",label:\"Kod\"}]}";

    @Override
    public void runTest(ClientGameTestContext context) {
        context.waitForScreen(TitleScreen.class);
        try (TestSingleplayerContext world = context.worldBuilder().create()) {
            context.waitFor(mc -> mc.player != null && mc.level != null);
            context.waitTicks(20);

            // A form: the first text box has the focus (not the check box above it) and takes the typing.
            show(context, world, FORM);
            List<AbstractWidget> boxes = boxes(context);
            expect(boxes.size() == 2, "the form has two text boxes, not " + boxes.size());
            expect(focused(context) == boxes.get(0), "the form's first text box has the focus");
            expect(boxes.get(0).isFocused(), "the first text box shows its caret");
            context.getInput().typeChars("Kaan_07");
            context.waitTicks(2);
            expect("Kaan_07".equals(((EditBox) boxes.get(0)).getValue()), "the typing went into the first text box");
            context.takeScreenshot("overlay-dialog");

            // A text box under a long text: it is below the fold, so nothing takes the focus.
            show(context, world, LONG);
            expect(boxes(context).size() == 1, "the long window has its text box");
            expect(!(focused(context) instanceof EditBox), "a text box below the fold takes no focus");

            // A box of several lines first: it is the one.
            show(context, world, NOTE);
            List<AbstractWidget> note = boxes(context);
            expect(note.size() == 2 && note.get(0) instanceof MultiLineEditBox, "the note window starts with a box of lines");
            expect(focused(context) == note.get(0), "a box of several lines takes the focus too");

            // No text box: nothing moves into the window's inputs.
            show(context, world, RULES);
            expect(boxes(context).isEmpty(), "the rules window has no text box");
            expect(!(focused(context) instanceof Checkbox), "a window without a text box keeps the game's own focus");

            // The player Tabs to the second box; a resize keeps it there, and the first box keeps its text.
            show(context, world, FORM);
            List<AbstractWidget> again = boxes(context);
            expect(focused(context) == again.get(0), "the form's first text box has the focus again");
            context.getInput().typeChars("Ada_42");
            context.getInput().pressKey(InputConstants.KEY_TAB);
            context.waitTicks(2);
            expect(focused(context) == again.get(1), "Tab moved the focus to the second text box");
            int[] size = context.computeOnClient(mc -> new int[] {
                mc.getWindow().getScreenWidth(), mc.getWindow().getScreenHeight()});
            context.getInput().resizeWindow(size[0] + 146, size[1] + 160);
            context.waitTicks(SETTLE_TICKS);
            expect(focused(context) == again.get(1), "a resize keeps the player's focus on the second text box");
            expect("Ada_42".equals(((EditBox) again.get(0)).getValue()), "a resize keeps the first box's text");
            context.getInput().resizeWindow(size[0], size[1]);
            context.waitTicks(SETTLE_TICKS);
            expect(focused(context) == again.get(1), "the focus stays where the player put it");
            context.setScreen(() -> null);
        }
    }

    /** Shows {@code dialog} from the world's server and waits for its window (a new one). */
    private static void show(ClientGameTestContext context, TestSingleplayerContext world, String dialog) {
        Screen before = context.computeOnClient(Compat::screen);
        world.getServer().runCommand("dialog show @a " + dialog);
        context.waitFor(mc -> Compat.screen(mc) instanceof DialogScreen<?> shown && shown != before);
        context.waitTicks(SETTLE_TICKS);
    }

    /** The window's text boxes, in the order it lays them out. */
    private static List<AbstractWidget> boxes(ClientGameTestContext context) {
        return context.computeOnClient(mc -> {
            List<AbstractWidget> found = new ArrayList<>();
            collect(Compat.screen(mc), found, 0);
            return found;
        });
    }

    private static void collect(GuiEventListener node, List<AbstractWidget> found, int depth) {
        if (!(node instanceof ContainerEventHandler holder) || depth > 12) {
            return;
        }
        for (GuiEventListener child : holder.children()) {
            if (child instanceof EditBox || child instanceof MultiLineEditBox) {
                found.add((AbstractWidget) child);
            } else {
                collect(child, found, depth + 1);
            }
        }
    }

    /** The widget at the end of the window's focus path, or null when nothing has the focus. */
    private static GuiEventListener focused(ClientGameTestContext context) {
        return context.computeOnClient(DialogFocusTest::focusedIn);
    }

    private static GuiEventListener focusedIn(Minecraft mc) {
        Screen screen = Compat.screen(mc);
        GuiEventListener at = screen == null ? null : screen.getFocused();
        for (int depth = 0; depth < 12 && at instanceof ContainerEventHandler holder && holder.getFocused() != null;
             depth++) {
            at = holder.getFocused();
        }
        return at;
    }

    private static void expect(boolean holds, String what) {
        if (!holds) {
            throw new AssertionError("DIALOG-FOCUS: expected " + what);
        }
    }
}
