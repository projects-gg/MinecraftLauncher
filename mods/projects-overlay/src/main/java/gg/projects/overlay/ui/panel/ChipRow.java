package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The conversations as a row of chips (any number: the row scrolls sideways with the wheel or the arrow
 * keys and fades at the ends that have more). A chip shows the name, the crew sign for crew chat and the
 * unread count in gold. The order is kept while the panel is open, so chips never jump under the pointer;
 * a new conversation joins at the end.
 */
public final class ChipRow extends PanelWidget {
    public static final int HEIGHT = 14;
    private static final int GAP = 4;
    private static final int PAD = 6;

    private record Chip(String id, String title, int titleWidth, boolean crew, boolean muted, String unread,
        int unreadWidth, int width) {
    }

    private final Consumer<String> onPick;
    private final List<String> order = new ArrayList<>();
    private final List<Chip> chips = new ArrayList<>();
    private final Spring scroll = Spring.snappy(0f);
    private String selected;
    private float contentWidth;
    private long lastFrame;
    private boolean reducedMotion;

    public ChipRow(Consumer<String> onPick) {
        super(0, 0, 100, HEIGHT, Texts.tr("chats"));
        this.onPick = onPick;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    public String selected() {
        return selected;
    }

    public boolean isEmpty() {
        return chips.isEmpty();
    }

    /** Rebuilds the chips from the state, keeping the order the panel opened with. */
    public void update(OverlayState state, String select) {
        for (OverlayState.Conversation c : state.conversations()) {
            if (!order.contains(c.id())) {
                order.add(c.id());
            }
        }
        order.removeIf(id -> state.conversation(id) == null);
        chips.clear();
        Font strong = Fonts.ui();
        Font small = Fonts.small();
        float x = 0f;
        for (String id : order) {
            OverlayState.Conversation c = state.conversation(id);
            String title = Texts.fit(strong, c.title().isEmpty() ? Texts.str("toast.someone") : c.title(), 96);
            int tw = strong.width(title);
            boolean showUnread = c.unread() > 0 && !id.equals(select);
            String unread = showUnread ? (c.unread() > 99 ? "99+" : Integer.toString(c.unread())) : null;
            int uw = unread == null ? 0 : small.width(unread) + 7;
            int w = PAD * 2 + tw + (c.crew() ? 11 : 0) + (unread == null ? 0 : uw + 4);
            chips.add(new Chip(id, title, tw, c.crew(), c.muted(), unread, uw, w));
            x += w + GAP;
        }
        contentWidth = Math.max(0f, x - GAP);
        selected = select;
        reveal(select, false);
    }

    private float maxScroll() {
        return Math.max(0f, contentWidth - getWidth());
    }

    /** Scrolls so the chip {@code id} is in view. */
    private void reveal(String id, boolean animate) {
        float x = 0f;
        for (Chip c : chips) {
            if (c.id.equals(id)) {
                float target = scroll.target();
                if (x < target) {
                    target = x - 8;
                } else if (x + c.width > target + getWidth()) {
                    target = x + c.width - getWidth() + 8;
                }
                target = Math.max(0f, Math.min(maxScroll(), target));
                if (animate && !reducedMotion) {
                    scroll.setTarget(target);
                } else {
                    scroll.snap(target);
                }
                return;
            }
            x += c.width + GAP;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), scroll.target())));
    }

    private void pick(String id) {
        if (id == null || id.equals(selected)) {
            return;
        }
        selected = id;
        reveal(id, true);
        onPick.accept(id);
    }

    private int indexOf(String id) {
        for (int i = 0; i < chips.size(); i++) {
            if (chips.get(i).id.equals(id)) {
                return i;
            }
        }
        return -1;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        float local = (float) event.x() - getX() + scroll.value();
        float x = 0f;
        for (Chip c : chips) {
            if (local >= x && local < x + c.width) {
                pick(c.id);
                return;
            }
            x += c.width + GAP;
        }
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY) || maxScroll() <= 0f) {
            return false;
        }
        float delta = (float) (scrollX != 0 ? -scrollX : -scrollY) * 30f;
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() + delta));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
        return true;
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        int i = indexOf(selected);
        if (event.isLeft() && i > 0) {
            PanelInput.usedKeyboard();
            pick(chips.get(i - 1).id);
            return true;
        }
        if (event.isRight() && i >= 0 && i < chips.size() - 1) {
            PanelInput.usedKeyboard();
            pick(chips.get(i + 1).id);
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        g.enableScissor(x0 - 2, y0 - 3, x0 + w + 2, y0 + HEIGHT + 3);
        Font strong = Fonts.ui();
        Font small = Fonts.small();
        float x = x0 - scroll.value();
        float textY = Fonts.centeredY(Fonts.Face.UI, y0 + HEIGHT / 2f);
        for (Chip c : chips) {
            if (x + c.width >= x0 - 2 && x <= x0 + w + 2) {
                boolean isSelected = c.id.equals(selected);
                boolean lit = isHovered() && mouseX >= x && mouseX < x + c.width;
                Frames frame = isSelected ? Frames.CHIP_SELECTED : lit ? Frames.CHIP_HOVER : Frames.CHIP;
                Draw.frame(g, frame, x, y0, c.width, HEIGHT);
                if (isSelected && keyboardFocus()) {
                    Draw.frame(g, Frames.FOCUS_14, x, y0, c.width, HEIGHT);
                }
                float cx = x + PAD;
                int ink = isSelected ? Theme.BONE_50 : c.muted ? Theme.ASH_400 : lit ? Theme.BONE_100 : Theme.ASH_300;
                if (c.crew) {
                    Draw.icon(g, Icon.CREW, 8, cx, y0 + (HEIGHT - 8) / 2f, ink);
                    cx += 11;
                }
                Draw.text(g, strong, c.title, cx, textY, ink);
                cx += c.titleWidth + 4;
                if (c.unread != null) {
                    float py = y0 + (HEIGHT - 9) / 2f;
                    Draw.frame(g, Frames.PILL_GOLD, cx, py, c.unreadWidth, 9);
                    Draw.text(g, small, c.unread, cx + 3.5f, Fonts.centeredY(Fonts.Face.SMALL, py + 4.5f),
                        Theme.GOLD_INK);
                }
            }
            x += c.width + GAP;
        }
        g.disableScissor();
        if (scroll.value() > 0.5f) {
            Draw.strip(g, ShapeSet.FADE_LEFT, x0 - 2, y0 - 2, ShapeSet.FADE_WIDTH, HEIGHT + 4);
        }
        if (scroll.value() < maxScroll() - 0.5f) {
            Draw.strip(g, ShapeSet.FADE_RIGHT, x0 + w + 2 - ShapeSet.FADE_WIDTH, y0 - 2, ShapeSet.FADE_WIDTH,
                HEIGHT + 4);
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        int i = indexOf(selected);
        if (i >= 0) {
            Chip c = chips.get(i);
            output.add(NarratedElementType.TITLE, Texts.tr("narration.chat", Component.literal(c.title), i + 1,
                chips.size()));
        } else {
            output.add(NarratedElementType.TITLE, getMessage());
        }
        output.add(NarratedElementType.USAGE, Texts.tr("narration.chat.usage"));
    }
}
