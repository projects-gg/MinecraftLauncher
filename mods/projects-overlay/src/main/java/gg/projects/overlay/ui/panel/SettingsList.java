package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.lang.Language;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Sounds;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BooleanSupplier;
import java.util.function.Function;
import java.util.function.IntConsumer;
import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The overlay's settings page, one list: switches, choices and sliders in rows, the shortcut, and
 * "Varsayılana dön" (with "Geri al" for a while after). The mouse clicks and drags; the keyboard moves with
 * the arrows (up and down between rows, left and right to change), Enter or Space flips. A change applies
 * at once: the next toast already follows it. Switch thumbs and choice plates glide on springs (they jump
 * under reduced motion).
 */
public final class SettingsList extends PanelWidget {
    private static final int ROW_H = 20;
    private static final int SECTION_H = 18;
    private static final int TOGGLE_W = 18;
    private static final int TOGGLE_H = 10;
    private static final int SLIDER_W = 60;
    private static final long UNDO_MS = 6_000;

    private abstract static class Item {
        String label;
        float y;
        int height = ROW_H;
        boolean section;

        boolean enabled(OverlaySettings s) {
            return true;
        }

        Component value(OverlaySettings s) {
            return Component.empty();
        }
    }

    private static final class Section extends Item {
        Section(String label) {
            this.label = label;
            this.height = SECTION_H;
            this.section = true;
        }
    }

    private static final class Toggle extends Item {
        final Predicate<OverlaySettings> get;
        final Function<OverlaySettings, OverlaySettings> flip;
        final Spring thumb = Spring.snappy(0f);
        final Tween on = new Tween(0f);
        Predicate<OverlaySettings> enabledIf;
        boolean placed;

        Toggle(String label, Predicate<OverlaySettings> get, Function<OverlaySettings, OverlaySettings> flip) {
            this.label = label;
            this.get = get;
            this.flip = flip;
        }

        @Override
        boolean enabled(OverlaySettings s) {
            return enabledIf == null || enabledIf.test(s);
        }

        @Override
        Component value(OverlaySettings s) {
            // The overlay's words, not the game's "On" / "Off": the game may speak another language.
            return Texts.tr(get.test(s) ? "settings.on" : "settings.off");
        }
    }

    private static final class Slider extends Item {
        final Function<OverlaySettings, Integer> get;
        final java.util.function.BiFunction<OverlaySettings, Integer, OverlaySettings> set;
        final Sounds.Cue cue;
        final Predicate<OverlaySettings> enabledIf;
        final Spring shown = Spring.snappy(0f);
        boolean placed;
        int labelValue = -1;
        String labelText = "";

        Slider(String label, Function<OverlaySettings, Integer> get,
            java.util.function.BiFunction<OverlaySettings, Integer, OverlaySettings> set, Sounds.Cue cue,
            Predicate<OverlaySettings> enabledIf) {
            this.label = label;
            this.get = get;
            this.set = set;
            this.cue = cue;
            this.enabledIf = enabledIf;
        }

        @Override
        boolean enabled(OverlaySettings s) {
            return enabledIf.test(s);
        }

        @Override
        Component value(OverlaySettings s) {
            return Component.literal(Texts.percent(get.apply(s)));
        }
    }

    private static final class Choice extends Item {
        final String[] options;
        final int[] widths;
        final Function<OverlaySettings, Integer> get;
        final java.util.function.BiFunction<OverlaySettings, Integer, OverlaySettings> set;
        final Spring plate = Spring.snappy(0f);
        /** A choice kept elsewhere than in the overlay's settings (the language, the launcher's): the pick goes here. */
        IntConsumer pick;
        BooleanSupplier enabledIf;
        boolean placed;
        float optionWidth;

        Choice(String label, String[] options, Function<OverlaySettings, Integer> get,
            java.util.function.BiFunction<OverlaySettings, Integer, OverlaySettings> set) {
            this.label = label;
            this.options = options;
            this.widths = new int[options.length];
            Font small = Fonts.small();
            int widest = 0;
            for (int i = 0; i < options.length; i++) {
                widths[i] = small.width(options[i]);
                widest = Math.max(widest, widths[i]);
            }
            this.optionWidth = widest + 12;
            this.get = get;
            this.set = set;
        }

        @Override
        boolean enabled(OverlaySettings s) {
            return enabledIf == null || enabledIf.getAsBoolean();
        }

        @Override
        Component value(OverlaySettings s) {
            return Component.literal(options[get.apply(s)]);
        }
    }

    /**
     * The "Dil" row (OVL-LANG): the language the overlay shows, whether a pick reaches the launcher now (which keeps it),
     * and the pick. The options name themselves in their own language.
     */
    public interface Languages {
        Language current();

        boolean editable();

        void pick(Language language);
    }

    private static final class KeyRow extends Item {
        final Supplier<String> key;
        final Runnable change;
        String changeLabel;
        String noneLabel;
        String shown;
        boolean bound;
        long checkedAt;

        KeyRow(String label, Supplier<String> key, Runnable change) {
            this.label = label;
            this.key = key;
            this.change = change;
            this.changeLabel = Texts.str("settings.key.change");
            this.noneLabel = Texts.str("settings.key.none");
        }

        /** The key's name, read again at most twice a second (it changes only in Controls). */
        void check(long now) {
            if (shown == null || now - checkedAt > 500) {
                String k = key.get();
                bound = k != null;
                shown = k == null ? noneLabel : k;
                checkedAt = now;
            }
        }

        @Override
        Component value(OverlaySettings s) {
            String k = key.get();
            return k == null ? Texts.tr("settings.key.none") : Component.literal(k);
        }
    }

    private static final class ResetRow extends Item {
        ResetRow(String label) {
            this.label = label;
            this.height = ROW_H + 6;
        }
    }

    private final Supplier<OverlaySettings> settings;
    private final java.util.function.Consumer<UnaryOperator<OverlaySettings>> apply;
    private final List<Item> items = new ArrayList<>();
    private final Spring scroll = Spring.snappy(0f);
    private int selected = -1;
    private float contentHeight;
    private boolean reducedMotion;
    private long lastFrame;
    private Slider dragging;
    private OverlaySettings undo;
    private long undoUntil;
    private String resetDone = "";
    private String undoLabel = "";

    /**
     * {@code languages}: the "Dil" row's language, the launcher's (OVL-LANG), at the top under "Genel"; null leaves it out
     * (an older launcher: the overlay follows the game's language).
     */
    public SettingsList(Supplier<OverlaySettings> settings,
        java.util.function.Consumer<UnaryOperator<OverlaySettings>> apply, Supplier<String> keyName,
        Runnable changeKey, Languages languages) {
        super(0, 0, 100, 100, Texts.tr("settings.title"));
        this.settings = settings;
        this.apply = apply;
        if (languages != null) {
            Language[] all = Language.values();
            String[] names = new String[all.length];
            for (int i = 0; i < all.length; i++) {
                names[i] = all[i].ownName;
            }
            items.add(new Section(Texts.str("settings.general")));
            Choice language = new Choice(Texts.str("settings.language"), names, s -> languages.current().ordinal(),
                null);
            language.pick = i -> languages.pick(all[i]);
            language.enabledIf = languages::editable;
            items.add(language);
        }
        items.add(new Section(Texts.str("settings.notifications")));
        items.add(new Toggle(Texts.str("settings.toasts.message"), OverlaySettings::messageToasts,
            s -> s.withMessageToasts(!s.messageToasts())));
        items.add(new Toggle(Texts.str("settings.toasts.call"), OverlaySettings::callToasts,
            s -> s.withCallToasts(!s.callToasts())));
        items.add(new Toggle(Texts.str("settings.toasts.invite"), OverlaySettings::inviteToasts,
            s -> s.withInviteToasts(!s.inviteToasts())));
        items.add(new Toggle(Texts.str("settings.toasts.info"), OverlaySettings::infoToasts,
            s -> s.withInfoToasts(!s.infoToasts())));
        Toggle text = new Toggle(Texts.str("settings.show_text"), OverlaySettings::showMessageText,
            s -> s.withShowMessageText(!s.showMessageText()));
        text.enabledIf = OverlaySettings::messageToasts;
        items.add(text);
        items.add(new Choice(Texts.str("settings.duration"), new String[] {Texts.str("settings.duration.short"),
            Texts.str("settings.duration.normal"), Texts.str("settings.duration.long")},
            s -> s.duration().ordinal(), (s, i) -> s.withDuration(OverlaySettings.Duration.values()[i])));
        items.add(new Choice(Texts.str("settings.corner"), new String[] {Texts.str("settings.corner.bottom"),
            Texts.str("settings.corner.top")}, s -> s.corner().ordinal(),
            (s, i) -> s.withCorner(OverlaySettings.Corner.values()[i])));
        items.add(new Section(Texts.str("settings.sound")));
        items.add(new Toggle(Texts.str("settings.sound.message"), OverlaySettings::messageSound,
            s -> s.withMessageSound(!s.messageSound())));
        items.add(new Slider(Texts.str("settings.sound.volume"), OverlaySettings::messageVolume,
            OverlaySettings::withMessageVolume, Sounds.Cue.MESSAGE, OverlaySettings::messageSound));
        items.add(new Toggle(Texts.str("settings.sound.call"), OverlaySettings::callSound,
            s -> s.withCallSound(!s.callSound())));
        items.add(new Slider(Texts.str("settings.sound.volume"), OverlaySettings::callVolume,
            OverlaySettings::withCallVolume, Sounds.Cue.CALL, OverlaySettings::callSound));
        items.add(new Section(Texts.str("settings.motion")));
        items.add(new Toggle(Texts.str("settings.reduce_motion"), OverlaySettings::reduceMotion,
            s -> s.withReduceMotion(!s.reduceMotion())));
        items.add(new Section(Texts.str("settings.key")));
        items.add(new KeyRow(Texts.str("settings.key.toggle"), keyName, changeKey));
        items.add(new ResetRow(Texts.str("settings.reset")));
        resetDone = Texts.str("settings.reset.done");
        undoLabel = Texts.str("settings.reset.undo");
        float y = 0f;
        for (Item item : items) {
            item.y = y;
            y += item.height;
        }
        contentHeight = y + 4;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** The scenario driver: the "Dil" row chosen as the arrow keys choose it (false: there is no such row). */
    public boolean chooseLanguageRow() {
        for (int i = 0; i < items.size(); i++) {
            if (items.get(i) instanceof Choice c && c.pick != null) {
                selected = i;
                reveal();
                return true;
            }
        }
        return false;
    }

    /** The scenario driver: the middle of the "Dil" row's option {@code index} on screen, or null without the row. */
    public float[] languageOption(int index) {
        for (Item item : items) {
            if (item instanceof Choice c && c.pick != null) {
                float wellX = getX() + getWidth() - (c.optionWidth * c.options.length + 4);
                return new float[] {wellX + 2 + (index + 0.5f) * c.optionWidth,
                    getY() + c.y - scroll.value() + c.height / 2f};
            }
        }
        return null;
    }

    /** Stands where {@code from} stood: the same row chosen, the same scroll (the panel built anew in another language). */
    public void carry(SettingsList from) {
        if (from == null || from.items.size() != items.size()) {
            return;
        }
        selected = from.selected;
        scroll.snap(from.scroll.target());
    }

    /** A choice row's pick: the launcher's choice goes to it, the overlay's own changes its settings. */
    private void choose(Choice c, int option) {
        if (c.pick != null) {
            undoUntil = 0;
            c.pick.accept(option);
        } else {
            change(x -> c.set.apply(x, option));
        }
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - getHeight());
    }

    private void change(UnaryOperator<OverlaySettings> op) {
        undoUntil = 0;
        apply.accept(op);
    }

    private void activate(Item item, int direction) {
        OverlaySettings s = settings.get();
        if (!item.enabled(s)) {
            return;
        }
        if (item instanceof Toggle t) {
            boolean on = t.get.test(s);
            if (direction == 0 || (direction > 0) != on) {
                change(t.flip::apply);
            }
        } else if (item instanceof Choice c) {
            int now = c.get.apply(s);
            int next = direction == 0 ? (now + 1) % c.options.length
                : Math.max(0, Math.min(c.options.length - 1, now + direction));
            if (next != now) {
                choose(c, next);
            }
        } else if (item instanceof Slider sl) {
            if (direction != 0) {
                int next = OverlaySettings.clampVolume(sl.get.apply(s) + direction * 5);
                change(x -> sl.set.apply(x, next));
                Sounds.preview(sl.cue, next);
            }
        } else if (item instanceof KeyRow k) {
            if (direction == 0) {
                k.change.run();
            }
        } else if (item instanceof ResetRow) {
            if (direction == 0) {
                reset();
            }
        }
    }

    private void reset() {
        long now = now();
        if (undoUntil > now && undo != null) {
            OverlaySettings back = undo;
            undo = null;
            undoUntil = 0;
            apply.accept(x -> back.withHintSeen(x.hintSeen()));
            return;
        }
        OverlaySettings before = settings.get();
        if (before.isDefault()) {
            return;
        }
        apply.accept(OverlaySettings::reset);
        undo = before;
        undoUntil = now + UNDO_MS;
    }

    private void moveSelection(int direction) {
        int i = selected;
        do {
            i += direction;
        } while (i >= 0 && i < items.size() && items.get(i).section);
        if (i < 0 || i >= items.size()) {
            if (selected < 0) {
                for (int j = 0; j < items.size(); j++) {
                    if (!items.get(j).section) {
                        selected = j;
                        break;
                    }
                }
            }
        } else {
            selected = i;
        }
        reveal();
    }

    private void reveal() {
        if (selected < 0) {
            return;
        }
        Item item = items.get(selected);
        float target = scroll.target();
        if (item.y < target + 2) {
            target = item.y - (selected > 0 && items.get(selected - 1).section ? SECTION_H : 4);
        } else if (item.y + item.height > target + getHeight()) {
            target = item.y + item.height - getHeight() + 4;
        }
        target = Math.max(0f, Math.min(maxScroll(), target));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    @Override
    public void setFocused(boolean focused) {
        super.setFocused(focused);
        if (focused && selected < 0) {
            moveSelection(1);
        }
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isUp()) {
            PanelInput.usedKeyboard();
            moveSelection(-1);
            return true;
        }
        if (event.isDown()) {
            PanelInput.usedKeyboard();
            moveSelection(1);
            return true;
        }
        if (selected >= 0 && (event.isLeft() || event.isRight())) {
            PanelInput.usedKeyboard();
            activate(items.get(selected), event.isLeft() ? -1 : 1);
            return true;
        }
        if (selected >= 0 && (event.isSelection() || event.isConfirmation())) {
            PanelInput.usedKeyboard();
            activate(items.get(selected), 0);
            return true;
        }
        if (event.key() == InputConstants.KEY_HOME || event.key() == InputConstants.KEY_END) {
            PanelInput.usedKeyboard();
            selected = -1;
            if (event.key() == InputConstants.KEY_HOME) {
                moveSelection(1);
            } else {
                selected = items.size();
                moveSelection(-1);
            }
            return true;
        }
        return false;
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY) || maxScroll() <= 0f) {
            return false;
        }
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() - (float) scrollY * 24f));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
        return true;
    }

    private int itemAt(double mouseY) {
        float cy = (float) mouseY - getY() + scroll.value();
        for (int i = 0; i < items.size(); i++) {
            Item item = items.get(i);
            if (!item.section && cy >= item.y && cy < item.y + item.height) {
                return i;
            }
        }
        return -1;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int i = itemAt(event.y());
        if (i < 0) {
            return;
        }
        selected = i;
        Item item = items.get(i);
        OverlaySettings s = settings.get();
        if (!item.enabled(s)) {
            return;
        }
        float right = getX() + getWidth();
        if (item instanceof Choice c) {
            float start = right - c.optionWidth * c.options.length - 2;
            int option = (int) ((event.x() - start - 2) / c.optionWidth);
            if (option >= 0 && option < c.options.length && option != c.get.apply(s)) {
                choose(c, option);
            }
        } else if (item instanceof Slider sl) {
            dragging = sl;
            dragTo(sl, event.x());
        } else if (item instanceof KeyRow k) {
            if (event.x() >= right - changeWidth(k)) {
                k.change.run();
            }
        } else if (item instanceof ResetRow) {
            reset();
        } else {
            activate(item, 0);
        }
    }

    @Override
    protected void onDrag(MouseButtonEvent event, double dragX, double dragY) {
        if (dragging != null) {
            dragTo(dragging, event.x());
        }
    }

    @Override
    public void onRelease(MouseButtonEvent event) {
        if (dragging != null) {
            Sounds.preview(dragging.cue, dragging.get.apply(settings.get()));
            dragging = null;
        }
    }

    private void dragTo(Slider sl, double mouseX) {
        float trackX = getX() + getWidth() - SLIDER_W;
        int value = Math.round((float) ((mouseX - trackX) / SLIDER_W) * OverlaySettings.MAX_VOLUME);
        int next = OverlaySettings.clampVolume(value);
        if (next != sl.get.apply(settings.get())) {
            apply.accept(x -> sl.set.apply(x, next));
        }
    }

    private static float changeWidth(KeyRow k) {
        return Fonts.small().width(k.changeLabel) + 12;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        OverlaySettings s = settings.get();
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        int h = getHeight();
        float right = x0 + w;
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        g.enableScissor(x0 - 4, y0, x0 + w + 4, y0 + h);
        float top = scroll.value();
        for (int i = 0; i < items.size(); i++) {
            Item item = items.get(i);
            float y = y0 + item.y - top;
            if (y > y0 + h || y + item.height < y0) {
                stepHidden(item, s, dt);
                continue;
            }
            if (item.section) {
                Draw.text(g, small, item.label, x0, Fonts.centeredY(Fonts.Face.SMALL, y + SECTION_H / 2f + 2),
                    Theme.ASH_400);
                Draw.hairline(g, x0 + small.width(item.label) + 6, right, y + SECTION_H / 2f + 1.5f,
                    Theme.withAlpha(0xFFFFFF, 0.07f));
                continue;
            }
            boolean enabled = item.enabled(s);
            boolean lit = isHovered() && mouseY >= y && mouseY < y + item.height;
            boolean chosen = i == selected && keyboardFocus();
            if (chosen || (lit && enabled && !(item instanceof ResetRow))) {
                Draw.frame(g, Frames.ROW_HOVER, x0 - 4, y + 1, w + 8, item.height - 2, chosen ? 0xFFFFFFFF
                    : 0x99FFFFFF);
            }
            Draw.pushAlpha(enabled ? 1f : 0.45f);
            float mid = y + item.height / 2f;
            if (!(item instanceof ResetRow)) {
                Draw.text(g, ui, item.label, x0, Fonts.centeredY(Fonts.Face.UI, mid), Theme.BONE_100);
            }
            if (item instanceof Toggle t) {
                drawToggle(g, t, s, right - TOGGLE_W, mid - TOGGLE_H / 2f, now, dt, chosen);
            } else if (item instanceof Slider sl) {
                drawSlider(g, sl, s, right - SLIDER_W, mid, dt, small);
            } else if (item instanceof Choice c) {
                drawChoice(g, c, s, right, mid, dt, small, mouseX, lit);
            } else if (item instanceof KeyRow k) {
                k.check(now);
                drawKey(g, k, right, mid, small, mouseX, lit);
            } else if (item instanceof ResetRow) {
                drawReset(g, (ResetRow) item, x0, w, mid, now, small, lit, s);
            }
            Draw.popAlpha();
        }
        g.disableScissor();
    }

    /** Rows out of view still settle their springs, so a scroll back shows the right state. */
    private void stepHidden(Item item, OverlaySettings s, float dt) {
        if (item instanceof Toggle t) {
            t.thumb.snap(t.get.test(s) ? 1f : 0f);
            t.placed = true;
        }
    }

    private void drawToggle(GuiGraphicsExtractor g, Toggle t, OverlaySettings s, float x, float y, long now,
        float dt, boolean chosen) {
        boolean on = t.get.test(s);
        float target = on ? 1f : 0f;
        if (!t.placed || reducedMotion) {
            t.thumb.snap(target);
            t.on.snap(target);
            t.placed = true;
        } else {
            t.thumb.setTarget(target);
            t.thumb.step(dt);
            t.on.animateTo(target, now, 150, Ease.OUT);
        }
        float lit = t.on.value(now);
        Draw.frame(g, Frames.TOGGLE_OFF, x, y, TOGGLE_W, TOGGLE_H);
        if (lit > 0f) {
            Draw.pushAlpha(lit);
            Draw.frame(g, Frames.TOGGLE_ON, x, y, TOGGLE_W, TOGGLE_H);
            Draw.popAlpha();
        }
        if (chosen) {
            Draw.frame(g, Frames.FOCUS_10, x, y, TOGGLE_W, TOGGLE_H);
        }
        float travel = TOGGLE_W - TOGGLE_H;
        float cx = x + TOGGLE_H / 2f + travel * t.thumb.value();
        Draw.disc(g, 8, cx, y + TOGGLE_H / 2f, Theme.mix(Theme.ASH_300, Theme.BONE_50, lit));
    }

    private void drawSlider(GuiGraphicsExtractor g, Slider sl, OverlaySettings s, float x, float mid, float dt,
        Font small) {
        int value = sl.get.apply(s);
        float target = value / (float) OverlaySettings.MAX_VOLUME;
        if (!sl.placed || reducedMotion || dragging == sl) {
            sl.shown.snap(target);
            sl.placed = true;
        } else {
            sl.shown.setTarget(target);
            sl.shown.step(dt);
        }
        float v = Math.max(0f, Math.min(1f, sl.shown.value()));
        float ty = mid - 1.5f;
        Draw.frame(g, Frames.SLIDER_TRACK, x, ty, SLIDER_W, 3);
        float fill = Math.max(3f, SLIDER_W * v);
        Draw.frame(g, Frames.SLIDER_FILL, x, ty, fill, 3);
        Draw.disc(g, 8, x + SLIDER_W * v, mid, Theme.BONE_50);
        if (value != sl.labelValue) {
            sl.labelValue = value;
            sl.labelText = Texts.percent(value);
        }
        Draw.text(g, small, sl.labelText, x - 6 - small.width(sl.labelText), Fonts.centeredY(Fonts.Face.SMALL, mid),
            Theme.ASH_300);
    }

    private void drawChoice(GuiGraphicsExtractor g, Choice c, OverlaySettings s, float right, float mid, float dt,
        Font small, int mouseX, boolean lit) {
        int chosen = c.get.apply(s);
        float wellW = c.optionWidth * c.options.length + 4;
        float wx = right - wellW;
        float wy = mid - 7;
        float target = chosen * c.optionWidth;
        if (!c.placed || reducedMotion) {
            c.plate.snap(target);
            c.placed = true;
        } else {
            c.plate.setTarget(target);
            c.plate.step(dt);
        }
        Draw.frame(g, Frames.CHIP, wx, wy, wellW, 14);
        float px = wx + 2 + c.plate.value();
        Draw.frame(g, Frames.CHIP_SELECTED, px, wy, c.optionWidth, 14);
        for (int i = 0; i < c.options.length; i++) {
            float ox = wx + 2 + i * c.optionWidth;
            float overlap = Math.max(0f, Math.min(ox + c.optionWidth, px + c.optionWidth) - Math.max(ox, px))
                / c.optionWidth;
            boolean over = lit && mouseX >= ox && mouseX < ox + c.optionWidth;
            int ink = Theme.mix(over ? Theme.BONE_100 : Theme.ASH_400, Theme.BONE_50, overlap);
            Draw.text(g, small, c.options[i], ox + (c.optionWidth - c.widths[i]) / 2f,
                Fonts.centeredY(Fonts.Face.SMALL, mid), ink);
        }
    }

    private void drawKey(GuiGraphicsExtractor g, KeyRow k, float right, float mid, Font small, int mouseX,
        boolean lit) {
        float bw = changeWidth(k);
        float bx = right - bw;
        boolean over = lit && mouseX >= bx;
        Draw.frame(g, over ? Frames.CHIP_HOVER : Frames.CHIP, bx, mid - 7, bw, 14);
        Draw.text(g, small, k.changeLabel, bx + 6, Fonts.centeredY(Fonts.Face.SMALL, mid), over ? Theme.BONE_50
            : Theme.BONE_100);
        float kw = small.width(k.shown) + 6;
        float kx = bx - 6 - kw;
        if (k.bound) {
            Draw.frame(g, Frames.KEYCAP, kx, mid - 5, kw, 10);
            Draw.text(g, small, k.shown, kx + 3, Fonts.centeredY(Fonts.Face.SMALL, mid) - 0.5f, Theme.BONE_100);
        } else {
            Draw.text(g, small, k.shown, kx + 3, Fonts.centeredY(Fonts.Face.SMALL, mid), Theme.ASH_400);
        }
    }

    private void drawReset(GuiGraphicsExtractor g, ResetRow r, float x0, int w, float mid, long now, Font small,
        boolean lit, OverlaySettings s) {
        boolean undoing = undoUntil > now && undo != null;
        String label = undoing ? undoLabel : r.label;
        float lw = small.width(label) + 22;
        if (undoing) {
            float dw = small.width(resetDone);
            float total = dw + 8 + lw;
            float sx = x0 + (w - total) / 2f;
            Draw.text(g, small, resetDone, sx, Fonts.centeredY(Fonts.Face.SMALL, mid), Theme.ASH_300);
            drawResetButton(g, label, sx + dw + 8, mid, lw, small, lit, Icon.RESET);
        } else {
            Draw.pushAlpha(s.isDefault() ? 0.45f : 1f);
            drawResetButton(g, label, x0 + (w - lw) / 2f, mid, lw, small, lit && !s.isDefault(), Icon.RESET);
            Draw.popAlpha();
        }
    }

    private void drawResetButton(GuiGraphicsExtractor g, String label, float x, float mid, float w, Font small,
        boolean lit, Icon icon) {
        Draw.frame(g, lit ? Frames.CHIP_HOVER : Frames.CHIP, x, mid - 7, w, 14);
        Draw.icon(g, icon, 8, x + 6, mid - 4, lit ? Theme.BONE_50 : Theme.ASH_300);
        Draw.text(g, small, label, x + 17, Fonts.centeredY(Fonts.Face.SMALL, mid), lit ? Theme.BONE_50
            : Theme.BONE_100);
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        if (selected >= 0 && selected < items.size()) {
            Item item = items.get(selected);
            output.add(NarratedElementType.HINT, Texts.tr("narration.setting", item.label,
                item.value(settings.get())));
            output.add(NarratedElementType.USAGE, Texts.tr("narration.settings.usage"));
        }
    }
}
