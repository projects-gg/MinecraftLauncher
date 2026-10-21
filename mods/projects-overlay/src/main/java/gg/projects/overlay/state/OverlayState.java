package gg.projects.overlay.state;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayDeque;
import java.util.Collection;
import java.util.Deque;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.UUID;

/**
 * What the panel shows, built only from launcher frames. Lives on the render thread; the launcher keeps the
 * real social state and history, this is a small mirror for one game session.
 */
public final class OverlayState {
    /** Messages kept per conversation in the game; the launcher holds the full history. */
    public static final int MAX_LINES = 100;
    /** A message the launcher did not confirm within this time is shown as not sent. */
    public static final long SEND_TIMEOUT_MS = 10_000;

    public record Friend(String id, String name, String where, boolean joinable) {
    }

    public record Conversation(String id, String kind, String title, int unread, boolean muted) {
    }

    public record CrewCall(String id, String crew, String from, int coming, int soon, int cant, String mine) {
    }

    public enum Delivery { PENDING, SENT, FAILED }

    public static final class ChatLine {
        public final String clientId;
        public final String from;
        public final String text;
        public final boolean mine;
        public final long createdAt;
        public String id;
        public Delivery delivery;
        public String failure;

        ChatLine(String id, String clientId, String from, String text, boolean mine, Delivery delivery,
            long createdAt) {
            this.id = id;
            this.clientId = clientId;
            this.from = from;
            this.text = text;
            this.mine = mine;
            this.delivery = delivery;
            this.createdAt = createdAt;
        }

        public Delivery deliveryAt(long now) {
            if (delivery == Delivery.PENDING && now - createdAt > SEND_TIMEOUT_MS) {
                return Delivery.FAILED;
            }
            return delivery;
        }
    }

    /** A toast the launcher asked for; the mod may still skip it (hidden GUI, conversation already open). */
    public record ToastRequest(String kind, String key, String title, String body, String conversation) {
    }

    private final Map<String, Friend> friends = new LinkedHashMap<>();
    private final Map<String, Conversation> conversations = new LinkedHashMap<>();
    private final Map<String, Deque<ChatLine>> lines = new HashMap<>();
    private final Map<String, CrewCall> calls = new LinkedHashMap<>();

    private BridgeClient.Status status = BridgeClient.Status.CONNECTING;
    private String statusReason;
    private String me = "";
    private boolean dnd;
    private boolean reducedMotion;
    private int revision;

    public BridgeClient.Status status() {
        return status;
    }

    public String statusReason() {
        return statusReason;
    }

    public String me() {
        return me;
    }

    public boolean dnd() {
        return dnd;
    }

    public boolean reducedMotion() {
        return reducedMotion;
    }

    /** Bumps on every visible change so an open panel knows when to rebuild. */
    public int revision() {
        return revision;
    }

    public Collection<Friend> friends() {
        return friends.values();
    }

    public Collection<Conversation> conversations() {
        return conversations.values();
    }

    public Conversation conversation(String id) {
        return conversations.get(id);
    }

    public Collection<CrewCall> calls() {
        return calls.values();
    }

    public Deque<ChatLine> lines(String conversation) {
        return lines.computeIfAbsent(conversation, k -> new ArrayDeque<>());
    }

    public void setStatus(BridgeClient.Status next, String reason) {
        status = next;
        statusReason = reason;
        revision++;
    }

    /** Adds the player's own message before the launcher confirms it; returns the client id to send. */
    public String addPending(String conversation, String text, long now) {
        String clientId = UUID.randomUUID().toString();
        append(conversation, new ChatLine(null, clientId, me, text, true, Delivery.PENDING, now));
        return clientId;
    }

    public void markFailed(String clientId, String reason) {
        ChatLine line = findPending(clientId);
        if (line != null) {
            line.delivery = Delivery.FAILED;
            line.failure = reason;
            revision++;
        }
    }

    /** Applies one launcher frame and returns a toast to show, or null. */
    public ToastRequest apply(JsonObject frame, long now) {
        ToastRequest toast = null;
        switch (Protocol.type(frame)) {
            case "welcome" -> {
                JsonObject self = obj(frame, "me");
                me = self == null ? "" : name(self, "name");
                dnd = Protocol.bool(frame, "dnd", false);
                reducedMotion = Protocol.bool(frame, "reducedMotion", false);
            }
            case "snapshot" -> {
                friends.clear();
                forEach(frame, "friends", this::putFriend);
                conversations.clear();
                forEach(frame, "convs", this::putConversation);
                calls.clear();
                forEach(frame, "calls", this::putCall);
                dnd = Protocol.bool(frame, "dnd", dnd);
            }
            case "friend" -> putFriend(frame);
            case "friend-gone" -> friends.remove(Protocol.str(frame, "id", ""));
            case "conv" -> putConversation(frame);
            case "dnd" -> dnd = Protocol.bool(frame, "on", false);
            case "msg" -> putMessage(frame, now);
            case "history" -> {
                String conv = Protocol.str(frame, "conv", "");
                Deque<ChatLine> deque = lines(conv);
                deque.removeIf(l -> l.delivery != Delivery.PENDING);
                JsonArray msgs = arr(frame, "msgs");
                if (msgs != null) {
                    for (JsonElement e : msgs) {
                        if (e.isJsonObject()) {
                            JsonObject m = e.getAsJsonObject();
                            m.addProperty("conv", conv);
                            putMessage(m, now);
                        }
                    }
                }
            }
            case "sent" -> {
                ChatLine line = findPending(Protocol.str(frame, "cid", ""));
                if (line != null) {
                    line.delivery = Delivery.SENT;
                    line.id = Protocol.str(frame, "id", null);
                }
            }
            case "send-failed" -> markFailed(Protocol.str(frame, "cid", ""), Protocol.str(frame, "reason", "error"));
            case "call" -> putCall(frame);
            case "call-end" -> calls.remove(Protocol.str(frame, "id", ""));
            case "toast" -> toast = new ToastRequest(
                Protocol.str(frame, "kind", "message"),
                Protocol.plain(Protocol.str(frame, "key", Protocol.str(frame, "id", "")), 64),
                name(frame, "title"),
                Protocol.plain(Protocol.str(frame, "body", ""), Protocol.MAX_MESSAGE_CHARS),
                Protocol.str(frame, "conv", null));
            default -> {
                return null;
            }
        }
        revision++;
        return toast;
    }

    private void putFriend(JsonObject o) {
        String id = Protocol.str(o, "id", "");
        if (id.isEmpty()) {
            return;
        }
        friends.put(id, new Friend(id, name(o, "name"), Protocol.str(o, "where", "offline"),
            Protocol.bool(o, "joinable", false)));
    }

    private void putConversation(JsonObject o) {
        String id = Protocol.str(o, "id", "");
        if (id.isEmpty()) {
            return;
        }
        conversations.put(id, new Conversation(id, Protocol.str(o, "kind", "dm"), name(o, "title"),
            (int) Math.max(0, Protocol.num(o, "unread", 0)), Protocol.bool(o, "muted", false)));
    }

    private void putCall(JsonObject o) {
        String id = Protocol.str(o, "id", "");
        if (id.isEmpty()) {
            return;
        }
        JsonObject answers = obj(o, "answers");
        calls.put(id, new CrewCall(id, name(o, "crew"), name(o, "from"),
            answers == null ? 0 : (int) Protocol.num(answers, "coming", 0),
            answers == null ? 0 : (int) Protocol.num(answers, "soon", 0),
            answers == null ? 0 : (int) Protocol.num(answers, "cant", 0),
            Protocol.str(o, "mine", null)));
    }

    private void putMessage(JsonObject o, long now) {
        String conv = Protocol.str(o, "conv", "");
        if (conv.isEmpty()) {
            return;
        }
        String id = Protocol.str(o, "id", null);
        String clientId = Protocol.str(o, "cid", null);
        Deque<ChatLine> deque = lines(conv);
        for (ChatLine line : deque) {
            if ((id != null && id.equals(line.id)) || (clientId != null && clientId.equals(line.clientId))) {
                line.id = id;
                line.delivery = Delivery.SENT;
                return;
            }
        }
        append(conv, new ChatLine(id, clientId, name(o, "from"),
            Protocol.plain(Protocol.str(o, "text", ""), Protocol.MAX_MESSAGE_CHARS), Protocol.bool(o, "mine", false),
            Delivery.SENT, now));
    }

    private void append(String conversation, ChatLine line) {
        Deque<ChatLine> deque = lines(conversation);
        deque.addLast(line);
        while (deque.size() > MAX_LINES) {
            deque.removeFirst();
        }
        revision++;
    }

    private ChatLine findPending(String clientId) {
        if (clientId == null || clientId.isEmpty()) {
            return null;
        }
        for (Deque<ChatLine> deque : lines.values()) {
            for (ChatLine line : deque) {
                if (clientId.equals(line.clientId)) {
                    return line;
                }
            }
        }
        return null;
    }

    private static String name(JsonObject o, String key) {
        return Protocol.plain(Protocol.str(o, key, ""), Protocol.MAX_NAME_CHARS);
    }

    private static JsonObject obj(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e != null && e.isJsonObject() ? e.getAsJsonObject() : null;
    }

    private static JsonArray arr(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e != null && e.isJsonArray() ? e.getAsJsonArray() : null;
    }

    private static void forEach(JsonObject o, String key, java.util.function.Consumer<JsonObject> action) {
        JsonArray a = arr(o, key);
        if (a == null) {
            return;
        }
        for (JsonElement e : a) {
            if (e.isJsonObject()) {
                action.accept(e.getAsJsonObject());
            }
        }
    }
}
