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
    /** "Ekibe katıldı" lines kept per crew conversation (the launcher keeps 30 a crew). */
    public static final int MAX_ARRIVALS = 30;

    /**
     * A friend. {@code badge} (TEAM6): the crew whose tag they wear, as the launcher named it (its card is in
     * {@link #badge(String)}), or null. {@code seen} (OVL-UX2, Q2): when an offline friend was last seen, as roughly as
     * the launcher's list says it ({@code today}, {@code yesterday}, {@code week}, {@code long}), or null. OVL-UX2
     * (Q3-Q5): {@code fav}, one of the player's favourites (the list's star); {@code alert}, "Çevrimiçi olunca bildir"
     * is on for them; {@code group}, their category's id (null: "Arkadaşlar"); {@code crews}, where they stand in the
     * crews the player may invite to ({@code member} or {@code invited}, by crew id).
     */
    public record Friend(String id, String name, String where, boolean joinable, String host, String badge,
        String seen, boolean fav, boolean alert, String group, Map<String, String> crews) {
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

    /**
     * How a friend menu action went (OVL-UX), said for a moment under the friend list: {@code key} names the words
     * ({@code people.note.<key>}) with the friend's {@code name}, or {@code text} is the line itself (world sharing's);
     * {@code error} when it did not go.
     */
    public record PersonNote(String key, String name, String text, boolean error, long at, String detail) {
    }

    /** One of the player's friend categories (OVL-UX2, Q7). */
    public record Group(String id, String name) {
    }

    /** A crew the player may invite friends to (OVL-UX2, Q7); {@code full}: nobody more can be invited. */
    public record CrewTarget(String id, String name, boolean full) {
    }

    /**
     * A crew tag's card (TEAM6): the chip ({@code tag} on the crew's colour) and the bubble it opens. {@code since}
     * is the month the crew was founded ("2026-09", empty when not known); {@code mine}: the player is in it.
     */
    public record Badge(String crew, String tag, String color, String frame, String emblem, String name,
        String motto, String since, int members, boolean mine) {
    }

    /** Someone came into a crew while the launcher ran (TEAM6 §7): "Ece ekibe katıldı", or the player's own. */
    public record Arrival(String id, String name, boolean mine, long at) {
    }

    /**
     * HOST1 (H8): a friend in the player's world session as the launcher mirrors it: the guest id once the friend's
     * launcher connected (else empty), the friend's id and shown name, the game name once known, and {@code invited},
     * {@code joining} or {@code in}.
     */
    public record HostGuest(String guest, String friend, String label, String name, String state) {
        public boolean in() {
            return state.equals("in");
        }

        public boolean joining() {
            return state.equals("joining");
        }
    }

    /**
     * HOST1 (H8): the player's world session, whole, on every change; {@code error} ({@code unavailable}, {@code
     * limit}) only when the world did not open.
     */
    public record HostView(boolean open, int max, List<HostGuest> guests, String error) {
        /** Guests in the world now. */
        public int inWorld() {
            int n = 0;
            for (HostGuest g : guests) {
                if (g.in()) {
                    n++;
                }
            }
            return n;
        }

        public HostGuest byGuest(String guestId) {
            for (HostGuest g : guests) {
                if (!guestId.isEmpty() && g.guest().equals(guestId)) {
                    return g;
                }
            }
            return null;
        }
    }

    /** HOST1 (H13): this friend invited the player to their open world: {@code guests + 1} of {@code max + 1}. */
    public record WorldMark(int guests, int max) {
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
        /** TEAM6: the crew whose tag its sender wears (someone else's message in crew chat), or null. */
        public String badge;
        /** OVL-UX2 (Q6): its sender's user id (someone else's message in crew chat; their name opens their menu). */
        public String by;

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

    /**
     * A toast the launcher asked for; the mod may still skip it (hidden GUI, conversation already open). {@code badge}
     * (TEAM6): the card of the tag a message's sender wears, while tags are on.
     */
    public record ToastRequest(String kind, String key, String title, String body, String conversation,
        String crew, String sticker, Badge badge) {
        public ToastRequest(String kind, String key, String title, String body, String conversation, String crew) {
            this(kind, key, title, body, conversation, crew, null, null);
        }
    }

    private final Map<String, Friend> friends = new LinkedHashMap<>();
    private final LinkedHashMap<String, Conversation> conversations = new LinkedHashMap<>();
    private final Map<String, Deque<ChatLine>> lines = new HashMap<>();
    private final Map<String, Integer> lineRevisions = new HashMap<>();
    private final Map<String, CrewCall> calls = new LinkedHashMap<>();
    private final Map<String, LinkedHashMap<String, Writer>> writers = new HashMap<>();
    private final Set<String> features = new HashSet<>();
    /** TEAM6: the cards of the tags friends and crewmates wear, by crew id. */
    private final Map<String, Badge> badges = new HashMap<>();
    /** TEAM6 §7: who came into each crew conversation, oldest first. */
    private final Map<String, List<Arrival>> arrivals = new HashMap<>();
    /** HOST1 (H13): friends who invited the player to their open world, by friend id. */
    private final Map<String, WorldMark> worlds = new HashMap<>();
    /** HOST1 (H8): the player's own world session as the launcher mirrors it, or null. */
    private HostView host;
    private int badgeRevision;

    private BridgeClient.Status status = BridgeClient.Status.CONNECTING;
    private String statusReason;
    private String me = "";
    private boolean welcomed;
    private boolean dnd;
    private boolean reducedMotion;
    private int revision;
    private int typingRevision;
    private Note note;
    // ---- The friend menu (OVL-UX) ----
    /** The crew whose tag the player wears (P9), or null. */
    private String meBadge;
    /** Friend id → their direct conversation (P10), while chat is on. */
    private final Map<String, String> directs = new HashMap<>();
    /** Friends whose Discord profile is known (P11). */
    private final Set<String> discords = new HashSet<>();
    /** Direct conversations the panel opened before they had a message, and whose they are. */
    private final LinkedHashMap<String, Conversation> drafts = new LinkedHashMap<>();
    private final Map<String, String> draftFriends = new HashMap<>();
    /** When "Birlikte oyna" last went to each friend (this computer's clock). */
    private final Map<String, Long> invitedAt = new HashMap<>();
    /** Friends reported with "Ayrıca engelle": their row goes, and the note says why. */
    private final Set<String> reportBlocks = new HashSet<>();
    /** The names of friends an action is on its way for (an unfriended one leaves the list before the answer). */
    private final Map<String, String> actionNames = new HashMap<>();
    /** What a note names besides the person (a category, a crew), by the person an action is on its way for. */
    private final Map<String, String> actionDetails = new HashMap<>();
    private PersonNote personNote;
    // ---- The rest of the launcher's person menu (OVL-UX2) ----
    private List<Group> groups = List.of();
    private List<CrewTarget> inviteCrews = List.of();
    private boolean crewmatesAsk;
    private final Set<String> asked = new HashSet<>();
    private final Set<String> asking = new HashSet<>();
    /** Q15: crewmates who are not friends, playing on Projects now ("Katıl"). */
    private final Set<String> matesHere = new HashSet<>();
    /** Q15: crewmates who are not friends, whose Discord profile is known ("Discord'dan mesaj at"). */
    private final Set<String> matesDiscord = new HashSet<>();

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

    /** Crew tags are on (TEAM6): the launcher's welcome says so (the service offers them, Studio has them on). */
    public boolean tagsOn() {
        return features.contains("tags");
    }

    /** The card of crew {@code crew}'s tag while tags are on, or null. */
    public Badge badge(String crew) {
        return crew == null || !tagsOn() ? null : badges.get(crew);
    }

    /** Bumps whenever a tag's card changes, comes or goes (what a laid-out line shows may change with it). */
    public int badgeRevision() {
        return badgeRevision;
    }

    /** Who came into {@code conversation} (a crew's chat) while the launcher ran, oldest first. */
    public List<Arrival> arrivals(String conversation) {
        List<Arrival> list = conversation == null ? null : arrivals.get(conversation);
        return list == null ? List.of() : list;
    }

    /** Bumps on every visible change so an open panel knows when to rebuild. */
    public int revision() {
        return revision;
    }

    /** HOST1: the player's world session as the launcher last mirrored it, or null (none, or it ended). */
    public HostView host() {
        return host;
    }

    /** HOST1: the session ended on this side (the player stopped it, the world closed); no mirror comes after. */
    public void hostEnded() {
        if (host != null) {
            host = null;
            revision++;
        }
    }

    /** HOST1 (H13): the open world friend {@code friendId} invited the player to, or null. */
    public WorldMark world(String friendId) {
        return friendId == null ? null : worlds.get(friendId);
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

    /** Newest activity first; a direct conversation the panel just opened for a friend (OVL-UX) before them. */
    public Collection<Conversation> conversations() {
        if (drafts.isEmpty()) {
            return conversations.values();
        }
        List<Conversation> all = new ArrayList<>(drafts.size() + conversations.size());
        for (Conversation draft : drafts.values()) {
            if (friends.containsKey(draftFriends.get(draft.id()))) {
                all.add(draft);
            }
        }
        all.addAll(conversations.values());
        return all;
    }

    public Conversation conversation(String id) {
        if (id == null) {
            return null;
        }
        Conversation c = conversations.get(id);
        if (c == null && (c = drafts.get(id)) != null && !friends.containsKey(draftFriends.get(id))) {
            // No longer a friend: nothing to write in.
            return null;
        }
        return c;
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

    // ---- The friend menu (OVL-UX) -----------------------------------------------------------------------

    /** "Birlikte oyna" to one friend waits this long after one went (the service's own five minutes). */
    public static final long INVITE_WAIT_MS = 5L * 60_000L;

    /** The crew whose tag the player wears while tags are on (its card: {@link #badge(String)}), or null. */
    public String meBadge() {
        return tagsOn() ? meBadge : null;
    }

    /** {@code friendId}'s direct conversation while chat is on (it may have no message yet), or null. */
    public String dm(String friendId) {
        return friendId == null || !has("chat") ? null : directs.get(friendId);
    }

    /** OVL-UX2: the friend whose direct conversation {@code conversation} is (its chip's menu is theirs), or null. */
    public String friendOfDirect(String conversation) {
        if (conversation == null) {
            return null;
        }
        for (Map.Entry<String, String> e : directs.entrySet()) {
            if (e.getValue().equals(conversation)) {
                return e.getKey();
            }
        }
        return draftFriends.get(conversation);
    }

    /** Whether {@code friendId}'s Discord profile is known ("Discord'dan mesaj at"). */
    public boolean discord(String friendId) {
        return friendId != null && discords.contains(friendId);
    }

    /**
     * {@code friendId}'s direct conversation, ready to show: the one there is, else an empty one until its first
     * message (the launcher sends the real one then). Null when they cannot be written to from here.
     */
    public String openDirect(String friendId) {
        String id = dm(friendId);
        Friend friend = friendId == null ? null : friends.get(friendId);
        if (id == null || friend == null) {
            return null;
        }
        if (!conversations.containsKey(id) && !drafts.containsKey(id)) {
            drafts.put(id, new Conversation(id, "dm", friend.name(), 0, false, true, 0));
            draftFriends.put(id, friendId);
            revision++;
        }
        return id;
    }

    /** Whether "Birlikte oyna" to {@code friendId} waits for its five minutes. */
    public boolean inviteWaiting(String friendId, long now) {
        Long at = friendId == null ? null : invitedAt.get(friendId);
        return at != null && now - at < INVITE_WAIT_MS;
    }

    /** An action for {@code friendId} goes now: their name is kept for the answer's words. */
    public void actionSent(String friendId) {
        Friend friend = friends.get(friendId);
        if (friend != null) {
            actionNames.put(friendId, friend.name());
        }
        actionDetails.remove(friendId);
    }

    /**
     * An action for {@code id} goes now (OVL-UX2): their name (a crewmate is not on the friend list) and what the
     * answer's words name besides them ({@code detail}: a category, a crew; may be null) are kept.
     */
    public void actionSent(String id, String name, String detail) {
        actionNames.put(id, name == null ? "" : name);
        if (detail == null) {
            actionDetails.remove(id);
        } else {
            actionDetails.put(id, detail);
        }
    }

    /** OVL-UX2 (Q7): the player's friend categories, in their order (empty: none, or an older service). */
    public List<Group> groups() {
        return groups;
    }

    /** OVL-UX2 (Q7): the crews the player may invite friends to (empty: none, or crews off). */
    public List<CrewTarget> inviteCrews() {
        return inviteCrews;
    }

    /** OVL-UX2 (Q7): crewmates may ask each other to be friends now. */
    public boolean crewmatesAsk() {
        return crewmatesAsk && has("people2");
    }

    /** OVL-UX2 (Q7): crewmate {@code id} asked the player to be friends. */
    public boolean askedBy(String id) {
        return id != null && asked.contains(id);
    }

    /** OVL-UX2 (Q7): the player asked crewmate {@code id} to be friends. */
    public boolean asking(String id) {
        return id != null && asking.contains(id);
    }

    /** OVL-UX2 (Q15): crewmate {@code id} (not a friend) plays on Projects now: their menu's "Katıl". */
    public boolean mateOnProjects(String id) {
        return id != null && has("people2") && matesHere.contains(id);
    }

    /** OVL-UX2 (Q15): crewmate {@code id}'s (not a friend) Discord profile is known: "Discord'dan mesaj at". */
    public boolean mateDiscord(String id) {
        return id != null && has("people2") && matesDiscord.contains(id);
    }

    /** A report about {@code friendId} went, {@code alsoBlock}: with "Ayrıca engelle" ticked. */
    public void reportSent(String friendId, boolean alsoBlock) {
        if (alsoBlock) {
            reportBlocks.add(friendId);
        } else {
            reportBlocks.remove(friendId);
        }
    }

    /** The note under the friend list now, or null. */
    public PersonNote personNote(long now) {
        return personNote != null && now - personNote.at() < NOTE_MS ? personNote : null;
    }

    /** Shows a note under the friend list for a moment (the mod's own, as when nothing could be sent). */
    public void personNote(String key, String name, boolean error, long now) {
        personNote(key, name, null, error, now);
    }

    /** {@link #personNote(String, String, boolean, long)} naming something besides the person (OVL-UX2). */
    public void personNote(String key, String name, String detail, boolean error, long now) {
        personNote = new PersonNote(key, name == null ? "" : name, null, error, now, detail);
        revision++;
    }

    /** Shows a line of world sharing's under the friend list for a moment (an invite to the world that did not go). */
    public void personNoteText(String text, boolean error, long now) {
        personNote = new PersonNote(null, "", text, error, now, null);
        revision++;
    }

    /** P10, P11: what the friend's frame says of their conversation and Discord. */
    private void putFriendExtras(String id, JsonObject o) {
        String dm = Protocol.str(o, "dm", "");
        if (dm.isEmpty() || dm.length() > 160) {
            directs.remove(id);
        } else {
            directs.put(id, dm);
        }
        if (Protocol.bool(o, "discord", false)) {
            discords.add(id);
        } else {
            discords.remove(id);
        }
    }

    /** Another account signed in: nothing of the previous one's menu stays. */
    private void forgetPeople() {
        drafts.clear();
        draftFriends.clear();
        invitedAt.clear();
        actionNames.clear();
        actionDetails.clear();
        reportBlocks.clear();
        personNote = null;
        groups = List.of();
        inviteCrews = List.of();
        crewmatesAsk = false;
        asked.clear();
        asking.clear();
        matesHere.clear();
        matesDiscord.clear();
    }

    /** P8: how a friend menu action went; the words come from the action and the reason. */
    private void onFriendResult(JsonObject frame, long now) {
        String friend = Protocol.str(frame, "friend", "");
        String action = Protocol.str(frame, "action", "");
        boolean ok = Protocol.bool(frame, "ok", false);
        String reason = Protocol.str(frame, "reason", "failed");
        Friend known = friends.get(friend);
        String name = actionNames.containsKey(friend) ? actionNames.remove(friend) : known == null ? ""
            : known.name();
        String detail = actionDetails.remove(friend);
        boolean queued = Protocol.bool(frame, "queued", false);
        String key = switch (action) {
            case "invite" -> {
                if (ok || reason.equals("wait")) {
                    // As the launcher: the next one to them waits five minutes either way.
                    invitedAt.put(friend, now);
                }
                yield ok ? (Protocol.bool(frame, "queued", false) ? "invite_queued" : "invited")
                    : reason.equals("wait") ? "invite_wait" : "invite_failed";
            }
            case "unfriend" -> ok ? "removed" : "failed";
            case "block" -> ok ? "blocked" : "failed";
            case "report" -> {
                // With "Ayrıca engelle" their row goes too: the note says both.
                boolean blocked = reportBlocks.remove(friend);
                yield ok ? (blocked ? "reported_blocked" : "reported") : "report_failed";
            }
            case "discord" -> ok ? null : "failed";
            // OVL-UX2: a mark that did not keep, a category, a crew invite, a crewmate's request.
            case "favorite", "alert" -> ok ? null : "failed";
            case "move" -> ok ? (detail == null ? null : "moved") : "failed";
            case "crew-invite" -> {
                if (ok && detail != null) {
                    yield "crew_invited";
                }
                yield ok ? "crew_invited_one" : "failed";
            }
            case "add-friend" -> {
                if (ok && queued) {
                    asking.add(friend);
                }
                yield ok ? (queued ? "request_sent" : "now_friends") : "failed";
            }
            default -> null;
        };
        if (key != null) {
            personNote = new PersonNote(key, name, null, !ok, now, detail);
        }
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
                boolean otherAccount = welcomed && !name.equals(me);
                if (welcomed && !name.equals(me)) {
                    // Another account: nothing of the previous one stays on screen.
                    lines.clear();
                    lineRevisions.clear();
                    friends.clear();
                    conversations.clear();
                    calls.clear();
                    writers.clear();
                    badges.clear();
                    arrivals.clear();
                    worlds.clear();
                    host = null;
                    note = null;
                }
                // Tags may have come or gone with the features: what is laid out is laid out again.
                badgeRevision++;
                me = name;
                // OVL-UX (P9): the tag the player wears, before their name in the panel's header.
                meBadge = self == null ? null : crewId(self);
                if (otherAccount) {
                    forgetPeople();
                }
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
                worlds.clear();
                forEach(frame, "friends", this::putFriend);
                LinkedHashMap<String, Conversation> next = new LinkedHashMap<>();
                forEachConversation(frame, next);
                conversations.clear();
                conversations.putAll(next);
                // A conversation the panel opened before its first message is the real one now (OVL-UX).
                drafts.keySet().removeIf(conversations::containsKey);
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
            case "friend-gone" -> {
                friends.remove(Protocol.str(frame, "id", ""));
                worlds.remove(Protocol.str(frame, "id", ""));
            }
            case "host-state" -> putHost(frame);
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
            case "badges" -> putBadges(frame);
            case "note" -> putArrival(frame);
            case "friend-result" -> onFriendResult(frame, now);
            case "lists" -> putLists(frame);
            case "toast" -> {
                String sticker = Protocol.str(frame, "sticker", null);
                String kind = Protocol.str(frame, "kind", "message");
                toast = new ToastRequest(
                    kind,
                    Protocol.plain(Protocol.str(frame, "key", Protocol.str(frame, "id", "")), 64),
                    name(frame, "title"),
                    Protocol.plain(Protocol.str(frame, "body", ""), Protocol.MAX_MESSAGE_CHARS),
                    Protocol.str(frame, "conv", null),
                    Protocol.plain(Protocol.str(frame, "crew", ""), Protocol.MAX_NAME_CHARS),
                    sticker == null || !stickerRef(sticker) ? null : sticker,
                    // "[GK] Ayşe": only a message's sender wears one.
                    kind.equals("message") ? badge(Protocol.str(frame, "badge", null)) : null);
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
        putFriendExtras(id, o);

        String host = Protocol.plain(Protocol.str(o, "host", ""), 255);
        friends.put(id, new Friend(id, name(o, "name"), Protocol.str(o, "where", "offline"),
            Protocol.bool(o, "joinable", false), host.isEmpty() ? null : host, crewId(o), seenOf(o),
            Protocol.bool(o, "fav", false), Protocol.bool(o, "alert", false), groupId(o), crewPlaces(o)));
        // HOST1 (H13): only the launcher that got the invite sends it; gone with the next frame without it.
        JsonObject world = obj(o, "world");
        if (world == null) {
            worlds.remove(id);
        } else {
            worlds.put(id, new WorldMark((int) Math.max(0, Math.min(7, Protocol.num(world, "guests", 0))),
                (int) Math.max(1, Math.min(7, Protocol.num(world, "max", 7)))));
        }
    }

    /** HOST1 (H8): the launcher's mirror of the player's world session. */
    private void putHost(JsonObject frame) {
        boolean open = Protocol.bool(frame, "open", false);
        String error = Protocol.str(frame, "error", null);
        List<HostGuest> guests = new ArrayList<>();
        forEach(frame, "guests", g -> {
            String friend = Protocol.str(g, "friend", "");
            if (friend.isEmpty() || guests.size() >= 7) {
                return;
            }
            String state = Protocol.str(g, "state", "invited");
            guests.add(new HostGuest(Protocol.plain(Protocol.str(g, "guest", ""), 8), friend, name(g, "label"),
                Protocol.plain(Protocol.str(g, "name", ""), 16),
                state.equals("in") || state.equals("joining") ? state : "invited"));
        });
        host = new HostView(open, (int) Math.max(1, Math.min(7, Protocol.num(frame, "max", 7))), List.copyOf(guests),
            open ? null : (error == null ? "unavailable" : error));
    }

    /** Q4: a friend's category id ({@code [a-z0-9]}, at most 12, as the launcher makes them), or null. */
    private static String groupId(JsonObject o) {
        String group = Protocol.str(o, "group", "");
        return group.matches("[a-z0-9]{1,12}") ? group : null;
    }

    /** Q5: where a friend stands in the crews the player may invite to: {@code member} or {@code invited}. */
    private static Map<String, String> crewPlaces(JsonObject o) {
        JsonObject crews = obj(o, "crews");
        if (crews == null || crews.isEmpty()) {
            return Map.of();
        }
        Map<String, String> out = new HashMap<>();
        for (Map.Entry<String, JsonElement> e : crews.entrySet()) {
            String place = e.getValue().isJsonPrimitive() && e.getValue().getAsJsonPrimitive().isString()
                ? e.getValue().getAsString() : "";
            if (!e.getKey().isEmpty() && e.getKey().length() <= 64 && (place.equals("member")
                || place.equals("invited")) && out.size() < 16) {
                out.put(e.getKey(), place);
            }
        }
        return Map.copyOf(out);
    }

    /** Q7: what the person menu's rows choose from, whole (categories, crews to invite to, crewmates' requests). */
    private void putLists(JsonObject frame) {
        List<Group> nextGroups = new ArrayList<>();
        forEach(frame, "groups", g -> {
            String id = Protocol.str(g, "id", "");
            String name = Protocol.plain(Protocol.str(g, "name", ""), 24);
            if (id.matches("[a-z0-9]{1,12}") && !name.isEmpty() && nextGroups.size() < 20) {
                nextGroups.add(new Group(id, name));
            }
        });
        List<CrewTarget> nextCrews = new ArrayList<>();
        forEach(frame, "crews", c -> {
            String id = Protocol.str(c, "id", "");
            if (!id.isEmpty() && id.length() <= 64 && nextCrews.size() < 10) {
                nextCrews.add(new CrewTarget(id, name(c, "name"), Protocol.bool(c, "full", false)));
            }
        });
        groups = List.copyOf(nextGroups);
        inviteCrews = List.copyOf(nextCrews);
        crewmatesAsk = Protocol.bool(frame, "crewmates", false);
        asked.clear();
        asking.clear();
        putIds(arr(frame, "asked"), asked);
        putIds(arr(frame, "asking"), asking);
        // Q15: crewmates who are not friends, on Projects or with a known Discord profile.
        matesHere.clear();
        matesDiscord.clear();
        putIds(arr(frame, "projects"), matesHere);
        putIds(arr(frame, "discord"), matesDiscord);
    }

    private static void putIds(JsonArray list, Set<String> into) {
        if (list == null) {
            return;
        }
        for (JsonElement e : list) {
            if (e.isJsonPrimitive() && e.getAsJsonPrimitive().isString() && into.size() < 200) {
                String id = e.getAsString();
                if (!id.isEmpty() && id.length() <= 64) {
                    into.add(id);
                }
            }
        }
    }

    /** Q6: the user id a message's sender has (someone else's message in crew chat), or null. */
    private static String senderId(JsonObject o) {
        String by = Protocol.str(o, "by", "");
        return by.isEmpty() || by.length() > 64 ? null : by;
    }

    /** Q2: an offline friend's last seen, one of the launcher's rough words; anything else is not shown. */
    private static String seenOf(JsonObject o) {
        String seen = Protocol.str(o, "seen", "");
        return switch (seen) {
            case "today", "yesterday", "week", "long" -> seen;
            default -> null;
        };
    }

    /** A crew id a frame names in {@code badge} (TEAM6), or null. */
    private static String crewId(JsonObject o) {
        String crew = Protocol.str(o, "badge", "");
        return crew.isEmpty() || crew.length() > 64 ? null : crew;
    }

    /** The launcher's crew tag cards (TEAM6, T3): the whole set with {@code all}, else changes; null takes one away. */
    private void putBadges(JsonObject frame) {
        JsonObject cards = obj(frame, "cards");
        if (Protocol.bool(frame, "all", false)) {
            badges.clear();
        }
        if (cards != null) {
            for (Map.Entry<String, JsonElement> e : cards.entrySet()) {
                String crew = e.getKey();
                if (crew.isEmpty() || crew.length() > 64) {
                    continue;
                }
                Badge card = e.getValue().isJsonObject() ? badgeOf(crew, e.getValue().getAsJsonObject()) : null;
                if (card == null) {
                    badges.remove(crew);
                } else {
                    badges.put(crew, card);
                }
            }
        }
        badgeRevision++;
    }

    /** A card as the panel shows it: plain text only, a tag of at most four characters; no tag, no card. */
    private static Badge badgeOf(String crew, JsonObject o) {
        String tag = Protocol.plain(Protocol.str(o, "tag", ""), 4);
        if (tag.isEmpty()) {
            return null;
        }
        String since = Protocol.str(o, "since", "");
        return new Badge(crew, tag, Protocol.plain(Protocol.str(o, "color", ""), 16),
            Protocol.plain(Protocol.str(o, "frame", ""), 16), Protocol.plain(Protocol.str(o, "emblem", ""), 16),
            name(o, "name"), Protocol.plain(Protocol.str(o, "motto", ""), 60),
            since.matches("\\d{4}-\\d{2}") ? since : "", (int) Math.max(0, Math.min(999, Protocol.num(o, "members", 0))),
            Protocol.bool(o, "mine", false));
    }

    /** "Ece ekibe katıldı" (TEAM6 §7, T7): kept per crew conversation, oldest first, at most {@link #MAX_ARRIVALS}. */
    private void putArrival(JsonObject frame) {
        String conv = Protocol.str(frame, "conv", "");
        String id = Protocol.str(frame, "id", "");
        if (conv.isEmpty() || id.isEmpty() || !"joined".equals(Protocol.str(frame, "kind", ""))) {
            return;
        }
        List<Arrival> list = arrivals.computeIfAbsent(conv, k -> new ArrayList<>());
        for (Arrival a : list) {
            if (a.id().equals(id)) {
                return;
            }
        }
        Arrival arrival = new Arrival(id, name(frame, "name"), Protocol.bool(frame, "mine", false),
            Protocol.num(frame, "at", 0));
        int at = list.size();
        while (at > 0 && list.get(at - 1).at() > arrival.at()) {
            at--;
        }
        list.add(at, arrival);
        while (list.size() > MAX_ARRIVALS) {
            list.remove(0);
        }
        touch(conv);
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
        // The real one of a conversation the panel opened before its first message (OVL-UX).
        drafts.remove(c.id());
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

    /** CHAT2's fields of a message (absent from an older launcher: the line keeps none), and TEAM6's tag. */
    private static void putExtras(ChatLine line, JsonObject o) {
        long sid = Protocol.id(o, "sid");
        line.sid = sid > 0 ? sid : line.sid;
        // The launcher's word on the sender's tag stands each time the message comes (it took it off: none).
        line.badge = line.mine ? null : crewId(o);
        line.by = line.mine ? null : senderId(o);
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
