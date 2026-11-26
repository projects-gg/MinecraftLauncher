package gg.projects.overlay.gametest;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.tweaks.PasswordEye;
import gg.projects.overlay.tweaks.SecretBox;
import java.util.ArrayList;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.events.ContainerEventHandler;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.dialog.DialogScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;

/**
 * PASSWORD-EYE in a real game, on the jar players get with the launcher's tweaks property (build.gradle): the world's
 * server shows windows the way a server does ({@code /dialog show}). The owner's login window hides its password box
 * (the window still gets the text as typed), its eye shows and hides it, copying a hidden password copies nothing, a
 * box whose label is hidden takes the password text above it, and a form without passwords is left alone.
 */
@SuppressWarnings("UnstableApiUsage")
public final class PasswordEyeTest implements FabricClientGameTest {
    /** Ticks a window gets to arrive and settle. */
    private static final int SETTLE_TICKS = 5;

    /** The owner's server login window: a greeting, the labelled password box, a check box. */
    private static final String LOGIN = "{type:\"minecraft:notice\",title:\"PROJECTS 2026\",pause:false,"
        + "body:{type:\"minecraft:plain_message\",contents:\"Merhaba Kaan_07! Lütfen aşağıya parolanızı yazınız:\"},"
        + "inputs:[{type:\"minecraft:text\",key:\"password\",label:\"Şifre:\",width:200},"
        + "{type:\"minecraft:boolean\",key:\"auto\",label:\"Otomatik Giriş\"}]}";

    /** A password box whose label the window hides, under a text that asks for the password. */
    private static final String HIDDEN_LABEL = "{type:\"minecraft:notice\",title:\"Giriş\",pause:false,"
        + "body:{type:\"minecraft:plain_message\",contents:\"Parolanı yaz:\"},"
        + "inputs:[{type:\"minecraft:text\",key:\"code\",label:\"Kod\",label_visible:false}]}";

    /** A form without passwords. */
    private static final String FORM = "{type:\"minecraft:notice\",title:\"Klan Kur\",pause:false,"
        + "body:{type:\"minecraft:plain_message\",contents:\"Klanına bir ad ver.\"},"
        + "inputs:[{type:\"minecraft:text\",key:\"name\",label:\"Ad\"},"
        + "{type:\"minecraft:text\",key:\"tag\",label:\"Etiket\"}]}";

    @Override
    public void runTest(ClientGameTestContext context) {
        context.waitForScreen(TitleScreen.class);
        try (TestSingleplayerContext world = context.worldBuilder().create()) {
            context.waitFor(mc -> mc.player != null && mc.level != null);
            context.waitTicks(20);

            show(context, world, LOGIN);
            List<EditBox> boxes = boxes(context);
            expect(boxes.size() == 1, "the login window has one text box, not " + boxes.size());
            EditBox box = boxes.getFirst();
            expect(box instanceof SecretBox, "the game's text box takes the tweak");
            expect(context.computeOnClient(mc -> ((SecretBox) box).projects$hidden()), "the password box hides its text");
            context.getInput().typeChars("gizli 123");
            context.waitTicks(2);
            expect("gizli 123".equals(context.computeOnClient(mc -> box.getValue())),
                "the box keeps the text as typed, for the window to send");
            context.takeScreenshot("overlay-password");

            // A hidden password copies nothing, when this machine's clipboard works at all.
            boolean clipboard = context.computeOnClient(mc -> {
                mc.keyboardHandler.setClipboard("before");
                return "before".equals(mc.keyboardHandler.getClipboard());
            });
            if (clipboard) {
                expect("before".equals(copyAll(context, box)), "copying a hidden password copies nothing");
            }

            // The eye shows the text, and hides it again.
            pressEye(context, box);
            expect(!context.computeOnClient(mc -> ((SecretBox) box).projects$hidden()), "a press on the eye shows it");
            expect("gizli 123".equals(context.computeOnClient(mc -> box.getValue())), "the eye changes no text");
            context.takeScreenshot("overlay-password-shown");
            if (clipboard) {
                expect("gizli 123".equals(copyAll(context, box)), "a shown password copies as any text");
            }
            pressEye(context, box);
            expect(context.computeOnClient(mc -> ((SecretBox) box).projects$hidden()), "a second press hides it");

            // A box without a label shown takes the text right above it.
            show(context, world, HIDDEN_LABEL);
            List<EditBox> code = boxes(context);
            expect(code.size() == 1 && context.computeOnClient(mc -> ((SecretBox) code.getFirst()).projects$hidden()),
                "a box under \"Parolanı yaz:\" hides its text");

            // A form without passwords is the game's own.
            show(context, world, FORM);
            for (EditBox plain : boxes(context)) {
                expect(!context.computeOnClient(mc -> ((SecretBox) plain).projects$isSecret()),
                    "a box labelled " + plain.getMessage().getString() + " stays clear");
            }
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

    /** Presses the eye of {@code box} the way a click reaches it: through the window. */
    private static void pressEye(ClientGameTestContext context, EditBox box) {
        context.runOnClient(mc -> {
            double x = box.getX() + box.getWidth() - 4 - 5;
            double y = box.getY() + box.getHeight() / 2.0;
            expect(PasswordEye.onEye(box, x, y), "the eye is where the test presses");
            Screen screen = Compat.screen(mc);
            MouseButtonEvent press = new MouseButtonEvent(x, y, new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0));
            screen.mouseClicked(press, false);
            screen.mouseReleased(press);
        });
        context.waitTicks(2);
    }

    /** Selects the whole text of {@code box} and presses Ctrl+C on it, as the game hands the box a key; the clipboard. */
    private static String copyAll(ClientGameTestContext context, EditBox box) {
        return context.computeOnClient(mc -> {
            box.moveCursorToEnd(false);
            box.setHighlightPos(0);
            // The key, and the layout's key code a version reads shortcuts from (SDL's 'c'; GLFW reads the key).
            box.keyPressed(new KeyEvent(InputConstants.KEY_C, 'c', InputConstants.MOD_CONTROL));
            return mc.keyboardHandler.getClipboard();
        });
    }

    /** The window's one-line text boxes, in the order it lays them out. */
    private static List<EditBox> boxes(ClientGameTestContext context) {
        return context.computeOnClient(mc -> {
            List<EditBox> found = new ArrayList<>();
            collect(Compat.screen(mc), found, 0);
            return found;
        });
    }

    private static void collect(GuiEventListener node, List<EditBox> found, int depth) {
        if (!(node instanceof ContainerEventHandler holder) || depth > 12) {
            return;
        }
        for (GuiEventListener child : holder.children()) {
            if (child instanceof EditBox box) {
                found.add(box);
            } else {
                collect(child, found, depth + 1);
            }
        }
    }

    private static void expect(boolean holds, String what) {
        if (!holds) {
            throw new AssertionError("PASSWORD-EYE: expected " + what);
        }
    }
}
