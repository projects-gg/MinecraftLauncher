package gg.projects.overlay.toast;

import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.Perf;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Sounds;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Shapes;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElement;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.ChatScreen;

/**
 * The toasts, drawn in the mod's own HUD layer at the bottom right (or top right): not the game's toast
 * system, so the stack, its timing and its motion are ours. Nothing runs while it is empty, apart from
 * one emptiness check per frame. Hidden with F1, so a private message never lands in a screenshot or a
 * video. Clears when the panel opens (the panel shows all of it), pauses its timers while the game window
 * is not in front. The travel toast ("Projects sunucusuna bağlanılıyor…") is the one exception to the HUD:
 * it stays through the saving, connecting and loading screens, drawn above them.
 */
public final class ToastLayer implements HudElement {
    /** Something drawn over the HUD for a short while (the panel fading out after it closed). */
    public interface Afterimage {
        /** Draws one frame; returns false once it is over. */
        boolean draw(GuiGraphicsExtractor g, long now);
    }

    static final int MARGIN = 8;
    private static final int ABOVE_HOTBAR = 60;
    private static final int ABOVE_CHAT_INPUT = 18;
    /** Room for five subtitle lines (the game draws them at the bottom right, 10 pixels each from 35 up). */
    private static final int ABOVE_SUBTITLES = 35 + 4 * 10 + 7;
    private static final int BELOW_EFFECTS = 52;
    private static final long MESSAGE_MS = 5_000;
    private static final long CALL_MS = 8_000;
    private static final long INFO_MS = 4_000;

    /**
     * Toast time stands still while the game window is not in front (the player would miss them). The
     * scenario driver turns this off: its test window never takes the focus.
     */
    public static volatile boolean pauseWhileAway = true;

    private final ToastStack<ToastCard> stack = new ToastStack<>();
    private final Spring anchor = Spring.snappy(0f);
    private boolean anchorPlaced;
    private long lastFrame;
    private Afterimage afterimage;
    private ToastCard travel;
    private final Tween travelAlpha = new Tween(0f);
    private final Tween travelDrop = new Tween(0f);

    public ToastStack<ToastCard> stack() {
        return stack;
    }

    public void setAfterimage(Afterimage next) {
        afterimage = next;
    }

    /** Whether a frame of this layer draws anything (the measurement driver uses it). */
    public boolean busy() {
        return !stack.isEmpty() || afterimage != null || travel != null;
    }

    /**
     * Shows a toast the launcher asked for, as the player's settings say.
     *
     * @param crewChat the message is from a crew conversation
     * @param panelConversation the conversation the open panel shows, or null when it is closed
     * @param panelOpen the panel is open (it shows everything itself; only the sound tells of others)
     */
    public void show(OverlayState.ToastRequest request, boolean crewChat, String panelConversation, boolean panelOpen,
        OverlaySettings settings, String keyName, boolean reducedMotion) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || !settings.showsKind(request.kind())) {
            return;
        }
        long now = now();
        boolean forOpenConversation = request.conversation() != null && request.conversation().equals(
            panelConversation);
        ToastCard.Kind kind = ToastCard.kindOf(request.kind());
        if (forOpenConversation) {
            return;
        }
        Sounds.Cue cue = kind == ToastCard.Kind.CALL || kind == ToastCard.Kind.INVITE ? Sounds.Cue.CALL
            : kind == ToastCard.Kind.MESSAGE ? Sounds.Cue.MESSAGE : null;
        if (cue != null) {
            Sounds.play(cue, settings, now);
        }
        // The panel shows it; on the way to a server the travel toast has the corner (the panel will have it).
        if (panelOpen || (travel != null && travelAlpha.target() > 0f)) {
            return;
        }
        Shapes.prepare();
        ToastStack.Entry<ToastCard> existing = find(request.key());
        int count = existing == null ? 1 : existing.count() + 1;
        ToastCard card = ToastCard.of(request, crewChat, count, settings.showMessageText(), keyName);
        stack.setAnchoredAtTop(settings.corner() == OverlaySettings.Corner.TOP_RIGHT);
        stack.setReducedMotion(reducedMotion);
        stack.push(request.key(), card, card.height(), duration(kind, settings, mc), now);
        mc.getNarrator().saySystemQueued(card.narration());
    }

    /**
     * Shows the travel toast: the other toasts make way, and it stays (above every screen) until
     * {@link #endTravel()}.
     */
    public void travel(ToastCard card, boolean reducedMotion) {
        long now = now();
        stack.clear(now);
        travel = card;
        travelAlpha.snap(0f);
        travelAlpha.animateTo(1f, now, ToastStack.FADE_IN_MS, Ease.OUT_EXPO);
        travelDrop.snap(reducedMotion ? 0f : -ToastStack.ENTER_OFFSET);
        travelDrop.animateTo(0f, now, ToastStack.FADE_IN_MS + 80, Ease.OUT_EXPO);
        Minecraft.getInstance().getNarrator().saySystemQueued(card.narration());
    }

    /** The game arrived, or gave up: the travel toast fades where it is. */
    public void endTravel() {
        if (travel != null && travelAlpha.target() > 0f) {
            travelAlpha.animateTo(0f, now(), ToastStack.FADE_OUT_MS, Ease.OUT);
        }
    }

    /** A short note from the overlay itself ("Zaten Projects sunucusundasın."), whatever the toast settings. */
    public void notice(ToastCard card, OverlaySettings settings, boolean reducedMotion) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return;
        }
        Shapes.prepare();
        stack.setAnchoredAtTop(settings.corner() == OverlaySettings.Corner.TOP_RIGHT);
        stack.setReducedMotion(reducedMotion);
        stack.push("overlay-notice", card, card.height(), duration(ToastCard.Kind.INFO, settings, mc), now());
        mc.getNarrator().saySystemQueued(card.narration());
    }

    /** The panel opened: every toast fades where it is. */
    public void clear() {
        stack.clear(now());
    }

    /** The world closed or the launcher went away. */
    public void reset() {
        stack.reset();
        anchorPlaced = false;
    }

    private ToastStack.Entry<ToastCard> find(String key) {
        for (ToastStack.Entry<ToastCard> e : stack.entries()) {
            if (!e.leaving() && e.key.equals(key)) {
                return e;
            }
        }
        return null;
    }

    private static long duration(ToastCard.Kind kind, OverlaySettings settings, Minecraft mc) {
        long base = switch (kind) {
            case CALL, INVITE -> CALL_MS;
            case INFO, TRAVEL -> INFO_MS;
            case MESSAGE -> MESSAGE_MS;
        };
        double accessibility = mc.options.notificationDisplayTime().get();
        return Math.round(base * settings.duration().factor * accessibility);
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, DeltaTracker delta) {
        if (stack.isEmpty() && afterimage == null && travel == null) {
            return;
        }
        long started = Perf.start();
        // A layer of our own: placing each element then scans only the overlay's elements, not the whole HUD.
        Draw.stratum(g);
        Minecraft mc = Minecraft.getInstance();
        long now = now();
        if (afterimage != null && !afterimage.draw(g, now)) {
            afterimage = null;
        }
        // Under a screen the travel toast is drawn above it instead (extractOverScreen).
        if (travel != null && Compat.screen(mc) == null && !Compat.hudHidden(mc)) {
            drawTravel(g, mc, now, false);
        }
        if (stack.isEmpty()) {
            Perf.toast(started);
            return;
        }
        OverlaySettings settings = gg.projects.overlay.OverlayClient.get().settings();
        boolean reduced = gg.projects.overlay.OverlayClient.get().reducedMotion();
        boolean top = settings.corner() == OverlaySettings.Corner.TOP_RIGHT;
        stack.setAnchoredAtTop(top);
        stack.update(now, pauseWhileAway && !mc.isWindowActive(), reduced);
        if (stack.isEmpty() || Compat.hudHidden(mc)) {
            Perf.toast(started);
            return;
        }
        Shapes.prepare();
        int width = g.guiWidth();
        int height = g.guiHeight();
        float x = width - MARGIN - ToastCard.WIDTH;
        float target = top ? topEdge(mc) : bottomEdge(mc, width, height, x);
        if (!anchorPlaced || reduced) {
            anchor.snap(target);
            anchorPlaced = true;
        } else {
            anchor.setTarget(target);
            anchor.step(Math.min(0.05f, Math.max(0L, now - lastFrame) / 1000f));
        }
        lastFrame = now;
        float base = anchor.value();
        for (int i = stack.entries().size() - 1; i >= 0; i--) {
            ToastStack.Entry<ToastCard> e = stack.entries().get(i);
            float a = e.alpha(now);
            if (a <= 0.004f) {
                continue;
            }
            Draw.pushAlpha(a);
            e.content().draw(g, x, base + e.y(), e.refresh(now));
            Draw.popAlpha();
        }
        Perf.toast(started);
    }

    /** Draws the travel toast above whatever screen is open (saving, connecting, loading, the pause menu). */
    public void extractOverScreen(GuiGraphicsExtractor g) {
        if (travel == null) {
            return;
        }
        long started = Perf.start();
        Draw.stratum(g);
        drawTravel(g, Minecraft.getInstance(), now(), true);
        Perf.toast(started);
    }

    private void drawTravel(GuiGraphicsExtractor g, Minecraft mc, long now, boolean overScreen) {
        if (travelAlpha.target() <= 0f && travelAlpha.done(now)) {
            travel = null;
            return;
        }
        Shapes.prepare();
        int width = g.guiWidth();
        int height = g.guiHeight();
        float x = width - MARGIN - ToastCard.WIDTH;
        OverlaySettings settings = gg.projects.overlay.OverlayClient.get().settings();
        float y;
        if (settings.corner() == OverlaySettings.Corner.TOP_RIGHT) {
            y = overScreen ? MARGIN : topEdge(mc);
        } else {
            y = (overScreen ? height - MARGIN : bottomEdge(mc, width, height, x)) - travel.height();
        }
        Draw.pushAlpha(travelAlpha.value(now));
        travel.draw(g, x, y + travelDrop.value(now), 1f);
        Draw.popAlpha();
    }

    /** Where the stack's bottom edge goes: above the hotbar when they would meet, above subtitles and chat. */
    private static float bottomEdge(Minecraft mc, int width, int height, float x) {
        float bottom = height - MARGIN;
        // The hotbar is 182 wide in the middle; the off-hand slot or the attack indicator sit beside it.
        int hotbarRight = width / 2 + 91 + 29 + 6;
        if (x < hotbarRight) {
            bottom = Math.min(bottom, height - ABOVE_HOTBAR);
        }
        if (mc.options.showSubtitles().get()) {
            bottom = Math.min(bottom, height - ABOVE_SUBTITLES);
        }
        if (Compat.screen(mc) instanceof ChatScreen) {
            bottom = Math.min(bottom, height - ABOVE_CHAT_INPUT);
        }
        return bottom;
    }

    /** Where the stack's top edge goes in the top corner: below the status effect icons when there are any. */
    private static float topEdge(Minecraft mc) {
        boolean effects = mc.player != null && !mc.player.getActiveEffects().isEmpty();
        return MARGIN + (effects ? BELOW_EFFECTS : 0);
    }

    static long now() {
        return System.nanoTime() / 1_000_000L;
    }
}
