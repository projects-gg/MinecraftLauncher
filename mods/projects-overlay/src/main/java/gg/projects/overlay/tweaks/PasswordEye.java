package gg.projects.overlay.tweaks;

import com.mojang.blaze3d.platform.cursor.CursorTypes;
import java.util.ArrayList;
import java.util.List;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractStringWidget;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.events.ContainerEventHandler;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.dialog.DialogScreen;
import net.minecraft.network.chat.Component;
import org.jetbrains.annotations.Nullable;

/**
 * PASSWORD-EYE: a server window's password box (1.21.6+ dialogs, where servers ask for the login password) shows
 * filled dots instead of what is typed, with an eye at its right end that shows and hides it. Minecraft has no hidden
 * box of its own, so a password typed into a server's login window sits in clear text on screen, in screenshots and
 * on stream.
 *
 * <p>As a window opens, its one-line text boxes are read with {@link SecretLabel}: a box whose label names a password,
 * or, without a label shown, whose text right above does. The box keeps the real text (the window sends it as typed);
 * only how it is drawn and measured changes ({@code EditBoxMixin}): dots of the same count, the caret and selection on
 * them, no copying or cutting while hidden, words not told apart (Ctrl+arrows and a double click take the whole), and
 * the narrator never reads it. The player's choice holds for that box until the window closes.
 */
public final class PasswordEye implements ClientTweak {
    /** Its id ({@link ClientTweaks#PROPERTY}; the launcher's `pl_overlay::tweaks::PASSWORD_EYE`). */
    static final String ID = "password-eye";
    /** The dot drawn for each character: the game's font has it as a round 4 by 4 dot. */
    static final char DOT = '\u25CF';
    /** The eye's distance from the box's right edge, like the text's from its left. */
    private static final int RIGHT = 4;
    /** The room the text gives up for the eye: the eye and a gap before it. */
    public static final int SPACE = EyeIcon.W + 3;
    /** The eye's colours: at rest, under the pointer, on a box that takes no typing. */
    private static final int REST = 0xFFA0A0A0;
    private static final int HOVER = 0xFFFFFFFF;
    private static final int OFF = 0xFF707070;
    /** Deeper than any window nests its widgets. */
    private static final int MAX_DEPTH = 12;
    private static final List<EyeIcon.Run> OPEN = EyeIcon.runs(EyeIcon.OPEN);
    private static final List<EyeIcon.Run> STRUCK = EyeIcon.runs(EyeIcon.STRUCK);

    @Nullable
    private static volatile TweakGuard guard;

    @Override
    public String id() {
        return ID;
    }

    @Override
    public void install(TweakGuard guard) {
        PasswordEye.guard = guard;
        // Every initialisation of every screen, a resize too: a window's boxes keep their marks and the player's
        // choice.
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) -> guard.run(() -> mark(screen)));
    }

    /** The tweak is on and has not failed. */
    public static boolean on() {
        TweakGuard at = guard;
        return at != null && at.on();
    }

    /** What a hidden box draws and measures for {@code value}: a dot for each character, so every index holds. */
    public static String mask(String value) {
        return String.valueOf(DOT).repeat(value.length());
    }

    /** Marks the password boxes of a server window; any other screen is left alone. */
    static void mark(Screen screen) {
        if (!(screen instanceof DialogScreen<?>)) {
            return;
        }
        List<GuiEventListener> widgets = new ArrayList<>();
        List<SecretLabel.Node> nodes = new ArrayList<>();
        walk(screen, widgets, nodes, 0);
        boolean[] secret = SecretLabel.secret(nodes);
        for (int i = 0; i < secret.length; i++) {
            if (widgets.get(i) instanceof SecretBox box) {
                box.projects$secret(secret[i]);
            }
        }
    }

    /** The window's widgets in the order it lays them out: texts, one-line boxes and the rest; holders opened. */
    private static void walk(GuiEventListener node, List<GuiEventListener> widgets, List<SecretLabel.Node> nodes,
                             int depth) {
        if (depth > MAX_DEPTH || (node instanceof AbstractWidget widget && !widget.visible)) {
            return;
        }
        if (node instanceof EditBox box) {
            widgets.add(box);
            nodes.add(new SecretLabel.Node(SecretLabel.Kind.BOX, box.getMessage().getString()));
        } else if (node instanceof AbstractStringWidget text) {
            widgets.add(text);
            nodes.add(new SecretLabel.Node(SecretLabel.Kind.TEXT, text.getMessage().getString()));
        } else if (node instanceof ContainerEventHandler holder) {
            for (GuiEventListener child : holder.children()) {
                walk(child, widgets, nodes, depth + 1);
            }
        } else {
            widgets.add(node);
            nodes.add(new SecretLabel.Node(SecretLabel.Kind.OTHER, ""));
        }
    }

    /** Where the eye of {@code box} starts across. */
    private static int eyeX(AbstractWidget box) {
        return box.getX() + box.getWidth() - RIGHT - EyeIcon.W;
    }

    /** The point is on the eye of {@code box}: the eye with a little room around it, the box's height. */
    public static boolean onEye(AbstractWidget box, double x, double y) {
        return x >= eyeX(box) - 3 && x < box.getX() + box.getWidth() && y >= box.getY()
            && y < box.getY() + box.getHeight();
    }

    /**
     * Draws the eye of a password box: open while hidden, struck through while shown; brighter under the pointer, with
     * the pointing hand and a tooltip saying what a press does.
     */
    public static void drawEye(GuiGraphicsExtractor graphics, AbstractWidget box, boolean shown, int mouseX,
                               int mouseY) {
        TweakGuard at = guard;
        if (at == null) {
            return;
        }
        at.run(() -> {
            boolean hover = box.isActive() && onEye(box, mouseX, mouseY);
            int colour = !box.isActive() ? OFF : hover ? HOVER : REST;
            int left = eyeX(box);
            int top = box.getY() + (box.getHeight() - EyeIcon.H) / 2;
            for (EyeIcon.Run run : shown ? STRUCK : OPEN) {
                graphics.fill(left + run.x(), top + run.y(), left + run.x() + run.length(), top + run.y() + 1, colour);
            }
            if (hover) {
                graphics.requestCursor(CursorTypes.POINTING_HAND);
                graphics.setTooltipForNextFrame(Minecraft.getInstance().font, Component.translatable(
                    shown ? "projects_overlay.password.hide" : "projects_overlay.password.show"), mouseX, mouseY);
            }
        });
    }

    /** The eye was pressed: the game's button click, as any other press in a window. */
    public static void pressed() {
        TweakGuard at = guard;
        if (at != null) {
            at.run(() -> AbstractWidget.playButtonClickSound(Minecraft.getInstance().getSoundManager()));
        }
    }
}
