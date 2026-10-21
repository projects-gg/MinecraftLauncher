package gg.projects.overlay.ui;

import gg.projects.overlay.Perf;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.toasts.Toast;
import net.minecraft.client.gui.components.toasts.ToastManager;
import net.minecraft.network.chat.Component;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundEvents;

/**
 * A corner notification drawn by the game's own toast system: it never takes input or focus, follows the
 * game's "Notification Time" accessibility option and stacks with vanilla toasts. Toasts with the same key
 * (one conversation, one call) update in place instead of piling up.
 */
public final class OverlayToast implements Toast {
    public enum Kind { MESSAGE, CALL, INFO }

    private static final int WIDTH = 160;
    private static final int HEIGHT = 32;
    private static final int TEXT_X = 10;
    private static final String ELLIPSIS = "…";

    private final Kind kind;
    private final String key;
    private final Component hint;
    private final boolean reducedMotion;
    private final long durationMs;

    private String title;
    private String titleLine;
    private String bodyLine;
    private int count = 1;
    private boolean changed;
    private long lastChanged;
    private Visibility wanted = Visibility.SHOW;

    private OverlayToast(Kind kind, String key, String title, String body, Component hint, boolean reducedMotion) {
        this.kind = kind;
        this.key = key;
        this.hint = hint;
        this.reducedMotion = reducedMotion;
        this.durationMs = switch (kind) {
            case CALL -> 8_000;
            case MESSAGE -> 5_000;
            case INFO -> 4_000;
        };
        layout(title, body);
    }

    /** Shows a toast, or refreshes the one already showing for the same key. Render thread only. */
    public static void show(Minecraft mc, Kind kind, String key, String title, String body, Component hint,
        boolean reducedMotion) {
        ToastManager manager = mc.getToastManager();
        OverlayToast existing = manager.getToast(OverlayToast.class, key);
        if (existing != null) {
            existing.count++;
            existing.layout(title, body);
            existing.changed = true;
        } else {
            manager.addToast(new OverlayToast(kind, key, title, body, hint, reducedMotion));
        }
        mc.getNarrator().saySystemQueued(Component.literal(body.isEmpty() ? title : title + ": " + body));
    }

    private void layout(String title, String body) {
        Font font = Minecraft.getInstance().font;
        this.title = title;
        String shownTitle = count > 1 ? title + " (" + count + ")" : title;
        int hintWidth = hint == null ? 0 : font.width(hint) + 6;
        this.titleLine = fit(font, shownTitle, WIDTH - TEXT_X - 6 - hintWidth);
        this.bodyLine = fit(font, body, WIDTH - TEXT_X - 6);
    }

    private static String fit(Font font, String text, int maxWidth) {
        if (font.width(text) <= maxWidth) {
            return text;
        }
        return font.plainSubstrByWidth(text, Math.max(0, maxWidth - font.width(ELLIPSIS))).stripTrailing() + ELLIPSIS;
    }

    @Override
    public Visibility getWantedVisibility() {
        return wanted;
    }

    @Override
    public void update(ToastManager manager, long elapsed) {
        if (changed) {
            lastChanged = elapsed;
            changed = false;
        }
        double shownFor = durationMs * manager.getNotificationDisplayTimeMultiplier();
        wanted = elapsed - lastChanged < shownFor ? Visibility.SHOW : Visibility.HIDE;
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, Font font, long elapsed) {
        long started = Perf.start();
        // F1 hides the HUD for screenshots and videos; private messages should not end up in them.
        if (!Minecraft.getInstance().options.hideGui) {
            g.fill(0, 0, WIDTH, HEIGHT, Theme.TOAST);
            g.fill(0, 0, 2, HEIGHT, kind == Kind.INFO ? Theme.TEXT_MUTED : Theme.GOLD);
            g.text(font, titleLine, TEXT_X, 7, Theme.TEXT, false);
            if (hint != null) {
                g.text(font, hint, WIDTH - 6 - font.width(hint), 7, Theme.TEXT_MUTED, false);
            }
            g.text(font, bodyLine, TEXT_X, 19, Theme.TEXT_DIM, false);
        }
        Perf.toast(started);
    }

    @Override
    public Object getToken() {
        return key;
    }

    @Override
    public SoundEvent getSoundEvent() {
        return kind == Kind.INFO ? null : SoundEvents.UI_TOAST_IN;
    }

    @Override
    public float xPos(int screenWidth, float visiblePortion) {
        // With reduced motion the toast appears in place instead of sliding in.
        return reducedMotion ? screenWidth - width() : Toast.super.xPos(screenWidth, visiblePortion);
    }

    @Override
    public int width() {
        return WIDTH;
    }

    @Override
    public int height() {
        return HEIGHT;
    }

    String title() {
        return title;
    }
}
