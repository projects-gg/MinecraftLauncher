package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.sounds.SoundManager;
import net.minecraft.network.chat.Component;

/**
 * The message field: the game's text box (typing, selection, clipboard, word jumps, mouse placement,
 * input methods), drawn our way in the panel's font: a thin gold caret, a gold-tinted selection and the
 * placeholder in ash. The box around it and the send button belong to the panel. The caret stays lit while
 * the player types and blinks only when they pause. What is drawn is worked out again only when the text,
 * the caret or the selection moves. If a later game version renames the box's private fields, the game's
 * own drawing takes over.
 */
public final class ComposerField extends EditBox {
    private static final VarHandle DISPLAY_POS = field("displayPos");
    private static final VarHandle HIGHLIGHT_POS = field("highlightPos");
    private static final long BLINK_MS = 530;

    private String placeholderText = "";
    private boolean editable = true;
    private long lastEdit;
    private String seenValue;
    private int seenCursor = -1;
    private int seenDisplay = -1;
    private int seenHighlight = -1;
    private int seenWidth = -1;
    private String shown = "";
    private float caretX;
    private float selectFrom;
    private float selectTo;

    public ComposerField(Component name) {
        super(Fonts.ui(), 0, 0, 100, 12, name);
        setBordered(false);
        setTextShadow(false);
        setTextColor(Theme.BONE_50);
        setTextColorUneditable(Theme.ASH_400);
    }

    @Override
    public void setEditable(boolean value) {
        editable = value;
        super.setEditable(value);
    }

    public void setPlaceholder(Component text) {
        placeholderText = text.getString();
        setHint(text);
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        PanelInput.usedKeyboard();
        return super.keyPressed(event);
    }

    @Override
    public boolean charTyped(CharacterEvent event) {
        PanelInput.usedKeyboard();
        return super.charTyped(event);
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        super.onClick(event, doubleClick);
    }

    @Override
    public void playDownSound(SoundManager manager) {
        // Quiet, like the rest of the panel.
    }

    @Override
    public void extractWidgetRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        if (DISPLAY_POS == null || HIGHLIGHT_POS == null) {
            super.extractWidgetRenderState(g, mouseX, mouseY, partialTick);
            return;
        }
        if (!isVisible()) {
            return;
        }
        long now = System.nanoTime() / 1_000_000L;
        Font font = Fonts.ui();
        measure(font, now);
        int x = getX();
        float textY = Fonts.centeredY(Fonts.Face.UI, getY() + getHeight() / 2f);
        if (seenValue.isEmpty()) {
            if (!placeholderText.isEmpty()) {
                Draw.text(g, font, placeholderText, x, textY, Theme.ASH_400);
            }
        } else {
            if (selectTo > selectFrom && isFocused()) {
                Draw.fill(g, x + selectFrom, getY() + 1, x + selectTo, getY() + getHeight() - 1,
                    Theme.withAlpha(Theme.GOLD_500, 0.42f));
            }
            Draw.text(g, font, shown, x, textY, editable ? Theme.BONE_50 : Theme.ASH_400);
        }
        if (isFocused() && editable) {
            long idle = now - lastEdit;
            if (idle < BLINK_MS || (idle / BLINK_MS) % 2 == 1) {
                int k = Draw.scale();
                float width = Math.max(1, k / 2) / (float) k;
                float top = getY() + getHeight() / 2f - 5f;
                Draw.fill(g, x + caretX, top, x + caretX + width, top + 10f, Theme.GOLD_300);
            }
        }
        if (isHovered()) {
            g.requestCursor(editable ? CursorTypes.IBEAM : CursorTypes.NOT_ALLOWED);
        }
    }

    /** Works out the visible text, the caret and the selection when something moved. */
    private void measure(Font font, long now) {
        String value = getValue();
        int cursor = getCursorPosition();
        int display = Math.max(0, Math.min(value.length(), (int) DISPLAY_POS.get(this)));
        int highlight = Math.max(0, Math.min(value.length(), (int) HIGHLIGHT_POS.get(this)));
        int width = getInnerWidth();
        if (value.equals(seenValue) && cursor == seenCursor && display == seenDisplay && highlight == seenHighlight
            && width == seenWidth) {
            return;
        }
        if (!value.equals(seenValue) || cursor != seenCursor) {
            lastEdit = now;
        }
        seenValue = value;
        seenCursor = cursor;
        seenDisplay = display;
        seenHighlight = highlight;
        seenWidth = width;
        shown = font.plainSubstrByWidth(value.substring(display), width);
        int c = Math.max(display, Math.min(value.length(), cursor));
        caretX = font.width(value.substring(display, c));
        int a = Math.max(display, Math.min(cursor, highlight));
        int b = Math.min(display + shown.length(), Math.max(cursor, highlight));
        if (b > a && highlight != cursor) {
            selectFrom = font.width(value.substring(display, a));
            selectTo = font.width(value.substring(display, b));
        } else {
            selectFrom = 0f;
            selectTo = 0f;
        }
    }

    private static VarHandle field(String name) {
        try {
            return MethodHandles.privateLookupIn(EditBox.class, MethodHandles.lookup())
                .findVarHandle(EditBox.class, name, int.class);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return null;
        }
    }
}
