package gg.projects.overlay.state;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
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
    /** Failure reason of a message nobody answered for. */
    public static final String TIMEOUT = "timeout";
    /** A writer shows this long after their last "yazıyor…" (CHAT2 §4). */
    public static final long TYPING_SHOW_MS = 6_000;
    /** A note (a reaction that did not go) shows this long. */
    public static final long NOTE_MS = 3_200;
    /** Reactions go only on messages younger than this (the service's mailbox keeps a week). */
    public static final long REACT_WINDOW_MS = 7L * 86_400_000L - 60_000L;
    /** Different emoji one player puts on one message, and kinds one message carries (CHAT2 §7). */
    public static final int REACTIONS_PER_PLAYER = 5;
    public static final int REACTION_KINDS = 10;
    /** Characters of a quoted message the launcher sends (it ends in an ellipsis when cut). */
    public static final int MAX_QUOTE_CHARS = 121;

    public record Friend(String id, String name, String where, boolean joinable, String host) {
        public boolean onProjects() {
            return where.equals("projects");
        }

        public boolean online() {
            return !where.equals("offline");
        }

        /** In a game (Projects, another server, singleplayer or the game's menus). */
        public boolean inGame() {
            return switch (where) {
                case "projects", "server", "single", "game" -> true;
                default -> false;
            };
        }
    }

    /**
     * A conversation. {@code seen} (direct conversations, CHAT2): the friend has seen the player's messages up to
     * this service id ("Görüldü"), 0 when not known or not shared.
     */
    public record Conversation(String id, String kind, String title, int unread, boolean muted, boolean writable,
        long seen) {
        public boolean crew() {
            return kind.equals("crew");
        }
    }

    public record CrewCall(String id, String crew, String from, long since, long until, int coming, int soon,
        int cant, String mine, String where, String host, boolean own) {
        public boolean liveAt(long now) {
            return until <= 0 || now < until;
        }
    }

    public enum Delivery { PENDING, SENT, FAILED }

    /**
     * The message a reply answers, as the launcher holds it (CHAT2): {@code id} is its line's id when that is
     * here (the list scrolls to it); {@code missing}: "Önceki bir mesaj".
     */
    public record Reply(String id, long sid, String from, boolean mine, String text, String sticker,
        boolean deleted, boolean missing) {
    }

    /**
     * One kind of reaction: {@code emoji} as the launcher sent it (sent back as it is), {@code shown} as the
     * game's font draws it, how many gave it, whether the player did, a few of the others' names.
     */
    public record Reaction(String emoji, String shown, int count, boolean mine, List<String> who) {
    }

    /** A link the player may press (the launcher decided), as written in the text. */
    public record Link(String url, boolean trusted) {
    }

    /** A link's card as one quiet line ("YouTube · Rekor kırıldı"). */
    public record Card(String site, String title) {
    }

    /** Someone writing in a conversation, until when (this computer's clock). */
    public record Writer(String by, String name, long until) {
    }

    /** A short note in the panel: a reaction that did not go ({@code key} names the words). */
    public record Note(String conversation, String key, long at) {
    }

    public static final class ChatLine {
        public final String clientId;
        public final String from;
        public final boolean mine;
        public final long createdAt;
        public String id;
        public String text;
        public long at;
        public boolean edited;
        /** Deleted for everyone: shown as a quiet mark in its place. */
        public boolean deleted;
        public Delivery delivery;
        public String failure;
        /** With failure {@code link}: the rule that refused it, when the launcher said. */
        public String failureRule;
        /** CHAT2: the service's id (replies and reactions name a message by it), 0 while unknown. */
        public long sid;
        /** CHAT2: a sticker ({@code pl1/gg}) in place of text, or null. */
        public String sticker;
        public Reply reply;
        public List<Reaction> reactions = List.of();
        public List<Link> links = List.of();
        public Card card;

        ChatLine(String id, String clientId, String from, String text, boolean mine, Delivery delivery, long at,
            long createdAt) {
            this.id = id;
            this.clientId = clientId;
            this.from = from;
            this.text = text;
            this.mine = mine;
            this.delivery = delivery;
            this.at = at;
            this.createdAt = createdAt;
        }

        public Delivery deliveryAt(long now) {
            if (delivery == Delivery.PENDING && now - createdAt > SEND_TIMEOUT_MS) {
                return Delivery.FAILED;
            }
            return delivery;
        }

        public String failureAt(long now) {
            if (delivery == Delivery.PENDING && now - createdAt > SEND_TIMEOUT_MS) {
                return TIMEOUT;
            }
            return failure;
        }

        /** How many of this message's reactions are the player's. */
        public int ownReactions() {
            int n = 0;
            for (Reaction r : reactions) {
                if (r.mine()) {
                    n++;
                }
            }
            return n;
        }

        /** The reaction kind showing {@code emoji} (as drawn), or null. */
        public Reaction reaction(String shown) {
            for (Reaction r : reactions) {
                if (r.shown().equals(shown)) {
                    return r;
                }
            }
            return null;
        }
    }

    /** A toast the launcher asked for; the mod may still skip it (hidden GUI, conversation already open). */
    public record ToastRequest(String kind, String key, String title, String body, String conversation,
        String crew, String sticker) {
        public ToastRequest(String kind, String key, String title, String body, String conversation, String crew) {
            this(kind, key, title, body, conversation, crew, null);
        }
    }

    private final Map<String, Friend> friends = new LinkedHashMap<>();
    private final LinkedHashMap<String, Conversation> conversations = new LinkedHashMap<>();
    private final Map<String, Deque<ChatLine>> lines = new HashMap<>();
    private final Map<String, Integer> lineRevisions = new HashMap<>();
    private final Map<String, CrewCall> calls = new LinkedHashMap<>();
    private final Map<String, LinkedHashMap<String, Writer>> writers = new HashMap<>();
    private final Set<String> features = new HashSet<>();

    private BridgeClient.Status status = BridgeClient.Status.CONNECTING;
    private String statusReason;
    private String me = "";
    private boolean welcomed;
    private boolean dnd;
    private boolean reducedMotion;
    private int revision;
    private int typingRevision;
    private Note note;

    public BridgeClient.Status status() {
        return status;
    }

    public String statusReason() {
        return statusReason;
    }

    public boolean online() {
        return status == BridgeClient.Status.ONLINE;
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

    /** The launcher greeted this session at least once (there is something to show). */
    public boolean welcomed() {
        return welcomed;
    }

    /** Signed in to Friends in the launcher: the welcome named at least one feature. */
    public boolean signedIn() {
        return !features.isEmpty();
    }

    public boolean has(String feature) {
        return features.contains(feature);
    }

    /** Bumps on every visible change so an open panel knows when to rebuild. */
    public int revision() {
        return revision;
    }

    /** Bumps when the lines of {@code conversation} change. */
    public int lineRevision(String conversation) {
        Integer r = lineRevisions.get(conversation);
        return r == null ? 0 : r;
    }

    public Collection<Friend> friends() {
        return friends.values();
    }

    public Friend friend(String id) {
        return friends.get(id);
    }

    /** Newest activity first. */
    public Collection<Conversation> conversations() {
        return conversations.values();
    }

    public Conversation conversation(String id) {
        return id == null ? null : conversations.get(id);
    }

    public int unreadTotal() {
        int n = 0;
        for (Conversation c : conversations.values()) {
            n += c.unread();
        }
        return n;
    }

    public Collection<CrewCall> calls() {
        return calls.values();
    }

    /** The newest call that still runs and is someone else's, or null. */
    public CrewCall activeCall(long now) {
        CrewCall found = null;
        for (CrewCall c : calls.values()) {
            if (!c.own() && c.liveAt(now)) {
                found = c;
            }
        }
        return found;
    }

    public Deque<ChatLine> lines(String conversation) {
        return lines.computeIfAbsent(conversation, k -> new ArrayDeque<>());
    }

    /** A line of {@code conversation} by its id, or null. */
    public ChatLine line(String conversation, String id) {
        Deque<ChatLine> deque = lines.get(conversation);
        return deque == null || id == null ? null : findById(deque, null, id);
    }

    public void setStatus(BridgeClient.Status next, String reason) {
        status = next;
        statusReason = reason;
        revision++;
    }

    // ---- "Yazıyor…" and notes ------------------------------------------------------------------------

    /** Bumps whenever someone starts or stops writing somewhere. */
    public int typingRevision() {
        return typingRevision;
    }

    /** Who writes in {@code conversation} now, by name, in the order they started. */
    public List<String> writers(String conversation, long now) {
        LinkedHashMap<String, Writer> people = writers.get(conversation);
        if (people == null || people.isEmpty()) {
            return List.of();
        }
        List<String> names = new ArrayList<>(people.size());
        for (Writer w : people.values()) {
            if (w.until() > now) {
                names.add(w.name());
            }
        }
        return names;
    }

    /** When the first writer of {@code conversation} stops showing, or {@link Long#MAX_VALUE}. */
    public long typingEndsAt(String conversation) {
        LinkedHashMap<String, Writer> people = writers.get(conversation);
        long first = Long.MAX_VALUE;
        if (people != null) {
            for (Writer w : people.values()) {
                first = Math.min(first, w.until());
            }
        }
        return first;
    }

    /** Forgets every writer (the panel closed: no frames come while it is). */
    public void clearTyping() {
        if (!writers.isEmpty()) {
            writers.clear();
            typingRevision++;
        }
    }

    private void stopWriting(String conversation, String name) {
        LinkedHashMap<String, Writer> people = writers.get(conversation);
        if (people != null && people.values().removeIf(w -> w.name().equals(name))) {
            typingRevision++;
        }
    }

    private void stopWritingEverywhere(String by) {
        boolean changed = false;
        for (LinkedHashMap<String, Writer> people : writers.values()) {
            changed |= people.remove(by) != null;
        }
        if (changed) {
            typingRevision++;
        }
    }

    /** The note to show now (a reaction that did not go), or null. */
    public Note note(long now) {
        return note != null && now - note.at() < NOTE_MS ? note : null;
    }

    /** Shows a note ({@code chat.note.<key>}) in {@code conversation} for a moment. */
    public void note(String conversation, String key, long now) {
        note = new Note(conversation, key, now);
        revision++;
    }

    // ---- The player's own messages ---------------------------------------------------------------------

    /** Adds the player's own message before the launcher confirms it; returns the client id to send. */
    public String addPending(String conversation, String text, long now) {
        return addPending(conversation, text, null, null, now);
    }

    /**
     * Adds the player's own message (words, or a sticker when {@code sticker} is not null), maybe answering the
     * message {@code quote} stands for, before the launcher confirms it; returns the client id to send.
     */
    public String addPending(String conversation, String text, String sticker, Reply quote, long now) {
        String clientId = UUID.randomUUID().toString();
        ChatLine line = new ChatLine(null, clientId, me, sticker == null ? text : "", true, Delivery.PENDING, now,
            now);
        line.sticker = sticker;
        line.reply = quote;
        append(conversation, line);
        return clientId;
    }

    /** What a reply quotes, made from a line the panel shows. */
    public static Reply quoteOf(ChatLine line) {
        String text = line.sticker != null || line.deleted ? null
            : Protocol.plain(line.text, MAX_QUOTE_CHARS);
        return new Reply(line.id, line.sid, line.from, line.mine, text, line.sticker, line.deleted, false);
    }

    public void markFailed(String clientId, String reason) {
        ChatLine line = findByClientId(clientId);
        if (line != null && line.delivery != Delivery.SENT) {
            line.delivery = Delivery.FAILED;
            line.failure = reason;
            touch(conversationOf(line));
        }
    }

    /** Takes a message that did not go out of its conversation; returns it (for a retry or a rewrite). */
    public ChatLine takeFailed(String clientId, long now) {
        for (Map.Entry<String, Deque<ChatLine>> entry : lines.entrySet()) {
            Iterator<ChatLine> it = entry.getValue().iterator();
            while (it.hasNext()) {
                ChatLine line = it.next();
                if (clientId.equals(line.clientId) && line.deliveryAt(now) == Delivery.FAILED) {
                    it.remove();
                    touch(entry.getKey());
                    return line;
                }
            }
        }
        return null;
    }

    // ---- Launcher frames -------------------------------------------------------------------------------

    /** Applies one launcher frame and returns a toast to show, or null. */
    public ToastRequest apply(JsonObject frame, long now) {
        ToastRequest toast = null;
        switch (Protocol.type(frame)) {
            case "welcome" -> {
                JsonObject self = obj(frame, "me");
                String name = self == null ? "" : name(self, "name");
                if (welcomed && !name.equals(me)) {
                    // Another account: nothing of the previous one stays on screen.
                    lines.clear();
                    lineRevisions.clear();
                    friends.clear();
                    conversations.clear();
                    calls.clear();
                    writers.clear();
                    note = null;
                }
                me = name;
                welcomed = true;
                dnd = Protocol.bool(frame, "dnd", false);
                reducedMotion = Protocol.bool(frame, "reducedMotion", false);
                features.clear();
                JsonArray list = arr(frame, "features");
                if (list != null) {
                    for (JsonElement e : list) {
                        if (e.isJsonPrimitive() && e.getAsJsonPrimitive().isString()) {
                            features.add(e.getAsString());
                        }
                    }
                }
                typingRevision++;
            }
            case "snapshot" -> {
                friends.clear();
                forEach(frame, "friends", this::putFriend);
                LinkedHashMap<String, Conversation> next = new LinkedHashMap<>();
                forEachConversation(frame, next);
                conversations.clear();
                conversations.putAll(next);
                lines.keySet().removeIf(id -> !conversations.containsKey(id) && !hasPending(lines.get(id)));
                calls.clear();
                forEach(frame, "calls", this::putCall);
                dnd = Protocol.bool(frame, "dnd", dnd);
            }
            case "friend" -> {
                putFriend(frame);
                if ("offline".equals(Protocol.str(frame, "where", ""))) {
                    // Gone offline: no longer writing anywhere.
                    stopWritingEverywhere(Protocol.str(frame, "id", ""));
                }
            }
            case "friend-gone" -> friends.remove(Protocol.str(frame, "id", ""));
            case "conv" -> putConversation(frame);
            case "dnd" -> dnd = Protocol.bool(frame, "on", false);
            case "msg" -> putMessage(frame, now, true);
            case "msg-gone" -> {
                // A line dropped (past the history's limit, an unsent message thrown away) leaves the list. A
                // message deleted for everyone comes as a "msg" with "deleted" instead (L16) and stays as a mark.
                String conv = Protocol.str(frame, "conv", "");
                String id = Protocol.str(frame, "id", "");
                Deque<ChatLine> deque = lines.get(conv);
                ChatLine line = deque == null || id.isEmpty() ? null : findById(deque, null, id);
                if (line != null) {
                    deque.remove(line);
                    touch(conv);
                }
            }
            case "history" -> {
                String conv = Protocol.str(frame, "conv", "");
                Deque<ChatLine> deque = lines(conv);
                List<ChatLine> waiting = new ArrayList<>();
                for (ChatLine l : deque) {
                    if (l.delivery != Delivery.SENT) {
                        waiting.add(l);
                    }
                }
                deque.clear();
                JsonArray msgs = arr(frame, "msgs");
                if (msgs != null) {
                    for (JsonElement e : msgs) {
                        if (e.isJsonObject()) {
                            JsonObject m = e.getAsJsonObject();
                            m.addProperty("conv", conv);
                            putMessage(m, now, false);
                        }
                    }
                }
                for (ChatLine l : waiting) {
                    if (findById(deque, l.clientId, null) == null) {
                        deque.addLast(l);
                    }
                }
                trim(deque);
                touch(conv);
            }
            case "sent" -> {
                ChatLine line = findByClientId(Protocol.str(frame, "cid", ""));
                if (line != null) {
                    line.delivery = Delivery.SENT;
                    line.failure = null;
                    line.id = Protocol.str(frame, "id", line.id);
                    long sid = Protocol.id(frame, "sid");
                    line.sid = sid > 0 ? sid : line.sid;
                    touch(conversationOf(line));
                }
            }
            case "send-failed" -> {
                String cid = Protocol.str(frame, "cid", "");
                markFailed(cid, Protocol.str(frame, "reason", "error"));
                ChatLine line = findByClientId(cid);
                if (line != null) {
                    line.failureRule = Protocol.str(frame, "rule", null);
                }
            }
            case "typing" -> {
                String conv = Protocol.str(frame, "conv", "");
                String by = Protocol.str(frame, "by", "");
                if (conv.isEmpty() || by.isEmpty()) {
                    return null;
                }
                String name = name(frame, "name");
                writers.computeIfAbsent(conv, k -> new LinkedHashMap<>())
                    .put(by, new Writer(by, name.isEmpty() ? "?" : name, now + TYPING_SHOW_MS));
                typingRevision++;
            }
            case "react-failed" -> {
                String conv = Protocol.str(frame, "conv", "");
                note = new Note(conv, "react." + reactReason(Protocol.str(frame, "reason", "")), now);
            }
            case "call" -> putCall(frame);
            case "call-end" -> calls.remove(Protocol.str(frame, "id", ""));
            case "toast" -> {
                String sticker = Protocol.str(frame, "sticker", null);
                toast = new ToastRequest(
                    Protocol.str(frame, "kind", "message"),
                    Protocol.plain(Protocol.str(frame, "key", Protocol.str(frame, "id", "")), 64),
                    name(frame, "title"),
                    Protocol.plain(Protocol.str(frame, "body", ""), Protocol.MAX_MESSAGE_CHARS),
                    Protocol.str(frame, "conv", null),
                    Protocol.plain(Protocol.str(frame, "crew", ""), Protocol.MAX_NAME_CHARS),
                    sticker == null || !stickerRef(sticker) ? null : sticker);
            }
            default -> {
                return null;
            }
        }
        revision++;
        return toast;
    }

    /** A reaction refusal's words ({@code chat.note.react.<key>}); one this build does not know reads general. */
    private static String reactReason(String reason) {
        return switch (reason) {
            case "full", "kinds", "old", "words", "rate", "muted", "off" -> reason;
            case "not-allowed" -> "not_allowed";
            default -> "failed";
        };
    }

    /** A sticker reference as the service takes it ({@code pl1/gg}). */
    public static boolean stickerRef(String value) {
        return value != null && value.matches("[a-z0-9]{1,8}/[a-z0-9-]{1,24}");
    }

    private void putFriend(JsonObject o) {
        String id = Protocol.str(o, "id", "");
        if (id.isEmpty()) {
            return;
        }
        String host = Protocol.plain(Protocol.str(o, "host", ""), 255);
        friends.put(id, new Friend(id, name(o, "name"), Protocol.str(o, "where", "offline"),
            Protocol.bool(o, "joinable", false), host.isEmpty() ? null : host));
    }

    private void forEachConversation(JsonObject frame, LinkedHashMap<String, Conversation> into) {
        JsonArray a = arr(frame, "convs");
        if (a == null) {
            return;
        }
        for (JsonElement e : a) {
            if (e.isJsonObject()) {
                Conversation c = conversationOf(e.getAsJsonObject());
                if (c != null) {
                    into.put(c.id(), c);
                }
            }
        }
    }

    private void putConversation(JsonObject o) {
        Conversation c = conversationOf(o);
        if (c == null) {
            return;
        }
        Conversation before = conversations.put(c.id(), c);
        if (before == null) {
            // A new conversation comes first.
            moveToFront(c.id());
        } else if (before.seen() != c.seen()) {
            // "Görüldü" moved: the list draws it.
            touch(c.id());
        }
    }

    private static Conversation conversationOf(JsonObject o) {
        String id = Protocol.str(o, "id", "");
        if (id.isEmpty()) {
            return null;
        }
        return new Conversation(id, Protocol.str(o, "kind", "dm"), name(o, "title"),
            (int) Math.max(0, Math.min(9999, Protocol.num(o, "unread", 0))), Protocol.bool(o, "muted", false),
            Protocol.bool(o, "writable", true), Protocol.id(o, "seen"));
    }

    private void putCall(JsonObject o) {
        String id = Protocol.str(o, "id", "");
        if (id.isEmpty()) {
            return;
        }
        JsonObject answers = obj(o, "answers");
        String host = Protocol.plain(Protocol.str(o, "host", ""), 255);
        calls.put(id, new CrewCall(id, name(o, "crew"), name(o, "from"), Protocol.num(o, "since", 0),
            Protocol.num(o, "until", 0),
            answers == null ? 0 : (int) Math.max(0, Protocol.num(answers, "coming", 0)),
            answers == null ? 0 : (int) Math.max(0, Protocol.num(answers, "soon", 0)),
            answers == null ? 0 : (int) Math.max(0, Protocol.num(answers, "cant", 0)),
            Protocol.str(o, "mine", null), Protocol.str(o, "where", null), host.isEmpty() ? null : host,
            Protocol.bool(o, "own", false)));
    }

    private void putMessage(JsonObject o, long now, boolean live) {
        String conv = Protocol.str(o, "conv", "");
        if (conv.isEmpty()) {
            return;
        }
        String id = Protocol.str(o, "id", null);
        String clientId = Protocol.str(o, "cid", null);
        String text = Protocol.plain(Protocol.str(o, "text", ""), Protocol.MAX_MESSAGE_CHARS);
        boolean edited = Protocol.bool(o, "edited", false);
        // Deleted for everyone (L16): the line stays as a quiet "Mesaj silindi" mark, like the launcher's.
        boolean deleted = Protocol.bool(o, "deleted", false);
        long at = Protocol.num(o, "at", now);
        Deque<ChatLine> deque = lines(conv);
        ChatLine known = findById(deque, clientId, id);
        if (known != null) {
            // The same message again: confirmed, edited, deleted, or with new reactions or its card.
            known.id = id == null ? known.id : id;
            if (deleted) {
                known.deleted = true;
                known.text = "";
                known.edited = false;
            } else {
                known.text = text.isEmpty() ? known.text : text;
                known.edited = edited;
            }
            known.at = at;
            known.delivery = Delivery.SENT;
            known.failure = null;
            putExtras(known, o);
            touch(conv);
            return;
        }
        if (deleted && live) {
            // The deletion of a message this game never showed: nothing to mark.
            return;
        }
        String from = name(o, "from");
        boolean mine = Protocol.bool(o, "mine", false);
        ChatLine line = new ChatLine(id, clientId, from, deleted ? "" : text, mine, Delivery.SENT, at, now);
        line.edited = edited && !deleted;
        line.deleted = deleted;
        putExtras(line, o);
        append(conv, line);
        if (live) {
            moveToFront(conv);
            if (!mine) {
                // Their message ends their "yazıyor…" at once.
                stopWriting(conv, from);
            }
        }
    }

    /** CHAT2's fields of a message (absent from an older launcher: the line keeps none). */
    private static void putExtras(ChatLine line, JsonObject o) {
        long sid = Protocol.id(o, "sid");
        line.sid = sid > 0 ? sid : line.sid;
        if (line.deleted) {
            line.sticker = null;
            line.reply = null;
            line.reactions = List.of();
            line.links = List.of();
            line.card = null;
            return;
        }
        String sticker = Protocol.str(o, "sticker", null);
        line.sticker = stickerRef(sticker) ? sticker : line.sticker;
        if (line.sticker != null && sticker != null) {
            // A sticker's words are for a mod that cannot draw it.
            line.text = "";
        }
        JsonObject reply = obj(o, "reply");
        if (reply != null) {
            String quotedSticker = Protocol.str(reply, "sticker", null);
            String quoted = Protocol.str(reply, "text", null);
            line.reply = new Reply(Protocol.str(reply, "id", null), Protocol.id(reply, "sid"), name(reply, "from"),
                Protocol.bool(reply, "mine", false),
                quoted == null ? null : Protocol.plain(quoted, MAX_QUOTE_CHARS),
                stickerRef(quotedSticker) ? quotedSticker : null, Protocol.bool(reply, "deleted", false),
                Protocol.bool(reply, "missing", false));
        } else if (o.has("sid")) {
            // The launcher's word stands (a reply whose message was gone went as a plain one).
            line.reply = null;
        }
        JsonArray reactions = arr(o, "reactions");
        if (reactions != null || o.has("sid")) {
            // A launcher that knows reactions sends the whole list each time (none: the field is left out).
            line.reactions = reactionsOf(reactions);
        }
        JsonArray links = arr(o, "links");
        if (links != null || o.has("sid")) {
            line.links = linksOf(links);
        }
        JsonObject card = obj(o, "card");
        if (card != null) {
            String site = Protocol.plain(Protocol.str(card, "site", ""), Protocol.MAX_NAME_CHARS);
            String title = Protocol.plain(Protocol.str(card, "title", ""), MAX_QUOTE_CHARS);
            line.card = site.isEmpty() || title.isEmpty() ? null : new Card(site, title);
        } else if (o.has("sid")) {
            line.card = null;
        }
    }

    private static List<Reaction> reactionsOf(JsonArray a) {
        if (a == null || a.isEmpty()) {
            return List.of();
        }
        List<Reaction> out = new ArrayList<>(Math.min(a.size(), REACTION_KINDS));
        for (JsonElement e : a) {
            if (!e.isJsonObject() || out.size() >= REACTION_KINDS) {
                continue;
            }
            JsonObject r = e.getAsJsonObject();
            String emoji = Protocol.str(r, "e", "");
            // Only what fits an emoji: a few code points, nothing that formats or breaks lines.
            String shown = Protocol.plain(emoji, 8);
            int count = (int) Math.max(0, Math.min(9999, Protocol.num(r, "n", 0)));
            if (emoji.isEmpty() || emoji.length() > 32 || shown.isEmpty() || count == 0) {
                continue;
            }
            List<String> who = new ArrayList<>(3);
            JsonArray names = arr(r, "who");
            if (names != null) {
                for (JsonElement n : names) {
                    if (n.isJsonPrimitive() && n.getAsJsonPrimitive().isString() && who.size() < 3) {
                        String name = Protocol.plain(n.getAsString(), Protocol.MAX_NAME_CHARS);
                        if (!name.isEmpty()) {
                            who.add(name);
                        }
                    }
                }
            }
            out.add(new Reaction(emoji, shown, count, Protocol.bool(r, "mine", false), List.copyOf(who)));
        }
        return List.copyOf(out);
    }

    /** The launcher's pressable links: only plain {@code https} addresses, at most three. */
    private static List<Link> linksOf(JsonArray a) {
        if (a == null || a.isEmpty()) {
            return List.of();
        }
        List<Link> out = new ArrayList<>(3);
        for (JsonElement e : a) {
            if (!e.isJsonObject() || out.size() >= 3) {
                continue;
            }
            JsonObject l = e.getAsJsonObject();
            String url = Protocol.str(l, "url", "");
            if (LinkRule.safe(url)) {
                out.add(new Link(url, Protocol.bool(l, "trusted", false)));
            }
        }
        return List.copyOf(out);
    }

    /**
     * The line that shows "Görüldü" (CHAT2 §5): the player's last sent message the friend's mark reaches, with
     * no message of the friend's after it (an answer says it was seen by itself). Null when there is none.
     */
    public static ChatLine seenLine(Iterable<ChatLine> lines, long seen) {
        if (seen <= 0) {
            return null;
        }
        ChatLine found = null;
        for (ChatLine line : lines) {
            if (!line.mine) {
                found = null;
                continue;
            }
            if (line.delivery != Delivery.SENT || line.sid <= 0 || line.deleted) {
                continue;
            }
            if (line.sid <= seen) {
                found = line;
            }
        }
        return found;
    }

    /**
     * Where "Yeni mesajlar" goes when a conversation opens with {@code unread} messages: over the oldest of the
     * newest that many from others (the player's own, sent or not, are never unread). With more unread than the
     * device keeps, over the first line; none for an empty conversation.
     */
    public static ChatLine firstUnread(List<ChatLine> lines, int unread) {
        int left = unread;
        for (int i = lines.size() - 1; i >= 0; i--) {
            if (!lines.get(i).mine && --left == 0) {
                return lines.get(i);
            }
        }
        return lines.isEmpty() ? null : lines.get(0);
    }

    /** New activity puts a conversation first (the panel keeps its own order while it is open). */
    private void moveToFront(String conv) {
        Conversation c = conversations.get(conv);
        if (c == null || conversations.keySet().iterator().next().equals(conv)) {
            return;
        }
        LinkedHashMap<String, Conversation> next = new LinkedHashMap<>();
        next.put(conv, c);
        for (Conversation other : conversations.values()) {
            if (!other.id().equals(conv)) {
                next.put(other.id(), other);
            }
        }
        conversations.clear();
        conversations.putAll(next);
    }

    private void append(String conversation, ChatLine line) {
        Deque<ChatLine> deque = lines(conversation);
        deque.addLast(line);
        trim(deque);
        touch(conversation);
    }

    private static void trim(Deque<ChatLine> deque) {
        while (deque.size() > MAX_LINES) {
            deque.removeFirst();
        }
    }

    private void touch(String conversation) {
        if (conversation != null) {
            lineRevisions.merge(conversation, 1, Integer::sum);
        }
        revision++;
    }

    private static boolean hasPending(Deque<ChatLine> deque) {
        if (deque == null) {
            return false;
        }
        for (ChatLine l : deque) {
            if (l.delivery == Delivery.PENDING) {
                return true;
            }
        }
        return false;
    }

    private static ChatLine findById(Deque<ChatLine> deque, String clientId, String id) {
        for (ChatLine line : deque) {
            if ((id != null && id.equals(line.id)) || (clientId != null && clientId.equals(line.clientId))) {
                return line;
            }
        }
        return null;
    }

    private ChatLine findByClientId(String clientId) {
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

    /** The conversation a line is in, or null. */
    public String conversationOf(ChatLine line) {
        for (Map.Entry<String, Deque<ChatLine>> entry : lines.entrySet()) {
            if (entry.getValue().contains(line)) {
                return entry.getKey();
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
