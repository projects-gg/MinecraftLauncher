package gg.projects.overlay.chat;

import gg.projects.overlay.EmojiKey;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.emoji.Emoji;
import gg.projects.overlay.emoji.EmojiState;
import gg.projects.overlay.emoji.Insertion;
import gg.projects.overlay.emoji.UnicodeSet;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.panel.PanelInput;
import java.util.List;
import java.util.function.UnaryOperator;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenKeyboardEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenMouseEvents;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;

/**
 * What the overlay adds to the game's chat screen (CHAT-EMOJI): the emoji button with its picker, and the line's length
 * once it nears the game's 256 characters (the game stops taking letters there without a word).
 *
 * <p>The button is the pause menu's Projects button in another sprite (Küp smiling, 16 × 16 pixels in a 20 GUI pixel
 * chamfered square), right above the chat line's right end; its place is taken at the screen's first frame, once every mod
 * has added its own (No Chat Reports puts one in that very corner), moving left when it is taken and staying away when no
 * place is free. A press or the picker's key (F9 by default, {@link EmojiKey}: a key binding of the game's Controls)
 * opens the picker; the tooltip names the key bound now (no key cap while none is). In the game the key opens the chat
 * with the picker ({@link #openPickerOnNextChat}). On Projects the picker writes the server's codes, elsewhere unicode
 * text, at the caret ({@link Insertion}).
 *
 * <p>Nothing is a widget of the screen: everything is drawn over what the screen drew, and the screen's key, click and
 * wheel events are looked at first, so a click on the button or the picker never reaches chat text under it (a link). The
 * chat line keeps its focus and every key the picker does not need; while the picker is closed only its key is taken.
 */
public final class ChatEmojiBar {
    /** What the bar needs from the running overlay (the whole overlay, or the emoji picker alone). */
    public interface Host {
        EmojiState emojis();

        OverlaySettings settings();

        void updateSettings(UnaryOperator<OverlaySettings> change);

        boolean reducedMotion();

        /** The host name of the server the game is on now, or null (singleplayer, menus). */
        String serverHost();

        /** The picker's key (F9 by default), as the game's Controls bind it. */
        EmojiKey emojiKey();
    }

    /** The game's chat line holds this many characters at most. */
    public static final int MAX_LINE = 256;
    /** The length shows from here on. */
    static final int COUNT_FROM = 200;
    static final int SIZE = 20;
    private static final int SPRITE = 16;
    private static final int MARGIN = 2;
    private static final int TRIES = 4;
    private static final long TIP_DELAY_MS = 250;
    private static final long TIP_IN_MS = 110;
    private static final int TIP_H = 14;
    private static final int TIP_PAD = 5;
    private static final int TIP_GAP = 3;
    private static final int KEY_H = 10;
    private static final int KEY_PAD = 3;
    private static final Identifier SPRITE_TEXTURE = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID,
        "textures/gui/emoji_button.png");
    /** How long the key's wish to open the picker waits for the chat's first frame (a chat the game refused). */
    private static final long REQUEST_MS = 1_000;

    /** When the picker's key asked, in the game, for the chat with the picker open (0: no wish). */
    private static volatile long pickerRequestedAt;

    /** The last bar made (the scenario driver looks at it and presses it as a player would). */
    private static ChatEmojiBar last;
    /**
     * The scenario driver's stand-in for the server's host (development runs: a singleplayer world shown as Projects), or
     * null.
     */
    public static volatile String devHost;

    private final Host host;
    private final ChatScreen screen;
    private final EditBox input;
    private final EmojiPicker picker = new EmojiPicker();
    private final Tween hover = new Tween(0f);
    private final Tween press = new Tween(0f);
    private boolean placed;
    private boolean shown;
    private int x;
    private int y;
    private boolean lit;
    private long tipSince;
    private String lastValue;
    private int listVersion = -1;
    private boolean listProjects;
    private Object listFont;
    private List<Emoji> list = List.of();

    private ChatEmojiBar(Host host, ChatScreen screen, EditBox input) {
        this.host = host;
        this.screen = screen;
        this.input = input;
        this.lastValue = input.getValue();
    }

    /** The bar the last chat screen got, or null (the scenario driver). */
    public static ChatEmojiBar last() {
        return last;
    }

    /** Adds the bar to a chat screen (any screen of the game's chat, mods' subclasses too). */
    public static void attach(Host host, Screen screen) {
        if (!(screen instanceof ChatScreen chat)) {
            return;
        }
        EditBox line = chatLine(chat);
        if (line == null) {
            return;
        }
        ChatEmojiBar bar = new ChatEmojiBar(host, chat, line);
        last = bar;
        ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> bar.draw(g, mouseX, mouseY));
        ScreenKeyboardEvents.allowKeyPress(screen).register((s, event) -> bar.allowKey(event));
        ScreenMouseEvents.allowMouseClick(screen).register((s, event) -> bar.allowClick(event));
        ScreenMouseEvents.allowMouseScroll(screen).register(
            (s, mouseX, mouseY, horizontal, vertical) -> bar.allowScroll(mouseX, mouseY, vertical));
    }

    /** The chat line: the screen's text field (the game's own is its only one). */
    private static EditBox chatLine(ChatScreen chat) {
        for (GuiEventListener child : chat.children()) {
            if (child instanceof EditBox box) {
                return box;
            }
        }
        return null;
    }

    // ---- What the scenario driver does as a player would ------------------------------------------------------

    /** Opens or closes the picker as a press on the button would. */
    public void pressForDriver() {
        toggle(false);
    }

    /** Opens or closes the picker as its key would on the chat screen. */
    public void shortcutForDriver() {
        toggle(true);
    }

    /** The picker's key in the game (no screen open): the chat that opens now opens with the picker. */
    public static void openPickerOnNextChat() {
        pickerRequestedAt = now();
    }

    /** Whether the key's wish is waiting for a chat (the scenario driver looks). */
    public static boolean pickerRequested() {
        return pickerRequestedAt != 0;
    }

    /** Writes the emoji with this code (or text) as a click on it would; false when it is not in the picker. */
    public boolean pickForDriver(String code, boolean keepOpen) {
        for (Emoji e : list) {
            if (e.code().equals(code)) {
                write(e, keepOpen);
                return true;
            }
        }
        return false;
    }

    /** A key as the player would press it on the chat screen (the picker sees it first). */
    public boolean keyForDriver(KeyEvent event) {
        if (!allowKey(event)) {
            return true;
        }
        return screen.keyPressed(event);
    }

    public boolean pickerOpen() {
        return picker.isOpen();
    }

    /** Where the picker shows the emoji with this code (x, y, w, h), or null. */
    public float[] emojiBox(String code) {
        return picker.boxOf(code);
    }

    /** The wheel over the picker, as the player would turn it there. */
    public void scrollForDriver(double amount) {
        float[] box = picker.isOpen() ? picker.boxOf(list.isEmpty() ? "" : list.get(0).code()) : null;
        if (box != null) {
            picker.scrolled(box[0] + 1, box[1] + 1, amount);
        }
    }

    /** The chat line now. */
    public String lineForDriver() {
        return input.getValue();
    }

    /** Sets the chat line as typing would have (the picker steps aside as it does then). */
    public void typeForDriver(String value) {
        input.setValue(value);
    }

    /** Where the button is (x, y; its size is {@link #SIZE}), or null while it is not shown. */
    public int[] buttonBox() {
        return shown ? new int[] {x, y, SIZE, SIZE} : null;
    }

    // ---- Events ---------------------------------------------------------------------------------------------

    private boolean active() {
        return host.emojis().on() && !list().isEmpty();
    }

    private boolean allowKey(KeyEvent event) {
        if (!host.emojis().on()) {
            return true;
        }
        if (host.emojiKey().pressed(event)) {
            if (picker.isOpen() || active()) {
                toggle(true);
                return false;
            }
            return true;
        }
        if (!picker.isOpen()) {
            return true;
        }
        switch (picker.key(event)) {
            case CLOSE -> {
                picker.close();
                return false;
            }
            case PICK -> {
                Emoji e = picker.focusedEmoji();
                if (e != null) {
                    write(e, event.hasShiftDown());
                }
                return false;
            }
            case TAKEN -> {
                return false;
            }
            default -> {
                // Enter without a pick sends the line; Tab goes to the game's suggestions: the picker steps aside.
                if (event.isConfirmation() || event.isCycleFocus()) {
                    picker.close();
                }
                return true;
            }
        }
    }

    private boolean allowClick(MouseButtonEvent event) {
        double mx = event.x();
        double my = event.y();
        if (shown && inButton(mx, my)) {
            if (event.button() == 0) {
                toggle(false);
            }
            return false;
        }
        if (!picker.isOpen()) {
            return true;
        }
        if (picker.contains(mx, my)) {
            Emoji e = picker.emojiAt(mx, my);
            if (e != null && event.button() == 0) {
                write(e, event.hasShiftDown());
            }
            return false;
        }
        // A click anywhere else closes the picker and goes on as it would have (the chat line, a link).
        picker.close();
        return true;
    }

    private boolean allowScroll(double mx, double my, double amount) {
        return !(picker.isOpen() && picker.scrolled(mx, my, amount));
    }

    private boolean inButton(double mx, double my) {
        return mx >= x && mx < x + SIZE && my >= y && my < y + SIZE;
    }

    private void toggle(boolean byKeyboard) {
        long now = now();
        press.snap(1f);
        press.animateTo(0f, now, 140, Ease.OUT);
        if (picker.isOpen()) {
            picker.close();
            return;
        }
        if (!active()) {
            return;
        }
        picker.open(listProjects, list, host.settings().recentEmojis(), byKeyboard, now);
        tipSince = 0;
    }

    /** Writes the emoji at the caret (or over the selection); the picker closes unless Shift is held. */
    private void write(Emoji emoji, boolean keepOpen) {
        String value = input.getValue();
        int cursor = input.getCursorPosition();
        String selected = input.getHighlighted();
        int start = cursor;
        int end = cursor;
        if (!selected.isEmpty()) {
            if (value.startsWith(selected, cursor)) {
                end = cursor + selected.length();
            } else {
                start = Math.max(0, cursor - selected.length());
            }
        }
        String text = Insertion.around(value, start, end, emoji.code(), MAX_LINE);
        if (text == null) {
            picker.lineFull(now());
            return;
        }
        screen.insertText(text, false);
        lastValue = input.getValue();
        String used = emoji.code();
        host.updateSettings(s -> s.withEmojiUsed(used));
        if (!keepOpen) {
            picker.close();
        }
    }

    // ---- What is shown --------------------------------------------------------------------------------------

    /** The picker's emojis here: the server's on Projects, the drawable unicode set elsewhere (kept until a change). */
    private List<Emoji> list() {
        EmojiState state = host.emojis();
        String server = devHost != null ? devHost : host.serverHost();
        boolean projects = state.projects().appliesTo(server);
        Object font = Fonts.gameGlyphs();
        if (state.version() != listVersion || projects != listProjects || font != listFont) {
            listVersion = state.version();
            listProjects = projects;
            listFont = font;
            list = projects ? state.projects().emojis() : UnicodeSet.drawable(GameFont::draws);
            picker.refresh(projects, list, host.settings().recentEmojis());
        }
        return list;
    }

    /** The button's place: above the chat line's right end, moving left past other mods' buttons. */
    private void placeOnce() {
        if (placed) {
            return;
        }
        placed = true;
        int top = input.getY() - 2;
        int y0 = top - MARGIN - SIZE;
        List<AbstractWidget> others = Screens.getWidgets(screen);
        for (int i = 0; i < TRIES; i++) {
            int x0 = screen.width - MARGIN - SIZE - i * (SIZE + 4);
            if (x0 < MARGIN) {
                break;
            }
            if (free(others, x0, y0)) {
                x = x0;
                y = y0;
                shown = true;
                return;
            }
        }
    }

    private static boolean free(List<AbstractWidget> others, int x, int y) {
        for (AbstractWidget o : others) {
            if (o.visible && o.getX() < x + SIZE + 3 && o.getX() + o.getWidth() > x - 3 && o.getY() < y + SIZE + 3
                && o.getY() + o.getHeight() > y - 3) {
                return false;
            }
        }
        return true;
    }

    private void draw(GuiGraphicsExtractor g, int realX, int realY) {
        placeOnce();
        long requested = pickerRequestedAt;
        if (requested != 0) {
            // The picker's key opened this chat from the game: the picker comes with it (from the keyboard).
            pickerRequestedAt = 0;
            if (now() - requested < REQUEST_MS && !picker.isOpen()) {
                toggle(true);
            }
        }
        // The scenario driver's pointer (development runs), never the player's own.
        int mouseX = PanelInput.pointerSet() ? PanelInput.pointerX() : realX;
        int mouseY = PanelInput.pointerSet() ? PanelInput.pointerY() : realY;
        long now = now();
        String value = input.getValue();
        if (!value.equals(lastValue)) {
            // The player writes again: the picker steps aside.
            lastValue = value;
            picker.close();
        }
        boolean active = active();
        if (!active && picker.isOpen()) {
            picker.close();
        }
        Draw.stratum(g);
        drawLength(g, value);
        if (shown && active) {
            drawButton(g, mouseX, mouseY, now);
        }
        if (picker.isOpen()) {
            int right = shown ? x + SIZE : screen.width - MARGIN;
            int bottom = (shown ? y : input.getY() - 2) - 3;
            picker.place(right, bottom, screen.width, list, host.settings().recentEmojis());
            Draw.stratum(g);
            picker.draw(g, mouseX, mouseY, now, host.reducedMotion(), Texts.turkish());
        } else if (shown && active) {
            drawTip(g, now);
        }
    }

    private void drawButton(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        boolean over = inButton(mouseX, mouseY);
        boolean on = over || picker.isOpen();
        if (on != lit) {
            lit = on;
            hover.animateTo(on ? 1f : 0f, now, 120, Ease.OUT);
        }
        if (over) {
            g.requestCursor(com.mojang.blaze3d.platform.cursor.CursorTypes.POINTING_HAND);
        }
        Draw.frame(g, Frames.PAUSE_BUTTON, x, y, SIZE, SIZE);
        float h = hover.value(now);
        if (h > 0f) {
            Draw.pushAlpha(h);
            Draw.frame(g, Frames.PAUSE_BUTTON_HOVER, x, y, SIZE, SIZE);
            Draw.popAlpha();
        }
        float dy = press.value(now) * 0.5f;
        float inset = (SIZE - SPRITE) / 2f;
        Draw.texture(g, SPRITE_TEXTURE, x + inset, y + inset + dy, SPRITE, SPRITE, 0, 0, SPRITE, SPRITE, SPRITE, SPRITE,
            0xFFFFFFFF);
        if (!over) {
            tipSince = 0;
        } else if (tipSince == 0) {
            tipSince = now;
        }
    }

    /**
     * "Emojiler" and the picker's key on a keycap (the one bound now in the game's Controls; none while it is unbound),
     * above the button, once the pointer settles on it.
     */
    private void drawTip(GuiGraphicsExtractor g, long now) {
        if (tipSince == 0) {
            return;
        }
        long shownFor = now - tipSince - TIP_DELAY_MS;
        if (shownFor < 0) {
            return;
        }
        Font small = Fonts.small();
        String words = Texts.str("chat_emoji.button");
        String key = host.emojiKey().keyName();
        float wordsW = small.width(words);
        float keyW = key == null ? 0f : small.width(key) + 2 * KEY_PAD;
        float w = TIP_PAD + wordsW + (key == null ? 0f : 4 + keyW) + TIP_PAD;
        float tx = Draw.snap(Math.max(2f, Math.min(screen.width - 2f - w, x + SIZE - w)));
        float ty = Draw.snap(y - TIP_GAP - TIP_H);
        Draw.pushAlpha(Ease.OUT.at(Math.min(1f, shownFor / (float) TIP_IN_MS)));
        Draw.frame(g, Frames.TOOLTIP, tx, ty, w, TIP_H);
        float textY = Fonts.centeredY(Fonts.Face.SMALL, ty + TIP_H / 2f);
        Draw.text(g, small, Texts.visual(words), tx + TIP_PAD, textY, Theme.BONE_100);
        if (key != null) {
            float kx = tx + TIP_PAD + wordsW + 4;
            float ky = ty + (TIP_H - KEY_H) / 2f;
            Draw.frame(g, Frames.KEYCAP, kx, ky, keyW, KEY_H);
            Draw.text(g, small, Texts.visual(key), kx + KEY_PAD, Fonts.centeredY(Fonts.Face.SMALL, ky + KEY_H / 2f) - 0.5f,
                Theme.BONE_100);
        }
        Draw.popAlpha();
    }

    /** The key the tooltip names now (the scenario driver reads it), or null while the picker's key is unbound. */
    public String tipKeyForDriver() {
        return host.emojiKey().keyName();
    }

    /**
     * The line's length near the game's limit ("214/256"), on a small plate left of the button (or in its place): the game
     * stops taking letters at 256 without a word, and a long message was cut off without the player noticing.
     */
    private void drawLength(GuiGraphicsExtractor g, String value) {
        int length = value.length();
        if (length < COUNT_FROM) {
            return;
        }
        Font small = Fonts.small();
        String text = length + "/" + MAX_LINE;
        float w = small.width(text) + 2 * KEY_PAD + 2;
        float right = shown && active() ? x - 4 : screen.width - MARGIN;
        float bottom = (shown ? y + SIZE : input.getY() - 2 - MARGIN);
        float px = Draw.snap(right - w);
        float py = Draw.snap(bottom - TIP_H);
        Draw.frame(g, Frames.TOOLTIP, px, py, w, TIP_H);
        FormattedCharSequence line = Texts.visual(text);
        Draw.text(g, small, line, px + KEY_PAD + 1, Fonts.centeredY(Fonts.Face.SMALL, py + TIP_H / 2f),
            length >= MAX_LINE ? Theme.FLARE_300 : Theme.ASH_300);
    }

    private static long now() {
        return System.nanoTime() / 1_000_000L;
    }

    /** The game moved to another server (a proxy's move too): the font's answers about the server's pack start over. */
    public static void serverChanged() {
        GameFont.forget();
    }

}
