package gg.projects.overlay.host;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * One open world session as the game keeps it (PROTOCOL.md §7.3): the publish the launcher asked for, the guests'
 * connections from the gate through login into the world, the names bound in this world, who was removed. The rules
 * live here, without Minecraft, so they have plain tests; the mixins and the overlay only ask. Thread-safe: the gate
 * runs on network threads, logins on the network and server threads, the overlay on the render thread.
 */
public final class HostSession {
    /** Where a guest's connection is: past the gate, past the login checks, or in the world. */
    public enum Stage { LOGIN, ADMITTED, IN }

    /** What the launcher's {@code host-publish} asked for. */
    public record Publish(String id, String mode, boolean commands, int max, String bindingKey) {
    }

    /** A connection that came through the gate. {@code handle} is the gate's (its network channel). */
    public static final class Guest {
        public final GuestIdentity identity;
        public final Object handle;
        private Stage stage = Stage.LOGIN;
        private String name;
        private UUID uuid;
        private boolean kicked;

        Guest(GuestIdentity identity, Object handle) {
            this.identity = identity;
            this.handle = handle;
        }

        public Stage stage() {
            return stage;
        }

        /** The name it logs in with, once its hello came. */
        public String name() {
            return name == null ? identity.name() : name;
        }

        public UUID uuid() {
            return uuid;
        }

        /** The host sent it off (removed, or sharing stopped), so its leaving is not its own. */
        public boolean kicked() {
            return kicked;
        }
    }

    /** A guest left: who, whether it had been in the world, and whether the host sent it off. */
    public record Left(GuestIdentity identity, String name, boolean wasIn, boolean kicked) {
    }

    private final Publish publish;
    private final String hostName;
    private final GuestBindings bindings;
    private final Set<Long> conns = new HashSet<>();
    private final Set<String> removed = new HashSet<>();
    /** By publish connection number. */
    private final Map<Long, Guest> guests = new HashMap<>();
    private boolean closed;

    /**
     * @param hostName the world owner's game name: never given to a remote connection
     * @param bindings the world's name bindings, read for this publish's key
     */
    public HostSession(Publish publish, String hostName, GuestBindings bindings) {
        this.publish = publish;
        this.hostName = hostName;
        this.bindings = bindings;
    }

    public Publish publish() {
        return publish;
    }

    public String hostName() {
        return hostName;
    }

    public synchronized boolean closed() {
        return closed;
    }

    /**
     * A verified header (PROTOCOL.md §7.3): it must be for this publish and its connection number unused. Returns the
     * guest's entry, or null: the gate then closes the connection without a word.
     */
    public synchronized Guest admitHeader(GuestIdentity identity, Object handle) {
        if (closed || !publish.id().equals(identity.id()) || !conns.add(identity.conn())) {
            return null;
        }
        Guest g = new Guest(identity, handle);
        guests.put(identity.conn(), g);
        return g;
    }

    /** The entry for a connection that came through the gate, or null. */
    public synchronized Guest guest(GuestIdentity identity) {
        Guest g = identity == null ? null : guests.get(identity.conn());
        return g != null && g.identity.equals(identity) ? g : null;
    }

    /**
     * The game's hello ({@code ServerLoginPacketListenerImpl.handleHello}): the name it asks for must be the header's,
     * never the host's, a valid one, not already in this world, not bound to another friend; the world must have room
     * and the guest must not have been removed. Returns why not, or null to go on.
     *
     * @param online whether a player with that name is in the world now
     */
    public synchronized KickText.Refusal hello(GuestIdentity identity, String requested, boolean online) {
        Guest g = guest(identity);
        if (g == null || closed) {
            return KickText.Refusal.CLOSED;
        }
        g.name = requested;
        return check(g, requested, online);
    }

    /**
     * Login is about to finish ({@code verifyLoginAndFinishConnectionSetup}): the same rules again with the profile the
     * login ended with (Mojang's for a Microsoft account), and the room counted among the admitted. Null admits it.
     */
    public synchronized KickText.Refusal verify(GuestIdentity identity, String profileName, UUID uuid, boolean online) {
        Guest g = guest(identity);
        if (g == null || closed) {
            return KickText.Refusal.CLOSED;
        }
        KickText.Refusal why = check(g, profileName, online);
        if (why == null) {
            g.stage = Stage.ADMITTED;
            g.name = profileName;
            g.uuid = uuid;
        }
        return why;
    }

    private KickText.Refusal check(Guest g, String name, boolean online) {
        if (removed.contains(g.identity.guest())) {
            return KickText.Refusal.REMOVED;
        }
        if (!Names.valid(name) || !Names.same(name, g.identity.name()) || Names.same(name, hostName) || online
            || nameInUse(g, name)) {
            return KickText.Refusal.NAME;
        }
        if (bindings.check(name, g.identity.friend()) == GuestBindings.Verdict.TAKEN) {
            return KickText.Refusal.NAME_TAKEN;
        }
        if (admittedBesides(g) >= publish.max()) {
            return KickText.Refusal.FULL;
        }
        return null;
    }

    /** Another connection logs in (or plays) under this name. */
    private boolean nameInUse(Guest self, String name) {
        for (Guest other : guests.values()) {
            if (other != self && other.stage != Stage.LOGIN && Names.same(other.name(), name)) {
                return true;
            }
        }
        return false;
    }

    private int admittedBesides(Guest self) {
        int n = 0;
        for (Guest other : guests.values()) {
            if (other != self && other.stage != Stage.LOGIN && !other.identity.guest().equals(self.identity.guest())) {
                n++;
            }
        }
        return n;
    }

    /**
     * The guest is in the world. Binds its name to its friend (the first time) and returns true when the binding file
     * changed and should be written.
     */
    public synchronized boolean joined(UUID uuid, long nowSeconds) {
        Guest g = byUuid(uuid);
        if (g == null) {
            return false;
        }
        g.stage = Stage.IN;
        return bindings.bind(g.name(), g.identity.friend(), nowSeconds);
    }

    /** The guest playing as {@code uuid}, or null (the host, or a player from elsewhere). */
    public synchronized Guest byUuid(UUID uuid) {
        if (uuid == null) {
            return null;
        }
        for (Guest g : guests.values()) {
            if (uuid.equals(g.uuid)) {
                return g;
            }
        }
        return null;
    }

    /** The guest's connection closed, in whatever stage. Null when it was not ours (or already gone). */
    public synchronized Left closedConnection(GuestIdentity identity) {
        Guest g = guest(identity);
        if (g == null) {
            return null;
        }
        guests.remove(identity.conn());
        return new Left(identity, g.name(), g.stage == Stage.IN, g.kicked);
    }

    /**
     * The host (or the launcher, for a block or a restriction) takes guest {@code guestId} out: refused from now on in
     * this session. Returns its live connections, marked as sent off, for the overlay to close.
     */
    public synchronized List<Guest> remove(String guestId) {
        removed.add(guestId);
        List<Guest> out = new ArrayList<>();
        for (Guest g : guests.values()) {
            if (g.identity.guest().equals(guestId)) {
                g.kicked = true;
                out.add(g);
            }
        }
        return out;
    }

    public synchronized boolean removed(String guestId) {
        return removed.contains(guestId);
    }

    /**
     * Sharing stops: nobody else gets in; every live connection, marked as sent off, for the overlay to close. Only the
     * first call returns them (the world's server and the overlay may both end it).
     */
    public synchronized List<Guest> close() {
        if (closed) {
            return List.of();
        }
        closed = true;
        List<Guest> out = new ArrayList<>(guests.values());
        for (Guest g : out) {
            g.kicked = true;
        }
        return out;
    }

    /** The guests in the world now. */
    public synchronized List<Guest> inWorld() {
        List<Guest> out = new ArrayList<>();
        for (Guest g : guests.values()) {
            if (g.stage == Stage.IN) {
                out.add(g);
            }
        }
        return out;
    }

    /** The binding file's text as it stands (written under this lock's care by the caller). */
    public synchronized String bindingsJson() {
        return bindings.toJson();
    }

    synchronized GuestBindings bindings() {
        return bindings;
    }
}
