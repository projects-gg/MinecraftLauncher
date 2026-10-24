package gg.projects.overlay.host;

import com.mojang.authlib.GameProfile;
import gg.projects.overlay.compat.HostCompat;
import io.netty.channel.Channel;
import io.netty.util.AttributeKey;
import java.io.IOException;
import java.nio.file.Path;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.storage.LevelResource;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Where the game's networking meets world sharing: the mixins ask here, the gate reads the session from here, and the
 * overlay hears what happened. Static, because the game's classes find nothing else. With no session (nearly always)
 * every question costs a field read and returns.
 */
public final class HostGate {
    /** The identity the gate bound to a connection (PROTOCOL.md §7.3). */
    public static final AttributeKey<GuestIdentity> IDENTITY = AttributeKey.valueOf("projects_host_identity");
    /** A connection that came in through the shared world's listener (identity or not). */
    public static final AttributeKey<Boolean> OURS = AttributeKey.valueOf("projects_host_ours");
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay host");
    private static final String UNVERIFIED = "multiplayer.disconnect.unverified_username";
    private static final String AUTH_DOWN = "multiplayer.disconnect.authservers_down";
    private static final ThreadLocal<Boolean> PUBLISHING = ThreadLocal.withInitial(() -> Boolean.FALSE);
    private static final ThreadLocal<Boolean> UNPUBLISHING = ThreadLocal.withInitial(() -> Boolean.FALSE);

    /** What the overlay hears (any thread). */
    public interface Listener {
        /** A guest got into the world. */
        void guestIn(HostSession session, GuestIdentity identity, String name);

        /** A guest who had been in the world left (or was sent off). */
        void guestOut(HostSession session, GuestIdentity identity, String name, boolean kicked);

        /** A connection did not get in: {@code reason} as the bridge names it. */
        void refused(HostSession session, GuestIdentity identity, String name, String reason);

        /** The game itself ends the world's sharing: the world closes, or vanilla's own control turned it off. */
        void endedByGame(HostSession session, String reason);
    }

    /** The answer to a guest's hello. */
    public enum Hello { VANILLA, OFFLINE, REFUSE }

    private static volatile HostSession session;
    private static volatile byte[] gateKey;
    private static volatile Listener listener;
    private static volatile Path bindingFile;
    /** The world server our session published on: 26.1.2 cannot unpublish, so the mark stays until it stops. */
    private static volatile MinecraftServer publishedOn;

    private HostGate() {
    }

    /** Set once at start with the bridge token's gate key and the overlay's listener. */
    public static void init(byte[] key, Listener l) {
        gateKey = key;
        listener = l;
    }

    public static HostSession session() {
        HostSession s = session;
        return s == null || s.closed() ? null : s;
    }

    public static byte[] gateKey() {
        return gateKey;
    }

    /** A session is about to publish; its bindings live in {@code file}. */
    static void open(HostSession s, Path file) {
        session = s;
        bindingFile = file;
    }

    /** Our publish on {@code server} worked. */
    static void markPublished(MinecraftServer server) {
        publishedOn = server;
    }

    /** The session ended (it was already closed); the gate refuses everyone from now on. */
    static void clear(HostSession s) {
        if (session == s) {
            session = null;
        }
    }

    /** Whether our session published on this server (and, on 26.1.2, it still listens). */
    public static boolean publishedBy(MinecraftServer server) {
        return server != null && publishedOn == server;
    }

    // ---- Publishing: the mixins know the call is ours ----------------------------------------------

    /** Inside our own {@code publishServer}: the listener goes on loopback, gets the gate, no LAN announcement. */
    public static boolean publishing() {
        return PUBLISHING.get();
    }

    /** Inside our own {@code unpublishServer}: not the game turning sharing off by itself. */
    public static boolean unpublishing() {
        return UNPUBLISHING.get();
    }

    static boolean publish(MinecraftServer server, String mode, boolean commands, int port) {
        PUBLISHING.set(Boolean.TRUE);
        try {
            return HostCompat.publish(server, mode, commands, port);
        } finally {
            PUBLISHING.set(Boolean.FALSE);
        }
    }

    static void unpublish(MinecraftServer server) {
        UNPUBLISHING.set(Boolean.TRUE);
        try {
            HostCompat.unpublish(server);
        } finally {
            UNPUBLISHING.set(Boolean.FALSE);
        }
        if (HostCompat.CAN_UNPUBLISH && publishedOn == server) {
            // 26.2+: the listener is gone; the world can be shared (or opened to LAN) again.
            publishedOn = null;
        }
    }

    /**
     * Vanilla's own control turns sharing off ({@code unpublishServer} from the game's screens, 26.2+): our session ends
     * first, so guests read why in their language before the game closes the listener.
     */
    public static void vanillaUnpublishing(MinecraftServer server) {
        HostSession s = session();
        Listener l = listener;
        if (s != null && l != null && publishedBy(server)) {
            l.endedByGame(s, "player");
        }
        if (publishedOn == server) {
            publishedOn = null;
        }
    }

    // ---- Login (network and server threads) ---------------------------------------------------------

    /** The identity bound to {@code channel}, or null. */
    public static GuestIdentity identity(Channel channel) {
        return channel == null ? null : channel.attr(IDENTITY).get();
    }

    /**
     * A remote connection's hello asks for {@code requested}. Connections through our listener need their identity and
     * the session's rules; a connection from anywhere else (vanilla LAN) only loses vanilla's hole: the host's own
     * name never logs in from outside (it would get the host's profile without any check).
     *
     * @return what to do; with {@link Hello#REFUSE}, {@link #refusalText} says why
     */
    public static Hello hello(MinecraftServer server, Channel channel, String requested, String[] refusalText) {
        GameProfile owner = server.getSingleplayerProfile();
        String hostName = owner == null ? null : owner.name();
        boolean ours = channel != null && Boolean.TRUE.equals(channel.attr(OURS).get());
        GuestIdentity identity = identity(channel);
        if (!ours && identity == null) {
            if (hostName != null && Names.same(requested, hostName)) {
                refusalText[0] = KickText.atLogin(KickText.Refusal.NAME, null);
                return Hello.REFUSE;
            }
            return Hello.VANILLA;
        }
        HostSession s = session;
        if (identity == null || s == null || s.closed()) {
            refusalText[0] = KickText.atLogin(KickText.Refusal.CLOSED, null);
            return Hello.REFUSE;
        }
        boolean online = server.getPlayerList().getPlayerByName(requested) != null;
        KickText.Refusal why = s.hello(identity, requested, online);
        if (why != null) {
            refuse(s, identity, requested, why);
            refusalText[0] = KickText.atLogin(why, null);
            return Hello.REFUSE;
        }
        // A Microsoft account goes through the game's own Mojang check (its next step); a username account gets the
        // offline profile, which only the invite's key, the launcher's header and the name binding let in.
        return identity.premium() ? Hello.VANILLA : Hello.OFFLINE;
    }

    /** Login is about to finish with {@code profile}: the last check, with the room counted. Null admits. */
    public static String verify(MinecraftServer server, Channel channel, GameProfile profile) {
        GuestIdentity identity = identity(channel);
        boolean ours = channel != null && Boolean.TRUE.equals(channel.attr(OURS).get());
        if (identity == null) {
            return ours ? KickText.atLogin(KickText.Refusal.CLOSED, null) : null;
        }
        HostSession s = session;
        if (s == null || s.closed()) {
            return KickText.atLogin(KickText.Refusal.CLOSED, null);
        }
        ServerPlayer already = server.getPlayerList().getPlayerByName(profile.name());
        KickText.Refusal why = s.verify(identity, profile.name(), profile.id(), already != null);
        if (why != null) {
            refuse(s, identity, profile.name(), why);
            return KickText.atLogin(why, null);
        }
        return null;
    }

    /** The login ends with a refusal: a Microsoft account Mojang did not vouch for is told to the launcher. */
    public static void loginDisconnect(Channel channel, Component reason) {
        GuestIdentity identity = identity(channel);
        HostSession s = session;
        if (identity == null || s == null || !(reason.getContents() instanceof TranslatableContents t)) {
            return;
        }
        if (UNVERIFIED.equals(t.getKey()) || AUTH_DOWN.equals(t.getKey())) {
            HostSession.Guest g = s.guest(identity);
            refuseQuietly(s, identity, g == null ? identity.name() : g.name(), "auth");
        }
    }

    private static void refuse(HostSession s, GuestIdentity identity, String name, KickText.Refusal why) {
        String reason = switch (why) {
            case NAME -> "name";
            case NAME_TAKEN -> "name-taken";
            case FULL -> "full";
            case REMOVED -> "removed";
            // Sharing is closing: the launcher knows already.
            case CLOSED -> null;
        };
        if (reason != null) {
            refuseQuietly(s, identity, name, reason);
        }
    }

    private static void refuseQuietly(HostSession s, GuestIdentity identity, String name, String reason) {
        LOG.info("A connection to the shared world was turned away ({})", reason);
        Listener l = listener;
        if (l != null) {
            l.refused(s, identity, name, reason);
        }
    }

    /** A connection that came through the gate closed (network thread). */
    public static void connectionClosed(HostSession s, GuestIdentity identity) {
        HostSession.Left left = s.closedConnection(identity);
        Listener l = listener;
        if (left != null && left.wasIn() && l != null) {
            l.guestOut(s, identity, left.name(), left.kicked());
        }
    }

    // ---- In the world (server thread) ----------------------------------------------------------------

    /** A player joined the world: a guest of ours is in now; its name is bound to its friend. */
    public static void joined(MinecraftServer server, ServerPlayer player) {
        HostSession s = session();
        if (s == null) {
            return;
        }
        HostSession.Guest g = s.byUuid(player.getUUID());
        if (g == null) {
            return;
        }
        if (s.joined(player.getUUID(), System.currentTimeMillis() / 1000L)) {
            writeBindings(s);
        }
        HostCompat.guestJoined(server, player, s.publish().mode());
        Listener l = listener;
        if (l != null) {
            l.guestIn(s, g.identity, g.name());
        }
    }

    private static void writeBindings(HostSession s) {
        Path file = bindingFile;
        if (file == null) {
            return;
        }
        try {
            s.bindings().save(file);
        } catch (IOException e) {
            LOG.warn("Could not write the world's guest names: {}", e.getMessage());
        }
    }

    /** The world's server is stopping (the host left the world, or the game closes): sharing ends with it. */
    public static void serverStopping(MinecraftServer server) {
        HostSession s = session();
        Listener l = listener;
        if (s != null && l != null && publishedBy(server)) {
            l.endedByGame(s, "left-world");
        }
    }

    /** The world's server stopped: nothing of it stays. */
    public static void serverStopped(MinecraftServer server) {
        if (publishedOn == server) {
            publishedOn = null;
            HostSession s = session;
            if (s != null) {
                s.close();
                session = null;
            }
        }
    }

    /** The world's folder name in {@code saves/} and its binding file. */
    static Path worldDir(MinecraftServer server) {
        return server.getWorldPath(LevelResource.ROOT).toAbsolutePath().normalize();
    }
}
