package gg.projects.overlay.ui;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.state.OverlayState;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.util.Mth;
import org.lwjgl.glfw.GLFW;

/**
 * The overlay panel: a drawer on the right with the crew call, friends and one conversation. It never pauses
 * the game (singleplayer keeps ticking) and draws only a light dim, so the world stays visible behind it.
 * Widgets and wrapped text are rebuilt only when launcher data changes, never per frame.
 */
public final class OverlayScreen extends Screen {
    private static final int PAD = 8;
    private static final int ROW = 14;
    private static final int MAX_FRIENDS = 6;
    private static final int MAX_TABS = 3;
    private static final String[] ANSWERS = {"coming", "soon", "cant"};

    private record Label(FormattedCharSequence text, int x, int y, int color) {
    }

    private record Dot(int x, int y, int color) {
    }

    private final OverlayClient overlay;
    private final OverlayState state;
    private final List<Label> labels = new ArrayList<>();
    private final List<Dot> dots = new ArrayList<>();
    private final List<Label> chat = new ArrayList<>();

    private String conversation;
    private EditBox input;
    private int panelX;
    private int panelW;
    private int chatTop;
    private int chatBottom;

    public OverlayScreen(OverlayClient overlay) {
        super(Component.translatable("projects_overlay.panel.title"));
        this.overlay = overlay;
        this.state = overlay.state();
        this.conversation = pickConversation();
    }

    /** The conversation on screen; its new messages do not raise toasts. */
    public String openConversation() {
        return conversation;
    }

    public void selectConversation(String id) {
        if (id != null && !id.equals(conversation)) {
            conversation = id;
            overlay.openConversation(id);
            rebuild();
        }
    }

    /** The message field (the spike driver types into it through the normal input path). */
    public EditBox input() {
        return input;
    }

    /** Called by the overlay on the render thread after every launcher frame. */
    public void onStateChanged() {
        if (conversation == null || state.conversation(conversation) == null) {
            String next = pickConversation();
            if (next != null && !next.equals(conversation)) {
                conversation = next;
                overlay.openConversation(next);
            }
        }
        rebuild();
    }

    private String pickConversation() {
        String first = null;
        for (OverlayState.Conversation c : state.conversations()) {
            if (first == null) {
                first = c.id();
            }
            if (c.unread() > 0) {
                return c.id();
            }
        }
        return first;
    }

    @Override
    protected void init() {
        panelW = Mth.clamp(width * 2 / 5, 220, 320);
        panelX = width - panelW;
        rebuild();
        if (conversation != null) {
            overlay.openConversation(conversation);
        }
    }

    private void rebuild() {
        if (font == null) {
            return;
        }
        String draft = input == null ? "" : input.getValue();
        clearWidgets();
        labels.clear();
        dots.clear();
        int x = panelX + PAD;
        int w = panelW - 2 * PAD;
        int y = 30;

        BridgeClient.Status status = state.status();
        // Without the launcher the last known state stays readable, but nothing can be sent.
        boolean live = status == BridgeClient.Status.ONLINE;
        if (!live) {
            Component detail = statusDetail(status, state.statusReason());
            if (detail != null) {
                for (FormattedCharSequence line : font.split(detail, w)) {
                    labels.add(new Label(line, x, y, Theme.TEXT_DIM));
                    y += 10;
                }
                y += 4;
            }
            if (status != BridgeClient.Status.CONNECTING && status != BridgeClient.Status.OUTDATED) {
                addRenderableWidget(Button.builder(tr("retry"), b -> overlay.retry()).bounds(x, y, 96, 16).build());
                y += 24;
            }
        } else if (state.dnd()) {
            labels.add(label(tr("dnd"), x, y, Theme.TEXT_MUTED));
            y += 14;
        }

        Iterator<OverlayState.CrewCall> calls = state.calls().iterator();
        if (calls.hasNext()) {
            OverlayState.CrewCall call = calls.next();
            labels.add(label(Component.translatable("projects_overlay.call.title", call.from(), call.crew()), x, y,
                Theme.GOLD_SOFT));
            y += 11;
            labels.add(label(Component.translatable("projects_overlay.call.counts", call.coming(), call.soon(),
                call.cant()), x, y, Theme.TEXT_DIM));
            y += 13;
            int bw = (w - 8) / 3;
            for (int i = 0; i < ANSWERS.length; i++) {
                String answer = ANSWERS[i];
                Component text = answer.equals(call.mine()) ? Component.literal("• ").append(tr("call." + answer))
                    : tr("call." + answer);
                Button button = Button.builder(text, b -> overlay.answer(call.id(), answer))
                    .bounds(x + i * (bw + 4), y, bw, 16).build();
                button.active = live;
                addRenderableWidget(button);
            }
            y += 26;
        }

        labels.add(label(tr("friends"), x, y, Theme.TEXT_MUTED));
        y += 13;
        int shown = 0;
        for (OverlayState.Friend f : state.friends()) {
            if (f.where().equals("offline") || shown == MAX_FRIENDS) {
                continue;
            }
            shown++;
            dots.add(new Dot(x + 1, y + 5, f.where().equals("launcher") ? Theme.TEXT_MUTED : Theme.JADE));
            labels.add(label(Component.literal(f.name()), x + 9, y + 3, Theme.TEXT));
            labels.add(label(tr("friend.where." + f.where()), x + 15 + font.width(f.name()), y + 3, Theme.TEXT_MUTED));
            if (f.joinable()) {
                Button join = Button.builder(tr("friend.join"), b -> overlay.join(f.id()))
                    .bounds(x + w - 48, y, 48, ROW).build();
                join.active = live;
                addRenderableWidget(join);
            }
            y += ROW + 2;
        }
        if (shown == 0) {
            labels.add(label(tr("friends.empty"), x, y + 3, Theme.TEXT_MUTED));
            y += ROW + 2;
        }

        y += 6;
        labels.add(label(tr("chats"), x, y, Theme.TEXT_MUTED));
        y += 13;
        int tabW = (w - (MAX_TABS - 1) * 4) / MAX_TABS;
        int tabs = 0;
        for (OverlayState.Conversation c : state.conversations()) {
            if (tabs == MAX_TABS) {
                break;
            }
            boolean selected = c.id().equals(conversation);
            String text = c.unread() > 0 && !selected ? c.title() + " (" + c.unread() + ")" : c.title();
            Button tab = Button.builder(Component.literal(text), b -> selectConversation(c.id()))
                .bounds(x + tabs * (tabW + 4), y, tabW, 16).build();
            tab.active = !selected;
            addRenderableWidget(tab);
            tabs++;
        }
        y += 22;
        chatTop = y;

        int inputY = height - PAD - 18;
        chatBottom = inputY - 6;
        input = new EditBox(font, x, inputY, w - 52, 18, tr("chat.hint"));
        input.setMaxLength(Protocol.MAX_MESSAGE_CHARS);
        input.setHint(tr("chat.hint"));
        input.setValue(draft);
        input.setEditable(conversation != null);
        addRenderableWidget(input);
        Button send = Button.builder(tr("chat.send"), b -> sendDraft()).bounds(x + w - 48, inputY, 48, 18).build();
        send.active = live && conversation != null;
        addRenderableWidget(send);
        setInitialFocus(input);

        wrapChat(x, w);
    }

    /** Wraps the conversation once per change; rendering only draws the prepared lines. */
    private void wrapChat(int x, int w) {
        chat.clear();
        if (conversation == null) {
            chat.add(label(tr("chat.none"), x, 0, Theme.TEXT_MUTED));
            return;
        }
        long now = System.currentTimeMillis();
        for (OverlayState.ChatLine line : state.lines(conversation)) {
            OverlayState.Delivery delivery = line.deliveryAt(now);
            Component text = line.mine ? Component.literal(line.text)
                : Component.literal(line.from + ": ").withColor(Theme.TEXT_DIM)
                    .append(Component.literal(line.text).withColor(Theme.TEXT));
            if (delivery == OverlayState.Delivery.FAILED) {
                text = text.copy().append(Component.literal(" · ").append(tr("chat.failed")).withColor(Theme.FLARE));
            }
            int color = switch (delivery) {
                case PENDING -> Theme.TEXT_MUTED;
                case FAILED -> Theme.FLARE;
                case SENT -> line.mine ? Theme.GOLD_SOFT : Theme.TEXT;
            };
            for (FormattedCharSequence part : font.split(text, w)) {
                chat.add(new Label(part, x, 0, color));
            }
        }
        if (chat.isEmpty()) {
            chat.add(label(tr("chat.empty"), x, 0, Theme.TEXT_MUTED));
        }
    }

    private void sendDraft() {
        if (conversation == null || input == null) {
            return;
        }
        String text = Protocol.plain(input.getValue(), Protocol.MAX_MESSAGE_CHARS);
        if (text.isEmpty()) {
            return;
        }
        input.setValue("");
        overlay.sendChat(conversation, text);
    }

    @Override
    public void extractBackground(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        // No blur, no menu texture: only a light dim, the world keeps running and stays visible.
        g.fill(0, 0, width, height, Theme.SCRIM);
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        long started = Perf.start();
        g.fill(panelX, 0, width, height, Theme.PANEL);
        g.fill(panelX, 0, panelX + 1, height, Theme.BORDER);
        int x = panelX + PAD;
        g.text(font, title, x, 10, Theme.TEXT, false);
        BridgeClient.Status status = state.status();
        Component statusLabel = tr("status." + status.name().toLowerCase(Locale.ROOT));
        int labelX = width - PAD - font.width(statusLabel);
        g.text(font, statusLabel, labelX, 10, Theme.TEXT_MUTED, false);
        int dot = switch (status) {
            case ONLINE -> Theme.JADE;
            case CONNECTING -> Theme.TEXT_MUTED;
            default -> Theme.FLARE;
        };
        g.fill(labelX - 8, 12, labelX - 4, 16, dot);
        g.fill(x, 23, width - PAD, 24, Theme.BORDER);

        for (Dot d : dots) {
            g.fill(d.x(), d.y(), d.x() + 4, d.y() + 4, d.color());
        }
        for (Label l : labels) {
            g.text(font, l.text(), l.x(), l.y(), l.color(), false);
        }
        int lineH = font.lineHeight + 1;
        int visible = Math.max(0, (chatBottom - chatTop) / lineH);
        int from = Math.max(0, chat.size() - visible);
        int y = chatBottom - (chat.size() - from) * lineH;
        for (int i = from; i < chat.size(); i++, y += lineH) {
            Label l = chat.get(i);
            g.text(font, l.text(), l.x(), y, l.color(), false);
        }

        super.extractRenderState(g, mouseX, mouseY, partialTick);
        Perf.panel(started);
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        boolean typing = input != null && input.isFocused();
        // The Shift+Tab chord always closes; a plain letter key closes only while not typing.
        if (overlay.key().isToggle(event) && (overlay.key().usesChord() || !typing)) {
            onClose();
            return true;
        }
        if (typing && (event.key() == GLFW.GLFW_KEY_ENTER || event.key() == GLFW.GLFW_KEY_KP_ENTER)) {
            sendDraft();
            return true;
        }
        return super.keyPressed(event);
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public void removed() {
        overlay.panelClosed();
    }

    private Label label(Component text, int x, int y, int color) {
        return new Label(text.getVisualOrderText(), x, y, color);
    }

    private static Component statusDetail(BridgeClient.Status status, String reason) {
        return switch (status) {
            case OFFLINE -> tr("detail.offline");
            case ENDED -> "game-start-close".equals(reason) ? tr("detail.closes") : tr("detail.ended");
            case OUTDATED -> tr("detail.outdated");
            case REJECTED -> tr("detail.rejected");
            default -> null;
        };
    }

    private static Component tr(String key) {
        return Component.translatable("projects_overlay." + key);
    }
}
