package gg.projects.overlay.toast;

import gg.projects.overlay.chat.ChatEmojiBar;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.Perf;
import gg.projects.overlay.keys.KeyHint;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.MarkTip;
import gg.projects.overlay.ui.Marks;
import gg.projects.overlay.ui.Sounds;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Shapes;
import java.util.List;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElement;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.TitleScreen;

/**
 * The toasts, drawn in the mod's own HUD layer at the bottom right (or top right): not the game's toast
 * system, so the stack, its timing and its motion are ours. Nothing runs while it is empty, apart from
 * one emptiness check per frame. Hidden with F1, so a private message never lands in a screenshot or a
 * video. Clears when the panel opens (the panel shows all of it), pauses its timers while the game window
 * is not in front. The travel toast ("Projects sunucusuna bağlanılıyor…") is the one exception to the HUD:
 * it stays through the saving, connecting and loading screens, drawn above them. A voice chat ring (VOICE-O) stands in
 * the stack for as long as it rings, never waiting for the others; with no world open (the title screen, the server
 * list) it is drawn above the screen, the HUD's own toasts being a world's.
 *
 * <p>In the chat the pointer is free, and a toast is pressed there like a button (F7-OPEN: players clicked the card's
 * key hint): under the pointer its edge lights gold over 120 ms and every toast's time stands still, and a press opens
 * the panel ({@link #cardAt}). The emoji picker, when it is open over a toast, keeps the pointer. The pointer resting on
 * the profile badge right of a title's name names every badge of that person (BADGES-UI, {@link MarkTip}).
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
    /** The chat's emoji button (CHAT-EMOJI) and the card above it. */
    private static final int ABOVE_EMOJI_BUTTON = 4;
    /** Room for five subtitle lines (the game draws them at the bottom right, 10 pixels each from 35 up). */
    private static final int ABOVE_SUBTITLES = 35 + 4 * 10 + 7;
    private static final int BELOW_EFFECTS = 52;
    /**
     * Minimaps drawn in the top right corner by default: Xaero's (the official pack's) and its fair-play build. The stack
     * keeps below what {@link #MINIMAP_SHARE} of the screen's height takes there.
     */
    private static final List<String> MINIMAP_MODS = List.of("xaerominimap", "xaerominimapfair");
    static final float MINIMAP_SHARE = 0.36f;
    private static Boolean minimap;
    /** The title screen's copyright line (10 GUI pixels from the bottom) plus the card's usual margin. */
    private static final int ABOVE_TITLE_COPYRIGHT = 10;
    private static final long MESSAGE_MS = 5_000;
    private static final long CALL_MS = 8_000;
    private static final long INFO_MS = 4_000;
    /** A toast under the pointer lights over this long, and goes dark again as fast (opacity only). */
    private static final long HOVER_MS = 120;
    /** A press finds the toasts where the last frame drew them, if it drew them this recently (a slow frame too). */
    private static final long DRAWN_RECENTLY_MS = 1_000;
    /** A toast takes a press once it is this visible (one fading in or out does not). */
    private static final float PRESSABLE_ALPHA = 0.5f;

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
    /** When a note in the travel toast's place ({@link #noteOverScreens}) fades by itself; 0: never. */
    private long travelEndsAt;
    // Where the last frame drew the stack (the left edge, the anchor's line) and when: a press in the chat finds its
    // toast there.
    private float drawnX;
    private float drawnBase;
    private long drawnAt = Long.MIN_VALUE / 2;
    /** The toast under the pointer in the chat (its key), or the one going dark; null: none. */
    private String hoverKey;
    private final Tween hover = new Tween(0f);
    /** The toast under the pointer this frame and where the pointer is (GUI pixels), for its badge's tooltip. */
    private ToastStack.Entry<ToastCard> pointed;
    private double pointerX;
    private double pointerY;
    /** BADGES-UI: the tooltip of the badge right of a title's name. */
    private final MarkTip markTip = new MarkTip();
    /** A point the scenario driver stands the pointer at (GUI pixels), or null: the real pointer. */
    private float[] driverPointer;

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
     * @param line what its key hint says (KEY-HINT)
     * @return the card now on screen (new or updated in place), or null when none went up
     */
    public ToastCard show(OverlayState.ToastRequest request, boolean crewChat, String panelConversation,
        boolean panelOpen, OverlaySettings settings, String keyName, KeyHint.Line line, boolean reducedMotion) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || !settings.showsKind(request.kind())) {
            return null;
        }
        long now = now();
        boolean forOpenConversation = request.conversation() != null && request.conversation().equals(
            panelConversation);
        ToastCard.Kind kind = ToastCard.kindOf(request.kind());
        if (forOpenConversation) {
            return null;
        }
        Sounds.Cue cue = kind == ToastCard.Kind.CALL || kind == ToastCard.Kind.INVITE || kind == ToastCard.Kind.WORLD
            ? Sounds.Cue.CALL : kind == ToastCard.Kind.MESSAGE ? Sounds.Cue.MESSAGE
            // OVL-SOC2: what waits for the player's answer (a request, a crew invite or request, a plan, a missed call)
            // says it with the message sound; a friend coming online and "Ekibe katıldın" quietly.
            : kind == ToastCard.Kind.SOCIAL && !quietCard(request.kind()) ? Sounds.Cue.MESSAGE : null;
        if (cue != null) {
            Sounds.play(cue, settings, now);
        }
        // The panel shows it; on the way to a server the travel toast has the corner (the panel will have it).
        if (panelOpen || (travel != null && travelAlpha.target() > 0f)) {
            return null;
        }
        Shapes.prepare();
        ToastStack.Entry<ToastCard> existing = find(request.key());
        int count = existing == null ? 1 : existing.count() + 1;
        ToastCard card = ToastCard.of(request, crewChat, count, settings.showMessageText(), keyName, line);
        placeStack(mc, settings);
        stack.setReducedMotion(reducedMotion);
        stack.push(request.key(), card, card.height(), duration(kind, settings, mc), now);
        mc.getNarrator().saySystemQueued(card.narration());
        return card;
    }

    /**
     * Shows the travel toast: the other toasts make way, and it stays (above every screen) until
     * {@link #endTravel()}.
     */
    public void travel(ToastCard card, boolean reducedMotion) {
        long now = now();
        stack.clear(now);
        travel = card;
        travelEndsAt = 0;
        travelAlpha.snap(0f);
        travelAlpha.animateTo(1f, now, ToastStack.FADE_IN_MS, Ease.OUT_EXPO);
        travelDrop.snap(reducedMotion ? 0f : -ToastStack.ENTER_OFFSET);
        travelDrop.animateTo(0f, now, ToastStack.FADE_IN_MS + 80, Ease.OUT_EXPO);
        Minecraft.getInstance().getNarrator().saySystemQueued(card.narration());
    }

    /**
     * A short note above whatever screen is open, the title screen included (the HUD's toasts draw only in a world):
     * the travel toast's place, fading by itself after {@code ms} (OVL-FIX: the "Devam et" world is gone).
     */
    public void noteOverScreens(ToastCard card, long ms, boolean reducedMotion) {
        travel(card, reducedMotion);
        travelEndsAt = now() + ms;
    }

    /** The game arrived, or gave up: the travel toast fades where it is. */
    public void endTravel() {
        if (travel != null && travelAlpha.target() > 0f) {
            travelAlpha.animateTo(0f, now(), ToastStack.FADE_OUT_MS, Ease.OUT);
        }
    }

    /**
     * A voice chat ring's card (VOICE-O), for {@code ms} at most or until {@link #endRing}: pinned in the stack, never
     * waiting for the others nor pushed out by them; the same {@code key} again updates it in place. Above any screen
     * while no world is open. Returns whether it went up (not on the way to a server: the travel toast has the corner).
     */
    public boolean ring(ToastCard card, String key, long ms, OverlaySettings settings, boolean reducedMotion) {
        if (ms <= 0 || (travel != null && travelAlpha.target() > 0f)) {
            return false;
        }
        Shapes.prepare();
        boolean fresh = find(key) == null;
        placeStack(Minecraft.getInstance(), settings);
        stack.setReducedMotion(reducedMotion);
        stack.pin(key, card, card.height(), ms, now());
        if (fresh) {
            Minecraft.getInstance().getNarrator().saySystemQueued(card.narration());
        }
        return true;
    }

    /** A ring's card may go up now (not while the travel toast has the corner). */
    public boolean ringable() {
        return travel == null || travelAlpha.target() <= 0f;
    }

    /** The ring is over (answered, declined, taken elsewhere, its time up): its card fades out. */
    public void endRing(String key) {
        stack.dismiss(key, now());
    }

    /** A short note from the overlay itself ("Zaten Projects sunucusundasın."), whatever the toast settings. */
    public void notice(ToastCard card, OverlaySettings settings, boolean reducedMotion) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return;
        }
        Shapes.prepare();
        placeStack(mc, settings);
        stack.setReducedMotion(reducedMotion);
        stack.push("overlay-notice", card, card.height(), duration(ToastCard.Kind.INFO, settings, mc), now());
        mc.getNarrator().saySystemQueued(card.narration());
    }

    /**
     * A voice chat note (VOICE-O): as {@link #notice}, and above the screen while no world is open (voice chat goes on
     * from the title screen); not while the travel toast has the corner.
     */
    public void voiceNote(ToastCard card, OverlaySettings settings, boolean reducedMotion) {
        if (travel != null && travelAlpha.target() > 0f) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        Shapes.prepare();
        placeStack(mc, settings);
        stack.setReducedMotion(reducedMotion);
        stack.push("overlay-voice-note", card, card.height(), duration(ToastCard.Kind.INFO, settings, mc), now());
        mc.getNarrator().saySystemQueued(card.narration());
    }

    /**
     * A note of world sharing for the host (HOST1: the world opened, a friend came in or left), under the player's
     * "Diğer bildirimler". Notes about different friends stack; the same one replaces itself. Returns whether it went
     * up.
     */
    public boolean host(ToastCard card, String key, OverlaySettings settings, boolean reducedMotion) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || !settings.infoToasts() || (travel != null && travelAlpha.target() > 0f)) {
            return false;
        }
        Shapes.prepare();
        placeStack(mc, settings);
        stack.setReducedMotion(reducedMotion);
        stack.push(key, card, card.height(), duration(ToastCard.Kind.HOST, settings, mc), now());
        mc.getNarrator().saySystemQueued(card.narration());
        return true;
    }

    // ---- A toast pressed in the chat (F7-OPEN) ------------------------------------------------------------------

    /**
     * The toast at this point of the chat screen (GUI pixels) as the last frame drew it, or null: none there, the
     * pointer is not free (no chat), or the emoji picker is open over the point.
     */
    public ToastCard cardAt(double x, double y) {
        Minecraft mc = Minecraft.getInstance();
        long now = now();
        if (now - drawnAt > DRAWN_RECENTLY_MS || !pointerFree(mc, x, y)) {
            return null;
        }
        ToastStack.Entry<ToastCard> e = entryAt(x, y, now);
        return e == null ? null : e.content();
    }

    /** Whether the pointer at this point of the chat is on a toast (it shows the pointing hand there). */
    public boolean pointing(double x, double y) {
        return cardAt(x, y) != null;
    }

    /** Stands the pointer at a point for the hover as the player's would (the scenario driver; null: the real one). */
    public void pointForDriver(float[] at) {
        driverPointer = at;
    }

    /** Where the toast of {@code conversation} is ({x, y, w, h}, GUI pixels) as last drawn, or null (the driver). */
    public float[] boxForDriver(String conversation) {
        for (ToastStack.Entry<ToastCard> e : stack.entries()) {
            if (!e.leaving() && conversation.equals(e.content().conversation())) {
                return new float[] {drawnX, drawnBase + e.y(), ToastCard.WIDTH, e.content().height()};
            }
        }
        return null;
    }

    private static boolean pointerFree(Minecraft mc, double x, double y) {
        return Compat.screen(mc) instanceof ChatScreen chat && !ChatEmojiBar.pickerOver(chat, x, y);
    }

    private ToastStack.Entry<ToastCard> entryAt(double x, double y, long now) {
        if (x < drawnX || x >= drawnX + ToastCard.WIDTH) {
            return null;
        }
        for (ToastStack.Entry<ToastCard> e : stack.entries()) {
            if (e.leaving() || e.alpha(now) < PRESSABLE_ALPHA) {
                continue;
            }
            float top = drawnBase + e.y();
            if (y >= top && y < top + e.content().height()) {
                return e;
            }
        }
        return null;
    }

    /** The toast the pointer is on in the chat this frame lights; the one it left goes dark. */
    private void trackHover(Minecraft mc, long now) {
        String under = null;
        pointed = null;
        if (Compat.screen(mc) instanceof ChatScreen) {
            double x;
            double y;
            if (driverPointer != null) {
                x = driverPointer[0];
                y = driverPointer[1];
            } else {
                x = mc.mouseHandler.getScaledXPos(mc.getWindow());
                y = mc.mouseHandler.getScaledYPos(mc.getWindow());
            }
            ToastStack.Entry<ToastCard> e = pointerFree(mc, x, y) ? entryAt(x, y, now) : null;
            under = e == null ? null : e.key;
            pointed = e;
            pointerX = x;
            pointerY = y;
        }
        if (under != null && !under.equals(hoverKey)) {
            hoverKey = under;
            hover.snap(0f);
            hover.animateTo(1f, now, HOVER_MS, Ease.OUT);
        } else if (under == null && hoverKey != null && hover.target() > 0f) {
            hover.animateTo(0f, now, HOVER_MS, Ease.OUT);
        }
        if (hoverKey != null && hover.target() <= 0f && hover.done(now)) {
            hoverKey = null;
        }
    }

    /**
     * BADGES-UI: the badge right of a title's name under the pointer (in the chat) names every badge of that person;
     * {@code x} and {@code base} are where this frame draws the stack.
     */
    private void trackMark(long now, float x, float base) {
        ToastStack.Entry<ToastCard> e = pointed;
        if (e != null && !e.content().marks().isEmpty()) {
            ToastCard card = e.content();
            float mx = Marks.left(x + card.nameEnd());
            float my = Marks.top(base + e.y() + ToastCard.titleMid());
            if (Marks.over(pointerX, pointerY, mx, my)) {
                markTip.track(e.key, card.marks(), mx, my, now);
                return;
            }
        }
        markTip.track(null, null, 0f, 0f, now);
    }

    /**
     * KEY-HINT: a card with the overlay key's hint is on screen (the last frames drew it): only then does the hint learn
     * from the keys the game's window hears.
     */
    public boolean keyHintShowing() {
        if (now() - drawnAt > DRAWN_RECENTLY_MS) {
            return false;
        }
        for (ToastStack.Entry<ToastCard> e : stack.entries()) {
            if (!e.leaving() && e.content().keyHint()) {
                return true;
            }
        }
        return false;
    }

    /** KEY-HINT: every card's key hint says {@code line} now, where it stands (one line for another: no card moves). */
    public void rehint(KeyHint.Line line) {
        stack.replaceAll(card -> card.rehinted(line));
    }

    /** The panel opened: every toast fades where it is. */
    public void clear() {
        stack.clear(now());
        hoverKey = null;
        markTip.track(null, null, 0f, 0f, now());
    }

    /** The world closed or the launcher went away. */
    public void reset() {
        stack.reset();
        anchorPlaced = false;
        hoverKey = null;
        markTip.track(null, null, 0f, 0f, now());
    }

    private ToastStack.Entry<ToastCard> find(String key) {
        for (ToastStack.Entry<ToastCard> e : stack.entries()) {
            if (!e.leaving() && e.key.equals(key)) {
                return e;
            }
        }
        return null;
    }

    /** OVL-SOC2: a social card that comes without a sound (news only, nothing to answer). */
    static boolean quietCard(String kind) {
        return switch (kind) {
            case "online", "projects", "friend", "crew-in" -> true;
            default -> false;
        };
    }

    private static long duration(ToastCard.Kind kind, OverlaySettings settings, Minecraft mc) {
        long base = switch (kind) {
            case CALL, INVITE, WORLD, RING -> CALL_MS;
            case INFO, TRAVEL -> INFO_MS;
            case MESSAGE, HOST, SOCIAL -> MESSAGE_MS;
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
        drawStack(g, mc, now, false);
        Perf.toast(started);
    }

    /**
     * The stack: in the HUD, or above the screen while no world is open (only voice chat's ring and notes are there then:
     * the world's toasts went with it).
     */
    private void drawStack(GuiGraphicsExtractor g, Minecraft mc, long now, boolean overScreen) {
        OverlaySettings settings = gg.projects.overlay.OverlayClient.get().settings();
        boolean reduced = gg.projects.overlay.OverlayClient.get().reducedMotion();
        boolean top = settings.corner() == OverlaySettings.Corner.TOP_RIGHT;
        stack.setAnchoredAtTop(top);
        // A toast under the pointer in the chat keeps every toast where it is: it does not go while it is reached for.
        boolean held = hoverKey != null && hover.target() > 0f;
        stack.update(now, (pauseWhileAway && !mc.isWindowActive()) || held, reduced);
        if (stack.isEmpty() || (!overScreen && Compat.hudHidden(mc))) {
            // Nothing drawn: a press finds no toast where one was.
            hoverKey = null;
            markTip.track(null, null, 0f, 0f, now);
            drawnAt = Long.MIN_VALUE / 2;
            return;
        }
        Shapes.prepare();
        int width = g.guiWidth();
        int height = g.guiHeight();
        float x = width - MARGIN - ToastCard.WIDTH;
        float target = overScreen ? (top ? MARGIN : height - MARGIN)
            : top ? topEdge(mc) : bottomEdge(mc, width, height, x);
        if (!anchorPlaced || reduced) {
            anchor.snap(target);
            anchorPlaced = true;
        } else {
            anchor.setTarget(target);
            anchor.step(Math.min(0.05f, Math.max(0L, now - lastFrame) / 1000f));
        }
        lastFrame = now;
        float base = anchor.value();
        drawnX = x;
        drawnBase = base;
        drawnAt = now;
        trackHover(mc, now);
        for (int i = stack.entries().size() - 1; i >= 0; i--) {
            ToastStack.Entry<ToastCard> e = stack.entries().get(i);
            float a = e.alpha(now);
            if (a <= 0.004f) {
                continue;
            }
            Draw.pushAlpha(a);
            e.content().draw(g, x, base + e.y(), e.refresh(now));
            if (e.key.equals(hoverKey)) {
                float lit = hover.value(now);
                if (lit > 0.004f) {
                    Draw.pushAlpha(lit);
                    Draw.frame(g, Frames.TOAST_HOVER, x, base + e.y(), ToastCard.WIDTH, e.content().height());
                    Draw.popAlpha();
                }
            }
            Draw.popAlpha();
        }
        // A badge's tooltip over the cards, inside the screen.
        trackMark(now, x, base);
        markTip.draw(g, now, MARGIN, MARGIN, width - MARGIN);
    }

    /** Draws the travel toast above whatever screen is open (saving, connecting, loading, the pause menu). */
    public void extractOverScreen(GuiGraphicsExtractor g) {
        Minecraft mc = Minecraft.getInstance();
        // With no world open the HUD draws nothing: a ring's card and voice chat's notes stand above the screen instead
        // (VOICE-O).
        boolean stackOver = mc.level == null && !stack.isEmpty();
        if (travel == null && !stackOver) {
            return;
        }
        long started = Perf.start();
        Draw.stratum(g);
        if (travel != null) {
            drawTravel(g, mc, now(), true);
        }
        if (stackOver) {
            drawStack(g, mc, now(), true);
        }
        Perf.toast(started);
    }

    private void drawTravel(GuiGraphicsExtractor g, Minecraft mc, long now, boolean overScreen) {
        if (travelEndsAt > 0 && now >= travelEndsAt) {
            travelEndsAt = 0;
            endTravel();
        }
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
            // On the title screen the game's copyright line runs along the bottom right: the card keeps clear of it.
            if (overScreen && Compat.screen(mc) instanceof TitleScreen) {
                y -= ABOVE_TITLE_COPYRIGHT;
            }
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
        if (Compat.screen(mc) instanceof ChatScreen chat) {
            bottom = Math.min(bottom, height - ABOVE_CHAT_INPUT);
            // And above the chat's emoji button at the line's right end, which sat on the card's corner (F7-OPEN).
            int button = ChatEmojiBar.buttonTop(chat);
            if (button > 0) {
                bottom = Math.min(bottom, button - ABOVE_EMOJI_BUTTON);
            }
        }
        return bottom;
    }

    /**
     * The stack's corner and how tall it may grow there before a new toast pushes the oldest out: from its edge to the
     * screen's other margin, and in the bottom corner up to below a minimap in the top right (REL-209: three cards in a
     * short window covered the official pack's Xaero map and its coordinates).
     */
    private void placeStack(Minecraft mc, OverlaySettings settings) {
        boolean top = settings.corner() == OverlaySettings.Corner.TOP_RIGHT;
        stack.setAnchoredAtTop(top);
        int width = mc.getWindow().getGuiScaledWidth();
        int height = mc.getWindow().getGuiScaledHeight();
        float x = width - MARGIN - ToastCard.WIDTH;
        stack.setRoom(top ? height - MARGIN - topEdge(mc)
            : bottomEdge(mc, width, height, x) - belowMinimap(height, minimapLoaded()));
    }

    /**
     * Where the stack's top edge goes in the top corner: below the status effect icons when there are any, and below a
     * minimap there.
     */
    private static float topEdge(Minecraft mc) {
        boolean effects = mc.player != null && !mc.player.getActiveEffects().isEmpty();
        float edge = MARGIN + (effects ? BELOW_EFFECTS : 0);
        return Math.max(edge, belowMinimap(mc.getWindow().getGuiScaledHeight(), minimapLoaded()));
    }

    /**
     * Below a minimap in the top right corner, GUI pixels from the top (none: the screen's margin). Its size is the
     * minimap's own setting, not readable here: {@link #MINIMAP_SHARE} of the height covers Xaero's default with its
     * coordinates line (83 of 240 GUI pixels measured) and the sizes that grow with the window.
     */
    static float belowMinimap(int guiHeight, boolean shown) {
        return shown ? (float) Math.ceil(guiHeight * MINIMAP_SHARE) + MARGIN : MARGIN;
    }

    /** Whether a mod that draws a minimap in the top right corner by default is in this game (asked once). */
    private static boolean minimapLoaded() {
        if (minimap == null) {
            FabricLoader loader = FabricLoader.getInstance();
            minimap = MINIMAP_MODS.stream().anyMatch(loader::isModLoaded);
        }
        return minimap;
    }

    static long now() {
        return System.nanoTime() / 1_000_000L;
    }
}
