package gg.projects.overlay.voice;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Deque;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * Voice chat as the launcher shows it to the game (VOICE-O; docs/tasks/ovl2-protocol.md rows V), built only from its
 * frames: whether it may be used now, the player's room with everyone in it, the call that rings at a friend, the rings
 * for the player, who speaks. The launcher keeps the real state; this is a small mirror for one game. Lives on the render
 * thread; no game classes, so it is tested on its own.
 *
 * <p>A {@code voice} frame replaces everything; {@code voice-speaking} changes only who speaks; {@code voice-notice} and
 * {@code voice-result} wait here until the game says them ({@link #takeNotice}, {@link #takeResult}). OVL-SOC:
 * {@code voice-crews} replaces the player's crews' rooms (crew chat's "Sesli").
 */
public final class VoiceState {
    /** A person's volume at most, in percent (the launcher's slider). */
    public static final int MAX_VOLUME = 200;
    /** The steps "Ses seviyesi" offers, in percent (100: as they come). */
    public static final int[] VOLUME_STEPS = {25, 50, 75, 100, 125, 150, 200};
    /** People in a room at most (the service's largest room), and names a frame carries for a ring. */
    static final int MAX_PEOPLE = 64;
    static final int MAX_WITH = 4;
    /** Notices kept until said (a burst of them says the newest few). */
    static final int MAX_NOTICES = 4;
    /** Room ids and person ids are short; anything longer is not the launcher's. */
    static final int MAX_ID = 64;
    /** Crews and rooms a crew a {@code voice-crews} frame carries at most (the service: 5 crews, 5 rooms). */
    static final int MAX_CREWS = 10;
    static final int MAX_CREW_ROOMS = 8;
    /** The player's shares a {@code voice-share} frame carries at most (the service allows up to 3). */
    static final int MAX_SHARES = 4;

    /** Someone else in the player's room. */
    public record Person(String id, String name, String role, boolean mute, boolean deaf, boolean mutedByModerator,
        String quality, boolean speaking, int volume, boolean mutedForMe, boolean canMute, boolean canKick,
        boolean canMod) {
        public boolean leader() {
            return "leader".equals(role);
        }

        public boolean moderator() {
            return "moderator".equals(role);
        }

        /** The connection is not good ("Bağlantısı zayıf"). */
        public boolean weak() {
            return !"good".equals(quality);
        }

        Person speaking(boolean now) {
            return now == speaking ? this : new Person(id, name, role, mute, deaf, mutedByModerator, quality, now,
                volume, mutedForMe, canMute, canKick, canMod);
        }
    }

    /** The player in the room. */
    public record Me(String role, boolean mute, boolean deaf, boolean mutedByModerator, boolean talking,
        String quality) {
        /** Nothing of the player's goes out: muted, deafened or muted by a moderator. */
        public boolean micOff() {
            return mute || deaf || mutedByModerator;
        }

        public boolean weak() {
            return !"good".equals(quality);
        }

        Me talking(boolean now) {
            return now == talking ? this : new Me(role, mute, deaf, mutedByModerator, now, quality);
        }
    }

    /** Someone named by id with the name the friend lists know (empty: "Bir oyuncu"). */
    public record Named(String id, String name) {
    }

    /** A call or invite ringing for the player; {@code until} in Unix milliseconds. */
    public record Ring(String room, String from, String name, List<String> with, long until) {
    }

    /** The player's room. */
    public record Room(String id, String kind, String name, boolean general, String crew, String phase, int max, Me me,
        List<Person> people, List<Named> ringing) {
        public boolean crewRoom() {
            return "crew".equals(kind);
        }

        /** Getting in ("Bağlanıyor…"). */
        public boolean joining() {
            return "joining".equals(phase);
        }

        /** The connection dropped for a while ("Yeniden bağlanıyor…"). */
        public boolean reconnecting() {
            return "reconnecting".equals(phase);
        }

        public Person person(String id) {
            for (Person p : people) {
                if (p.id.equals(id)) {
                    return p;
                }
            }
            return null;
        }

        /** No place left: the people, the player and the invites still ringing fill it (the launcher's rule). */
        public boolean full() {
            return people.size() + 1 + ringing.size() >= max;
        }
    }

    /**
     * One of a crew's voice rooms (OVL-SOC, V21): {@code name} null for the crew's "Genel" ({@code general}), who is in
     * it (the player too, when there); OVL-SOC 2 (VOICE-ROOMS): {@code max} its own people limit below the crew's (0:
     * none, the pill says no count), {@code full} nobody more fits ("Oda dolu.").
     */
    public record CrewRoom(String id, String name, boolean general, List<Named> people, int max, boolean full) {
    }

    /**
     * One of the player's crews' rooms (V21): the crew, its chat ({@code c:<crew>}), its rooms in the launcher's order
     * (with nobody in any, its "Genel" alone), and whether "Yeni oda" may show.
     */
    public record CrewRooms(String crew, String conv, List<CrewRoom> rooms, boolean add) {
    }

    /** A voice chat moment the game says (V4). */
    public record Notice(String kind, String reason, String name) {
    }

    /** How one of the player's presses went (V5). */
    public record Result(String act, boolean ok, String reason, String who) {
    }

    /**
     * One of the player's screen shares (OVL-SHARE): its number, what it shows ({@code game}, {@code window},
     * {@code screen}), whether the room took it ("Yayında"; false while it starts) and how many watch.
     */
    public record MyShare(int n, String kind, boolean live, int watchers) {
        public boolean game() {
            return "game".equals(kind);
        }
    }

    /** What someone's menu offers of voice chat (the launcher's {@code callOffer}). */
    public enum Offer {
        NONE,
        /** "Ara". */
        CALL,
        /** "Odaya davet et". */
        INVITE,
        /** The player's call or invite rings at them ("Aranıyor…", off). */
        RINGING,
        /** The room has no place left ("Odaya davet et", off). */
        FULL
    }

    private boolean heard;
    private boolean on;
    private Room room;
    private Named calling;
    private List<Ring> rings = List.of();
    private List<CrewRooms> crews = List.of();
    private boolean sound;
    private int volumeMax = MAX_VOLUME;
    private int revision;
    private int speakingRevision;
    private final Deque<Notice> notices = new ArrayDeque<>();
    private Result result;
    // OVL-SHARE: screen sharing in the room ({@code voice-share}).
    private String shareGame = "";
    private String shareWhy = "";
    private List<MyShare> myShares = List.of();
    private Set<String> sharing = Set.of();

    /**
     * Applies a launcher frame of voice chat's ({@code voice}, {@code voice-speaking}, {@code voice-notice},
     * {@code voice-result}); false for any other frame.
     */
    public boolean apply(JsonObject frame) {
        switch (Protocol.type(frame)) {
            case "voice" -> putView(frame);
            case "voice-speaking" -> {
                // Up to ten a second: only the rings move, nothing is laid out again.
                putSpeaking(frame);
                speakingRevision++;
                return true;
            }
            case "voice-notice" -> {
                String kind = id(frame, "kind");
                if (kind.isEmpty()) {
                    return true;
                }
                if (notices.size() >= MAX_NOTICES) {
                    notices.removeFirst();
                }
                notices.addLast(new Notice(kind, id(frame, "reason"), name(frame, "name")));
            }
            case "voice-result" -> result = new Result(id(frame, "act"), Protocol.bool(frame, "ok", false),
                id(frame, "reason"), id(frame, "who"));
            case "voice-crews" -> crews = crewsOf(arr(frame, "crews"));
            case "voice-share" -> putShare(frame);
            default -> {
                return false;
            }
        }
        revision++;
        return true;
    }

    /** The launcher went away: nothing of voice chat stays on screen until it speaks again. */
    public void clear() {
        if (!heard && room == null && rings.isEmpty() && notices.isEmpty() && result == null && crews.isEmpty()
            && shareGame.isEmpty() && myShares.isEmpty() && sharing.isEmpty()) {
            return;
        }
        shareGame = "";
        shareWhy = "";
        myShares = List.of();
        sharing = Set.of();
        heard = false;
        on = false;
        room = null;
        calling = null;
        rings = List.of();
        crews = List.of();
        sound = false;
        volumeMax = MAX_VOLUME;
        notices.clear();
        result = null;
        revision++;
    }

    /** Changes with every frame but the speakers'; the panel lays out again when it moved. */
    public int revision() {
        return revision;
    }

    /** Changes with every change of who speaks (the rings follow it without a layout). */
    public int speakingRevision() {
        return speakingRevision;
    }

    /**
     * Voice chat is something in this game: it may be used, the player is in a room or a ring rings (the settings row
     * shows). A launcher whose voice chat is off says only an empty {@code voice}: nothing of it shows.
     */
    public boolean heard() {
        return heard;
    }

    /** Voice chat may be used now: "Ara" and "Odaya davet et" may show. */
    public boolean on() {
        return on;
    }

    public Room room() {
        return room;
    }

    public boolean inRoom() {
        return room != null;
    }

    /** The friend the player's call rings at, or null. */
    public Named calling() {
        return calling;
    }

    /** Rings for the player still ringing at {@code now} (Unix milliseconds), oldest first. */
    public List<Ring> rings(long now) {
        if (rings.isEmpty()) {
            return rings;
        }
        List<Ring> out = new ArrayList<>(rings.size());
        for (Ring r : rings) {
            if (r.until > now) {
                out.add(r);
            }
        }
        return out;
    }

    public Ring ring(String room) {
        for (Ring r : rings) {
            if (r.room.equals(room)) {
                return r;
            }
        }
        return null;
    }

    /** The game rings for a ring: the launcher's window is away, so nothing of the launcher's rings. */
    public boolean sound() {
        return sound;
    }

    /**
     * The rooms of the crew whose chat is {@code conv} (OVL-SOC: crew chat's "Sesli"), or null: no crew chat, voice chat
     * not on, or a launcher that says nothing of crews' rooms.
     */
    public CrewRooms crewRooms(String conv) {
        if (!on || conv == null) {
            return null;
        }
        for (CrewRooms c : crews) {
            if (c.conv.equals(conv)) {
                return c;
            }
        }
        return null;
    }

    /** Whether the player is in the room {@code id} (crew chat's "Sesli" lights it). */
    public boolean inRoom(String id) {
        return room != null && id != null && room.id.equals(id);
    }

    /** Who speaks now (the people of the room, by id). */
    public Set<String> speakers() {
        if (room == null) {
            return Set.of();
        }
        Set<String> out = new HashSet<>();
        for (Person p : room.people) {
            if (p.speaking) {
                out.add(p.id);
            }
        }
        return out;
    }

    /**
     * OVL-SHARE: "Oyunu paylaş": {@code ready}, {@code off} (dim, {@link #shareWhy} says why), or empty while it does not
     * show (the room does not share, this computer cannot, the game is shared already).
     */
    public String shareGame() {
        return shareGame;
    }

    /** Why "Oyunu paylaş" is dim: {@code not-allowed}, {@code closed}, {@code full}; empty otherwise. */
    public String shareWhy() {
        return shareWhy;
    }

    /** The player's own shares, in the launcher's order. */
    public List<MyShare> myShares() {
        return myShares;
    }

    /** The player shares something the room took ("Yayında"). */
    public boolean live() {
        for (MyShare m : myShares) {
            if (m.live) {
                return true;
            }
        }
        return false;
    }

    /** How many watch the player's shares, all together. */
    public int watchers() {
        int out = 0;
        for (MyShare m : myShares) {
            out += m.watchers;
        }
        return out;
    }

    /** Whether someone else in the room shares now ("Yayında" on their row). */
    public boolean sharing(String id) {
        return id != null && sharing.contains(id);
    }

    /** The oldest notice not said yet, or null. */
    public Notice takeNotice() {
        return notices.pollFirst();
    }

    /** The newest press result not said yet, or null. */
    public Result takeResult() {
        Result r = result;
        result = null;
        return r;
    }

    /**
     * What a friend's menu offers ({@code presence}: their voice chat, {@code ready} or {@code in}; null: they cannot be
     * called). As the launcher: "Ara" out of a room, "Odaya davet et" into the player's friend room; nothing for someone
     * in the room. A crew room's invites (the crew's members) and a call that leaves it stay in the launcher, which has
     * the crew's lists.
     */
    public Offer offer(String friendId, String presence) {
        if (!on || presence == null || friendId == null) {
            return Offer.NONE;
        }
        if (calling != null && calling.id.equals(friendId)) {
            return Offer.RINGING;
        }
        if (room == null) {
            return Offer.CALL;
        }
        if (room.person(friendId) != null) {
            return Offer.NONE;
        }
        for (Named r : room.ringing) {
            if (r.id.equals(friendId)) {
                return Offer.RINGING;
            }
        }
        if (room.crewRoom()) {
            return Offer.NONE;
        }
        return room.full() ? Offer.FULL : Offer.INVITE;
    }

    /**
     * The volume steps to offer for someone at {@code current}, up to {@code max} (the voice page's top): the steps, and
     * their own value where it is none of them.
     */
    public static List<Integer> volumeChoices(int current, int max) {
        List<Integer> out = new ArrayList<>(VOLUME_STEPS.length + 1);
        boolean placed = current < 0 || current > max;
        for (int step : VOLUME_STEPS) {
            if (step > max) {
                break;
            }
            if (!placed && current < step) {
                out.add(current);
                placed = true;
            }
            if (step == current) {
                placed = true;
            }
            out.add(step);
        }
        if (!placed) {
            out.add(current);
        }
        return out;
    }

    /** A person's volume at most now (percent): 100 while the voice page plays through {@code <audio>}, else 200. */
    public int volumeMax() {
        return volumeMax;
    }

    /** The words a room's title needs, in the overlay's language. */
    public interface Words {
        /** The crew's "Genel". */
        String general();

        /** "Yalnız sen". */
        String justYou();

        /** "Bir oyuncu". */
        String someone();

        /** "A, B ve 3 kişi daha". */
        String more(String names, int count);
    }

    /** Someone's name as shown: theirs, else "Bir oyuncu". */
    public static String shown(String name, Words words) {
        return name == null || name.isEmpty() ? words.someone() : name;
    }

    /** Names in a line: "A, B, C", or "A, B ve 2 kişi daha" past three (the launcher's {@code namesLine}). */
    public static String names(List<String> names, Words words) {
        if (names.size() <= 3) {
            return String.join(", ", names);
        }
        return words.more(String.join(", ", names.subList(0, 2)), names.size() - 2);
    }

    /**
     * A room's title (the launcher's {@code roomTitle}): the crew's "Genel", its name, who is in it, the friend being
     * called while the player waits alone, else "Yalnız sen".
     */
    public static String title(Room room, Named calling, Words words) {
        if (room.general) {
            return words.general();
        }
        if (room.name != null && !room.name.isEmpty()) {
            return room.name;
        }
        if (!room.people.isEmpty()) {
            List<String> names = new ArrayList<>(room.people.size());
            for (Person p : room.people) {
                names.add(shown(p.name, words));
            }
            return names(names, words);
        }
        if (calling != null) {
            return shown(calling.name, words);
        }
        return words.justYou();
    }

    // ---- Frames -----------------------------------------------------------------------------------------

    private void putView(JsonObject frame) {
        on = Protocol.bool(frame, "on", false);
        sound = Protocol.bool(frame, "sound", false);
        // At least "as they come" (100): a lower top would be no launcher's.
        volumeMax = (int) Math.max(100, Math.min(MAX_VOLUME, Protocol.num(frame, "volumeMax", MAX_VOLUME)));
        JsonObject r = obj(frame, "room");
        room = r == null ? null : roomOf(r);
        JsonObject c = obj(frame, "calling");
        calling = c == null ? null : named(c);
        List<Ring> next = new ArrayList<>();
        JsonArray list = arr(frame, "rings");
        if (list != null) {
            for (JsonElement e : list) {
                if (e.isJsonObject() && next.size() < MAX_PEOPLE) {
                    JsonObject o = e.getAsJsonObject();
                    String id = id(o, "room");
                    if (!id.isEmpty()) {
                        next.add(new Ring(id, id(o, "from"), name(o, "name"),
                            strings(arr(o, "with"), MAX_WITH, Protocol.MAX_NAME_CHARS), Protocol.num(o, "until", 0L)));
                    }
                }
            }
        }
        rings = Collections.unmodifiableList(next);
        heard = on || room != null || !rings.isEmpty();
    }

    private static Room roomOf(JsonObject r) {
        JsonObject m = obj(r, "me");
        Me me = m == null ? new Me("member", false, false, false, false, "good")
            : new Me(word(m, "role", "member"), Protocol.bool(m, "mute", false), Protocol.bool(m, "deaf", false),
                Protocol.bool(m, "mutedByModerator", false), Protocol.bool(m, "talking", false),
                word(m, "quality", "good"));
        List<Person> people = new ArrayList<>();
        JsonArray list = arr(r, "people");
        if (list != null) {
            for (JsonElement e : list) {
                if (!e.isJsonObject() || people.size() >= MAX_PEOPLE) {
                    continue;
                }
                JsonObject p = e.getAsJsonObject();
                String id = id(p, "id");
                if (id.isEmpty()) {
                    continue;
                }
                JsonObject can = obj(p, "can");
                int volume = (int) Math.max(0, Math.min(MAX_VOLUME, Protocol.num(p, "volume", 100)));
                people.add(new Person(id, name(p, "name"), word(p, "role", "member"), Protocol.bool(p, "mute", false),
                    Protocol.bool(p, "deaf", false), Protocol.bool(p, "mutedByModerator", false),
                    word(p, "quality", "good"), Protocol.bool(p, "speaking", false), volume,
                    Protocol.bool(p, "mutedForMe", false), can != null && Protocol.bool(can, "mute", false),
                    can != null && Protocol.bool(can, "kick", false), can != null && Protocol.bool(can, "mod", false)));
            }
        }
        List<Named> ringing = new ArrayList<>();
        JsonArray out = arr(r, "ringing");
        if (out != null) {
            for (JsonElement e : out) {
                if (e.isJsonObject() && ringing.size() < MAX_PEOPLE) {
                    Named n = named(e.getAsJsonObject());
                    if (!n.id.isEmpty()) {
                        ringing.add(n);
                    }
                }
            }
        }
        String name = name(r, "name");
        String crew = name(r, "crew");
        return new Room(id(r, "id"), word(r, "kind", "friend"), name.isEmpty() ? null : name,
            Protocol.bool(r, "general", false), crew.isEmpty() ? null : crew, word(r, "phase", "live"),
            (int) Math.max(2, Math.min(MAX_PEOPLE, Protocol.num(r, "max", 10))), me,
            Collections.unmodifiableList(people), Collections.unmodifiableList(ringing));
    }

    /** OVL-SHARE: {@code voice-share}, whole. */
    private void putShare(JsonObject frame) {
        String game = id(frame, "game");
        shareGame = game.equals("ready") || game.equals("off") ? game : "";
        shareWhy = shareGame.equals("off") ? id(frame, "why") : "";
        List<MyShare> mine = new ArrayList<>();
        JsonArray list = arr(frame, "mine");
        if (list != null) {
            for (JsonElement e : list) {
                if (e.isJsonObject() && mine.size() < MAX_SHARES) {
                    JsonObject o = e.getAsJsonObject();
                    int n = (int) Protocol.num(o, "n", 0);
                    if (n > 0 && n < 256) {
                        mine.add(new MyShare(n, word(o, "kind", "window"), Protocol.bool(o, "live", false),
                            (int) Math.max(0, Math.min(999, Protocol.num(o, "watchers", 0)))));
                    }
                }
            }
        }
        myShares = Collections.unmodifiableList(mine);
        Set<String> live = new HashSet<>();
        JsonArray others = arr(frame, "live");
        if (others != null) {
            for (JsonElement e : others) {
                if (e.isJsonObject() && live.size() < MAX_PEOPLE) {
                    String person = id(e.getAsJsonObject(), "person");
                    if (!person.isEmpty()) {
                        live.add(person);
                    }
                }
            }
        }
        sharing = Collections.unmodifiableSet(live);
    }

    private void putSpeaking(JsonObject frame) {
        if (room == null) {
            return;
        }
        Set<String> ids = new HashSet<>(strings(arr(frame, "ids"), MAX_PEOPLE, MAX_ID));
        List<Person> people = new ArrayList<>(room.people.size());
        for (Person p : room.people) {
            people.add(p.speaking(ids.contains(p.id)));
        }
        room = new Room(room.id, room.kind, room.name, room.general, room.crew, room.phase, room.max,
            room.me.talking(Protocol.bool(frame, "me", false)), Collections.unmodifiableList(people), room.ringing);
    }

    private static Named named(JsonObject o) {
        return new Named(id(o, "id"), name(o, "name"));
    }

    /** {@code voice-crews}' list (V21): crews with an id and a chat, rooms with an id, people with an id. */
    private static List<CrewRooms> crewsOf(JsonArray list) {
        if (list == null) {
            return List.of();
        }
        List<CrewRooms> out = new ArrayList<>();
        for (JsonElement e : list) {
            if (!e.isJsonObject() || out.size() >= MAX_CREWS) {
                continue;
            }
            JsonObject c = e.getAsJsonObject();
            String crew = id(c, "crew");
            String conv = id(c, "conv");
            if (crew.isEmpty() || conv.isEmpty()) {
                continue;
            }
            List<CrewRoom> rooms = new ArrayList<>();
            JsonArray roomList = arr(c, "rooms");
            if (roomList != null) {
                for (JsonElement r : roomList) {
                    if (!r.isJsonObject() || rooms.size() >= MAX_CREW_ROOMS) {
                        continue;
                    }
                    JsonObject o = r.getAsJsonObject();
                    String id = id(o, "id");
                    if (id.isEmpty()) {
                        continue;
                    }
                    List<Named> people = new ArrayList<>();
                    JsonArray who = arr(o, "people");
                    if (who != null) {
                        for (JsonElement p : who) {
                            if (p.isJsonObject() && people.size() < MAX_PEOPLE) {
                                Named n = named(p.getAsJsonObject());
                                if (!n.id.isEmpty()) {
                                    people.add(n);
                                }
                            }
                        }
                    }
                    boolean general = Protocol.bool(o, "general", false);
                    String name = name(o, "name");
                    int max = (int) Math.max(0, Math.min(99, Protocol.num(o, "max", 0)));
                    rooms.add(new CrewRoom(id, general || name.isEmpty() ? null : name, general,
                        Collections.unmodifiableList(people), max, Protocol.bool(o, "full", false)));
                }
            }
            out.add(new CrewRooms(crew, conv, Collections.unmodifiableList(rooms), Protocol.bool(c, "add", false)));
        }
        return Collections.unmodifiableList(out);
    }

    /** At most {@code max} strings of a list, each plain and at most {@code chars} long. */
    private static List<String> strings(JsonArray list, int max, int chars) {
        if (list == null) {
            return List.of();
        }
        List<String> out = new ArrayList<>();
        for (JsonElement e : list) {
            if (out.size() >= max) {
                break;
            }
            if (e.isJsonPrimitive() && e.getAsJsonPrimitive().isString()) {
                out.add(Protocol.plain(e.getAsString(), chars));
            }
        }
        return List.copyOf(out);
    }

    private static String name(JsonObject o, String key) {
        return Protocol.plain(Protocol.str(o, key, ""), Protocol.MAX_NAME_CHARS);
    }

    /** An id or a code word: plain, short. */
    private static String id(JsonObject o, String key) {
        return Protocol.plain(Protocol.str(o, key, ""), MAX_ID);
    }

    private static String word(JsonObject o, String key, String fallback) {
        String v = id(o, key);
        return v.isEmpty() ? fallback : v;
    }

    private static JsonObject obj(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e != null && e.isJsonObject() ? e.getAsJsonObject() : null;
    }

    private static JsonArray arr(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e != null && e.isJsonArray() ? e.getAsJsonArray() : null;
    }
}
