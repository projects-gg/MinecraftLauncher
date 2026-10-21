package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Consumer;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * One conversation, as the launcher shows it: bubbles (the player's own on the right, gold-tinted; others'
 * on the left, with names over a group in crew chat), the time and where a message stands under a group's
 * last bubble, "düzenlendi", a quiet mark where a message was deleted, and a failed message's reason with
 * "Tekrar dene" or "Düzenle" and "Sil". Laid out only when the conversation changes, so a frame only draws.
 *
 * <p>The list sits at its newest message: a new one rises in and the older ones glide up to make room.
 * Scrolled up, it stays where it is and a "Yeni mesajlar" pill takes the player down.
 */
public final class MessageList extends PanelWidget {
    private static final int BUBBLE_PAD_X = 6;
    private static final int BUBBLE_PAD_Y = 4;
    private static final int LINE_H = 10;
    private static final int SMALL_H = 9;
    private static final int GROUP_GAP = 7;
    private static final int ROW_GAP = 2;
    private static final int DIVIDER_H = 16;
    private static final long ENTER_MS = 200;
    private static final float WHEEL_STEP = 24f;
    /** Right after a conversation opens, its history arrives: the list lands on it instead of gliding. */
    private static final long SETTLE_MS = 700;

    /** What the player can do with a failed message. */
    public interface Actions {
        void retry(OverlayState.ChatLine line);

        void rewrite(OverlayState.ChatLine line);

        void discard(OverlayState.ChatLine line);
    }

    private static final class Row {
        OverlayState.ChatLine line;
        boolean mine;
        FormattedCharSequence name;
        List<FormattedCharSequence> text;
        float textWidth;
        float y;
        float height;
        float bubbleW;
        float bubbleH;
        boolean meta;
        String time;
        String edited;
        float metaWidth;
        OverlayState.Delivery delivery;
        String reason;
        boolean rewrite;
        String firstAction;
        String secondAction;
        float firstActionX;
        float firstActionW;
        float secondActionX;
        float secondActionW;
        float actionsY;
        boolean divider;
        long appearedAt;
    }

    private final Actions actions;
    private final List<Row> rows = new ArrayList<>();
    private final Spring scroll = Spring.snappy(0f);
    private final Set<String> known = new HashSet<>();
    private String conversation;
    private boolean crew;
    private int laidOutRevision = -1;
    private int laidOutWidth = -1;
    private long nextTimeout = Long.MAX_VALUE;
    private int unreadAtOpen;
    private float contentHeight;
    private boolean pinned = true;
    private long settleUntil;
    private boolean newBelow;
    private boolean reducedMotion;
    private long lastFrame;
    private Component lastNarration = Component.empty();
    private String deletedText = "";
    private String editedText = "";
    private String dividerText = "";
    private String newBelowText = "";
    private float newBelowWidth;
    private String emptyText = "";

    public MessageList(Actions actions) {
        super(0, 0, 100, 100, Texts.tr("chat.list"));
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    public String conversation() {
        return conversation;
    }

    /** Shows another conversation (its newest messages). */
    public void open(String conv, boolean crewChat, int unread) {
        conversation = conv;
        crew = crewChat;
        unreadAtOpen = Math.max(0, unread);
        laidOutRevision = -1;
        known.clear();
        rows.clear();
        pinned = true;
        newBelow = false;
        settleUntil = now() + SETTLE_MS;
    }

    /** Lays out again when the conversation's lines, the width or a waiting message's time changed. */
    public void refresh(OverlayState state, long now) {
        if (conversation == null) {
            rows.clear();
            contentHeight = 0f;
            emptyText = Texts.str("chat.none");
            return;
        }
        int revision = state.lineRevision(conversation);
        if (revision == laidOutRevision && getWidth() == laidOutWidth && now < nextTimeout) {
            return;
        }
        boolean first = laidOutRevision == -1 || now < settleUntil;
        float oldMax = maxScroll();
        float oldTop = scroll.value();
        layout(state, now, first);
        laidOutRevision = revision;
        laidOutWidth = getWidth();
        float max = maxScroll();
        if (first || reducedMotion) {
            scroll.snap(pinned ? max : Math.min(max, oldTop));
        } else if (pinned) {
            // The older messages glide up as the new one comes in.
            scroll.snap(Math.min(oldTop, max));
            scroll.setTarget(max);
        } else if (max > oldMax + 0.5f) {
            newBelow = true;
        }
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - getHeight());
    }

    private void layout(OverlayState state, long now, boolean first) {
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        deletedText = Texts.str("chat.deleted");
        editedText = Texts.str("chat.edited");
        dividerText = Texts.str("chat.divider");
        newBelowText = Texts.str("chat.new_below");
        newBelowWidth = small.width(newBelowText) + 22f;
        emptyText = Texts.str("chat.empty");
        rows.clear();
        nextTimeout = Long.MAX_VALUE;
        int width = getWidth();
        int maxBubble = Math.max(60, Math.round(width * 0.78f));
        int textMax = maxBubble - 2 * BUBBLE_PAD_X;
        List<OverlayState.ChatLine> lines = new ArrayList<>(state.lines(conversation));
        int dividerAt = unreadAtOpen > 0 ? Math.max(0, lines.size() - unreadAtOpen) : -1;
        float y = 4f;
        Row previous = null;
        Set<String> seen = new HashSet<>();
        for (int i = 0; i < lines.size(); i++) {
            OverlayState.ChatLine line = lines.get(i);
            OverlayState.ChatLine next = i + 1 < lines.size() ? lines.get(i + 1) : null;
            Row row = new Row();
            row.line = line;
            row.mine = line.mine;
            row.delivery = line.deliveryAt(now);
            if (row.delivery == OverlayState.Delivery.PENDING) {
                nextTimeout = Math.min(nextTimeout, line.createdAt + OverlayState.SEND_TIMEOUT_MS + 1);
            }
            boolean newGroup = previous == null || previous.mine != line.mine
                || !previous.line.from.equals(line.from) || i == dividerAt;
            boolean lastOfGroup = next == null || next.mine != line.mine || !next.from.equals(line.from)
                || i + 1 == dividerAt;
            if (i == dividerAt && i > 0) {
                row.divider = true;
                y += 2;
            }
            if (newGroup && previous != null) {
                y += GROUP_GAP - ROW_GAP;
            }
            row.y = y;
            if (row.divider) {
                y += DIVIDER_H;
            }
            if (crew && !line.mine && newGroup) {
                row.name = Texts.visual(Texts.fit(small, line.from, maxBubble));
                y += SMALL_H + 1;
            }
            if (line.deleted) {
                row.text = List.of(Texts.visual(deletedText));
                row.textWidth = small.width(deletedText);
                row.bubbleW = row.textWidth + 2 * BUBBLE_PAD_X;
                row.bubbleH = SMALL_H + 2 * BUBBLE_PAD_Y;
            } else {
                row.text = Texts.wrap(ui, line.text, textMax, 12);
                float widest = 0f;
                for (FormattedCharSequence s : row.text) {
                    widest = Math.max(widest, ui.width(s));
                }
                row.textWidth = widest;
                row.bubbleW = Math.max(18f, widest + 2 * BUBBLE_PAD_X);
                row.bubbleH = row.text.size() * LINE_H + 2 * BUBBLE_PAD_Y;
            }
            y += row.bubbleH;
            boolean failed = row.delivery == OverlayState.Delivery.FAILED;
            row.meta = lastOfGroup || row.delivery != OverlayState.Delivery.SENT || (line.edited && !line.deleted);
            if (row.meta) {
                row.time = time(line.at);
                row.edited = line.edited && !line.deleted ? editedText : null;
                row.metaWidth = small.width(row.time) + (row.edited == null ? 0 : small.width(row.edited) + 4)
                    + (line.mine ? 11 : 0);
                y += 2 + SMALL_H;
            }
            if (failed) {
                String reason = line.failureAt(now);
                row.reason = Texts.str("chat.failure." + failureKey(reason));
                row.rewrite = isRewrite(reason);
                row.firstAction = Texts.str(row.rewrite ? "chat.rewrite" : "chat.retry");
                row.secondAction = Texts.str("chat.discard");
                row.firstActionW = small.width(row.firstAction) + 8;
                row.secondActionW = small.width(row.secondAction) + 8;
                row.secondActionX = row.secondActionW;
                row.firstActionX = row.secondActionW + 2 + row.firstActionW;
                y += 2 + SMALL_H + 2;
                row.actionsY = y;
                y += 12;
            }
            row.height = y - row.y;
            String id = line.id != null ? line.id : line.clientId;
            if (id != null) {
                seen.add(id);
                if (!first && !known.contains(id)) {
                    row.appearedAt = now;
                }
            }
            rows.add(row);
            y += ROW_GAP;
            previous = row;
        }
        known.clear();
        known.addAll(seen);
        contentHeight = rows.isEmpty() ? 0f : y + 4f;
        lastNarration = rows.isEmpty() ? Texts.tr("chat.empty") : narrationOf(rows.get(rows.size() - 1));
    }

    private static String failureKey(String reason) {
        if (reason == null) {
            return "connection";
        }
        return switch (reason) {
            case "link", "phone", "words", "rate", "muted", "off" -> reason;
            case "daily-limit" -> "daily_limit";
            case "too-long" -> "too_long";
            case "blocked", "not-allowed" -> "not_allowed";
            default -> "connection";
        };
    }

    private static boolean isRewrite(String reason) {
        return reason != null && switch (reason) {
            case "link", "phone", "words", "too-long" -> true;
            default -> false;
        };
    }

    private static String time(long at) {
        LocalDateTime t = LocalDateTime.ofInstant(Instant.ofEpochMilli(at), ZoneId.systemDefault());
        int h = t.getHour();
        int m = t.getMinute();
        return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m;
    }

    private Component narrationOf(Row row) {
        OverlayState.ChatLine line = row.line;
        if (line.deleted) {
            return Texts.tr("chat.deleted");
        }
        return line.mine ? Texts.tr("narration.mine", line.text)
            : Texts.tr("narration.theirs", line.from, line.text);
    }

    /** Takes the list to its newest message. */
    public void toBottom() {
        pinned = true;
        newBelow = false;
        if (reducedMotion) {
            scroll.snap(maxScroll());
        } else {
            scroll.setTarget(maxScroll());
        }
    }

    private void scrollBy(float delta) {
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() + delta));
        pinned = target >= maxScroll() - 1f;
        if (pinned) {
            newBelow = false;
        }
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY)) {
            return false;
        }
        scrollBy((float) -scrollY * WHEEL_STEP);
        return true;
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        switch (event.key()) {
            case InputConstants.KEY_UP -> scrollBy(-WHEEL_STEP);
            case InputConstants.KEY_DOWN -> scrollBy(WHEEL_STEP);
            case InputConstants.KEY_PAGEUP -> scrollBy(-getHeight() * 0.8f);
            case InputConstants.KEY_PAGEDOWN -> scrollBy(getHeight() * 0.8f);
            case InputConstants.KEY_HOME -> scrollBy(-contentHeight);
            case InputConstants.KEY_END -> toBottom();
            default -> {
                return false;
            }
        }
        PanelInput.usedKeyboard();
        return true;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        float mx = (float) event.x();
        float my = (float) event.y();
        if (newBelow && pillContains(mx, my)) {
            toBottom();
            return;
        }
        float cy = my - getY() + scroll.value();
        for (Row row : rows) {
            if (row.firstAction == null || cy < row.actionsY || cy > row.actionsY + 12) {
                continue;
            }
            float right = getX() + getWidth();
            float localX = mx;
            if (localX >= right - row.firstActionX && localX < right - row.firstActionX + row.firstActionW) {
                if (row.rewrite) {
                    actions.rewrite(row.line);
                } else {
                    actions.retry(row.line);
                }
                return;
            }
            if (localX >= right - row.secondActionX && localX < right - row.secondActionX + row.secondActionW) {
                actions.discard(row.line);
                return;
            }
        }
    }

    private float pillLeft() {
        return getX() + getWidth() / 2f - pillWidth() / 2f;
    }

    private float pillWidth() {
        return newBelowWidth;
    }

    private boolean pillContains(float mx, float my) {
        float top = getY() + getHeight() - 18f;
        return mx >= pillLeft() && mx < pillLeft() + pillWidth() && my >= top && my < top + 14f;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        int h = getHeight();
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        if (rows.isEmpty()) {
            Draw.text(g, small, emptyText, x0 + (w - small.width(emptyText)) / 2f,
                Fonts.centeredY(Fonts.Face.SMALL, y0 + h / 2f), Theme.ASH_400);
            return;
        }
        g.enableScissor(x0 - 3, y0, x0 + w + 3, y0 + h);
        float top = scroll.value();
        float right = x0 + w;
        for (Row row : rows) {
            float ry = y0 + row.y - top;
            if (ry > y0 + h || ry + row.height < y0) {
                continue;
            }
            float enter = row.appearedAt == 0 ? 1f : Math.min(1f, (now - row.appearedAt) / (float) ENTER_MS);
            float eased = Ease.OUT.at(enter);
            float rise = reducedMotion ? 0f : (1f - eased) * 4f;
            Draw.pushAlpha(eased);
            float y = ry + rise;
            if (row.divider) {
                String label = dividerText;
                float lw = small.width(label);
                float mid = y + DIVIDER_H / 2f - 1;
                Draw.hairline(g, x0, x0 + (w - lw) / 2f - 6, mid, Theme.withAlpha(Theme.GOLD_400, 0.35f));
                Draw.hairline(g, x0 + (w + lw) / 2f + 6, right, mid, Theme.withAlpha(Theme.GOLD_400, 0.35f));
                Draw.text(g, small, label, x0 + (w - lw) / 2f, Fonts.centeredY(Fonts.Face.SMALL, mid),
                    Theme.GOLD_300);
                y += DIVIDER_H;
            }
            if (row.name != null) {
                Draw.text(g, small, row.name, x0 + 1, Fonts.centeredY(Fonts.Face.SMALL, y + SMALL_H / 2f),
                    Theme.ASH_300);
                y += SMALL_H + 1;
            }
            float bx = row.mine ? right - row.bubbleW : x0;
            Frames frame = row.line.deleted ? Frames.BUBBLE_DELETED
                : row.delivery == OverlayState.Delivery.FAILED ? Frames.BUBBLE_FAILED
                : row.mine ? Frames.BUBBLE_MINE : Frames.BUBBLE_THEIRS;
            Draw.pushAlpha(row.delivery == OverlayState.Delivery.PENDING ? 0.7f : 1f);
            Draw.frame(g, frame, bx, y, row.bubbleW, row.bubbleH);
            float ty = y + BUBBLE_PAD_Y;
            if (row.line.deleted) {
                Draw.text(g, small, row.text.get(0), bx + BUBBLE_PAD_X,
                    Fonts.centeredY(Fonts.Face.SMALL, ty + SMALL_H / 2f), Theme.ASH_400);
            } else {
                for (FormattedCharSequence s : row.text) {
                    Draw.text(g, ui, s, bx + BUBBLE_PAD_X, Fonts.centeredY(Fonts.Face.UI, ty + LINE_H / 2f),
                        row.mine ? Theme.BONE_50 : Theme.BONE_100);
                    ty += LINE_H;
                }
            }
            Draw.popAlpha();
            y += row.bubbleH;
            if (row.meta) {
                y += 2;
                float mx = row.mine ? right - row.metaWidth - 1 : x0 + 1;
                float textY = Fonts.centeredY(Fonts.Face.SMALL, y + SMALL_H / 2f);
                if (row.mine) {
                    Icon mark = switch (row.delivery) {
                        case PENDING -> Icon.CLOCK;
                        case FAILED -> Icon.WARNING;
                        case SENT -> Icon.CHECK;
                    };
                    int markColor = row.delivery == OverlayState.Delivery.FAILED ? Theme.FLARE_400
                        : row.delivery == OverlayState.Delivery.SENT ? Theme.GOLD_300 : Theme.ASH_400;
                    Draw.icon(g, mark, 8, mx, y + 0.5f, markColor);
                    mx += 11;
                }
                Draw.text(g, small, row.time, mx, textY, Theme.ASH_400);
                if (row.edited != null) {
                    Draw.text(g, small, row.edited, mx + small.width(row.time) + 4, textY, Theme.ASH_400);
                }
                y += SMALL_H;
            }
            if (row.reason != null) {
                y += 2;
                float rw = small.width(row.reason);
                Draw.text(g, small, row.reason, right - rw - 1, Fonts.centeredY(Fonts.Face.SMALL, y + SMALL_H / 2f),
                    Theme.FLARE_300);
                y += SMALL_H + 2;
                drawAction(g, row.firstAction, right - row.firstActionX, y, row.firstActionW, mouseX, mouseY, small);
                drawAction(g, row.secondAction, right - row.secondActionX, y, row.secondActionW, mouseX, mouseY,
                    small);
            }
            Draw.popAlpha();
        }
        g.disableScissor();
        if (newBelow) {
            float pw = pillWidth();
            float px = pillLeft();
            float py = y0 + h - 18f;
            Draw.frame(g, Frames.CHIP_SELECTED, px, py, pw, 14);
            Draw.icon(g, Icon.DOWN, 8, px + 6, py + 3, Theme.GOLD_300);
            Draw.text(g, small, newBelowText, px + 16, Fonts.centeredY(Fonts.Face.SMALL, py + 7), Theme.BONE_100);
        }
    }

    private void drawAction(GuiGraphicsExtractor g, String label, float x, float y, float w, int mouseX, int mouseY,
        Font small) {
        boolean lit = isHovered() && mouseX >= x && mouseX < x + w && mouseY >= y && mouseY < y + 12;
        if (lit) {
            Draw.frame(g, Frames.CHIP_HOVER, x, y - 1, w, 14);
        }
        Draw.text(g, small, label, x + 4, Fonts.centeredY(Fonts.Face.SMALL, y + 6), lit ? Theme.BONE_100
            : Theme.ASH_300);
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("chat.list"));
        output.add(NarratedElementType.HINT, lastNarration);
    }
}
