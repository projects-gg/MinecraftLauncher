package gg.projects.overlay.host;

import com.google.gson.JsonObject;
import com.mojang.authlib.GameProfile;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.HostCompat;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Texts;
import io.netty.channel.Channel;
import java.io.IOException;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.regex.Pattern;
import net.minecraft.SharedConstants;
import net.minecraft.client.Minecraft;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Util;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The game side of "Dünyanı arkadaşlarına aç" (host1.md §4): which singleplayer world the player is in (told to the
 * launcher with {@code host-world}), opening it when the launcher says {@code host-publish} after the player's "Davet
 * et", sending guests off ({@code host-remove}) and closing it ({@code host-stop}, the player's own "Paylaşımı kapat",
 * leaving the world, the launcher going away). The rules for who gets in are {@link HostSession}'s; the game's
 * networking reaches them through {@link HostGate}. Lives on the render thread; what the world's server reports comes
 * back here through the game's task queue.
 */
public final class HostController implements HostGate.Listener {
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay host");
    private static final Pattern PUBLISH_ID = Pattern.compile("[A-Za-z0-9_-]{1,32}");
    private static final Pattern KEY_ID = Pattern.compile("[A-Za-z0-9_-]{8}");
    /** Friends one invite may name (the launcher caps it to Studio's limit too). */
    public static final int MAX_INVITE = 7;
    /** How often (ticks) the world's own LAN switch and game mode are looked at. */
    private static final int WORLD_CHECK_TICKS = 20;
    /** How long an opening may wait for the launcher before it reads as failed. */
    public static final long OPENING_MS = 20_000;

    /** What the overlay shows of world sharing. */
    public interface View {
        /** Something about the session changed (the panel, the screen). */
        void changed();

        /** A guest came in ({@code in}) or left; {@code label} as the player knows them. */
        void guest(String label, boolean in);
    }

    /** What an invite from elsewhere in the overlay did ({@link #inviteFriend}). */
    public enum WorldInvite {
        /** The player is not in a singleplayer world: the caller's own invite (to where the player plays) applies. */
        NOT_HERE(null),
        /** The open world got one more invite. */
        INVITED(null),
        /** The world opens with this friend's invite; the launcher sends it once the world is open. */
        OPENING(null),
        /** The world can't be opened now: nothing went out. */
        UNAVAILABLE("host.failed.unavailable"),
        /** The launcher is not there: nothing went out. */
        LAUNCHER_AWAY("host.launcher_needed");

        private final String lineKey;

        WorldInvite(String lineKey) {
            this.lineKey = lineKey;
        }

        /** The invite went out, or goes out once the world is open. */
        public boolean sent() {
            return this == INVITED || this == OPENING;
        }

        /** The one short line for an invite that did not go out (the caller shows it where the player is), or null. */
        public String line() {
            return lineKey == null ? null : Texts.str(lineKey);
        }
    }

    private final OverlayClient overlay;
    private final View view;
    /** The singleplayer world the player is in, or null. Render thread. */
    private IntegratedServer world;
    private String folder = "";
    private String mode = "survival";
    private boolean hardcore;
    private boolean lan;
    private boolean worldSent;
    private int checkIn;
    /** Our open session (any thread reads; changes on the render thread or the world's stopping). */
    private volatile HostSession session;
    /** When the last {@code host-request} went ({@code Util.getMillis()}), 0 when no opening waits for its answer. */
    private long requestedAt;
    /** Why the last opening failed ({@code unavailable}, {@code limit}), null when it did not or a new one began. */
    private String failure;
    /** Friends invited while the world was still opening: their invites go once it is open. */
    private final List<String> waiting = new ArrayList<>();
    /** The friends the last opening was for (a failed one keeps them ticked for "Tekrar dene"). */
    private List<String> lastInvite = List.of();
    /** When the last opening failed ({@code Util.getMillis()}). */
    private long failedAt;
    /** How long the card on the other pages keeps a failed opening's line. */
    private static final long FAILURE_SHOWN_MS = 120_000;

    public HostController(OverlayClient overlay, View view) {
        this.overlay = overlay;
        this.view = view;
    }

    // ---- What the screens ask ---------------------------------------------------------------------------

    /**
     * "Arkadaşlarını çağır" makes sense now: the launcher said this player may open their world (H2), the game is in
     * a singleplayer world without vanilla's own LAN sharing. Shown only then; while the launcher is away it shows
     * dimmed ({@link #launcherAway}).
     */
    public boolean canInvite() {
        return world != null && !lan && overlay.state().has("host");
    }

    /** The launcher's bridge is not there now ("Launcher açık olmalı."). */
    public boolean launcherAway() {
        return !overlay.state().online();
    }

    /** In a singleplayer world (the pause menu's button, the panel's section). */
    public boolean inWorld() {
        return world != null;
    }

    /** Our session is open. */
    public boolean hosting() {
        HostSession s = session;
        return s != null && !s.closed();
    }

    /** The world's game mode (the invite screen picks it first): {@code survival}, {@code creative} … */
    public String worldMode() {
        return mode;
    }

    /** A hardcore world keeps its own rules: no game mode to pick. */
    public boolean hardcore() {
        return hardcore;
    }

    /** The game mode the open session's guests play ({@code survival} when none is open). */
    public String sessionMode() {
        HostSession s = session;
        return s == null ? "survival" : s.publish().mode();
    }

    /** Whether the open session's guests may use commands. */
    public boolean sessionCommands() {
        HostSession s = session;
        return s != null && s.publish().commands();
    }

    /** An invite from the overlay goes to the player's own world now: they are in a singleplayer world. */
    public boolean invitesToWorld() {
        return world != null;
    }

    /**
     * An invite from elsewhere in the overlay (the friend menu's "Birlikte oyna") while the player is in a singleplayer
     * world. The owner, 24 Oct: the world switches online by itself, as the launcher's own invite does. With no session
     * the world opens for this friend ({@code host-request}: the guests play the world's game mode, without commands;
     * the world page changes those) and the launcher sends the invite once it is open; with one open the friend gets an
     * invite ({@code host-invite}); while it opens the friend waits for it. Nothing goes out when the world can't open.
     * Render thread.
     */
    public WorldInvite inviteFriend(String friendId) {
        if (world == null) {
            return WorldInvite.NOT_HERE;
        }
        if (launcherAway()) {
            return WorldInvite.LAUNCHER_AWAY;
        }
        if (friendId == null || friendId.isEmpty()) {
            return WorldInvite.UNAVAILABLE;
        }
        if (hosting()) {
            return overlay.bridge().send(Protocol.hostInvite(List.of(friendId))) ? WorldInvite.INVITED
                : WorldInvite.LAUNCHER_AWAY;
        }
        if (requesting()) {
            if (!waiting.contains(friendId) && waiting.size() < MAX_INVITE) {
                waiting.add(friendId);
            }
            return WorldInvite.OPENING;
        }
        if (!canInvite()) {
            return WorldInvite.UNAVAILABLE;
        }
        return invite(mode, false, List.of(friendId)) ? WorldInvite.OPENING : WorldInvite.LAUNCHER_AWAY;
    }

    /** A {@code host-request} waits for the launcher's answer (the page and the card show "Dünyan açılıyor"). */
    public boolean requesting() {
        return requestedAt != 0 && !hosting();
    }

    /** Why the last opening failed ({@code unavailable}, {@code limit}), or null. */
    public String failure() {
        return failure;
    }

    /** A failed opening the card on the other pages still mentions (for two minutes; the world page keeps it). */
    public boolean recentFailure() {
        return failure != null && Util.getMillis() - failedAt < FAILURE_SHOWN_MS;
    }

    /** The friends the last opening was for. */
    public List<String> lastInvite() {
        return lastInvite;
    }

    /**
     * "Davet et": with no session the world opens for these friends ({@code host-request}; the launcher checks and
     * answers with {@code host-publish}), with one open they get invites ({@code host-invite}).
     *
     * @return false when the launcher could not be told
     */
    public boolean invite(String guestMode, boolean commands, List<String> friends) {
        List<String> ids = friends.stream().filter(f -> f != null && !f.isEmpty()).distinct().limit(MAX_INVITE)
            .toList();
        if (ids.isEmpty()) {
            return false;
        }
        if (hosting()) {
            return overlay.bridge().send(Protocol.hostInvite(ids));
        }
        String m = switch (guestMode) {
            case "creative", "adventure" -> guestMode;
            default -> "survival";
        };
        if (!overlay.bridge().send(Protocol.hostRequest(m, commands, ids))) {
            return false;
        }
        requestedAt = Util.getMillis();
        failure = null;
        waiting.clear();
        lastInvite = ids;
        // A new try clears what the last failed one left on screen.
        overlay.state().hostEnded();
        return true;
    }

    /** "Paylaşımı kapat": the guests are sent off now; the launcher closes the paths. */
    public void stopSharing() {
        end("player", true);
    }

    /** "Dünyadan çıkar": sent off now and never let back in this session; the launcher drops the invite. */
    public void removeGuest(String guestId) {
        HostSession s = session;
        if (s == null || guestId == null || guestId.isEmpty()) {
            return;
        }
        sendOff(s, s.remove(guestId), KickText.Refusal.REMOVED);
        overlay.bridge().send(Protocol.hostRemove(guestId));
        view.changed();
    }

    // ---- The launcher -----------------------------------------------------------------------------------

    /**
     * A launcher frame of world sharing. Returns true when it is wholly handled here; {@code host-state} goes on to the
     * overlay's state as well (it mirrors the session for the screens).
     */
    public boolean onFrame(JsonObject frame) {
        switch (Protocol.type(frame)) {
            case "host-publish" -> {
                publish(frame);
                return true;
            }
            case "host-stop" -> {
                requestedAt = 0;
                waiting.clear();
                end(null, true);
                return true;
            }
            case "host-remove" -> {
                HostSession s = session;
                String guest = Protocol.str(frame, "guest", "");
                if (s != null && !guest.isEmpty()) {
                    // Removed, blocked, unfriended or restricted: the guest only ever reads that they were removed.
                    sendOff(s, s.remove(guest), KickText.Refusal.REMOVED);
                    view.changed();
                }
                return true;
            }
            default -> {
                return false;
            }
        }
    }

    /** The bridge came up or went away. */
    public void onBridgeStatus(BridgeClient.Status status) {
        if (status == BridgeClient.Status.ONLINE) {
            // A new connection knows nothing of the world yet.
            worldSent = false;
            return;
        }
        requestedAt = 0;
        waiting.clear();
        if (hosting()) {
            // Without the launcher nobody reaches the world: the guests are told it closed.
            LOG.info("The launcher went away; the world's sharing ends");
            end(null, true);
        }
    }

    /** Once per client tick: which world the player is in, and whether its LAN switch or mode changed. */
    public void tick(Minecraft mc) {
        if (requestedAt != 0 && !hosting() && Util.getMillis() - requestedAt > OPENING_MS) {
            // Nothing came back: it did not open.
            openingFailed("unavailable");
        }
        HostSession s = session;
        if (s != null && s.closed()) {
            // Closed under us (the world's server stopped).
            session = null;
            overlay.state().hostEnded();
            view.changed();
        }
        IntegratedServer server = mc.getSingleplayerServer();
        boolean in = server != null && mc.level != null && mc.player != null && server.isRunning();
        if (!in) {
            if (world != null) {
                leftWorld();
            }
            return;
        }
        if (server != world) {
            if (world != null) {
                leftWorld();
            }
            world = server;
            checkIn = 0;
            worldSent = false;
        }
        if (--checkIn <= 0) {
            checkIn = WORLD_CHECK_TICKS;
            String folderNow = folderOf(server);
            String modeNow = server.getDefaultGameType().getName();
            boolean lanNow = HostCompat.vanillaLanOpen(server);
            boolean hardcoreNow = server.isHardcore();
            if (!folderNow.equals(folder) || !modeNow.equals(mode) || lanNow != lan || hardcoreNow != hardcore) {
                folder = folderNow;
                mode = modeNow;
                lan = lanNow;
                hardcore = hardcoreNow;
                worldSent = false;
                view.changed();
            }
        }
        if (!worldSent && overlay.state().online()) {
            worldSent = overlay.bridge().send(Protocol.hostWorld(true, folder, mode, lan));
        }
    }

    private void leftWorld() {
        // A session still open here ends as the world's server stops (HostGate.serverStopping), on that server's own
        // thread: a send-off from here could wait forever on a server that no longer runs its tasks.
        world = null;
        folder = "";
        lan = false;
        requestedAt = 0;
        failure = null;
        waiting.clear();
        overlay.bridge().send(Protocol.hostWorld(false, null, null, false));
        worldSent = false;
        view.changed();
    }

    private static String folderOf(MinecraftServer server) {
        Path dir = HostGate.worldDir(server);
        Path name = dir.getFileName();
        return name == null ? "" : name.toString();
    }

    // ---- Opening and closing ----------------------------------------------------------------------------

    /** {@code host-publish}: opens the world on loopback for this publish, or says why not (H6). */
    private void publish(JsonObject frame) {
        String id = Protocol.str(frame, "id", "");
        if (!PUBLISH_ID.matcher(id).matches()) {
            LOG.warn("A publish request without a usable id was ignored");
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        IntegratedServer server = mc.getSingleplayerServer();
        if (server == null || mc.level == null || world != server) {
            requestedAt = 0;
            overlay.bridge().send(Protocol.hostPublishFailed(id, "not-world"));
            return;
        }
        if (hosting()) {
            overlay.bridge().send(Protocol.hostPublishFailed(id, "busy"));
            return;
        }
        if (HostCompat.vanillaLanOpen(server)) {
            openingFailed("unavailable");
            overlay.bridge().send(Protocol.hostPublishFailed(id, "lan"));
            return;
        }
        String guestMode = Protocol.str(frame, "mode", "survival");
        long max = Protocol.num(frame, "max", 0);
        String binding = Protocol.str(frame, "binding", "");
        if (!guestMode.equals("survival") && !guestMode.equals("creative") && !guestMode.equals("adventure")
            || max < 1 || max > 7 || !KEY_ID.matcher(binding).matches()) {
            openingFailed("unavailable");
            overlay.bridge().send(Protocol.hostPublishFailed(id, "error"));
            return;
        }
        Path file = HostGate.worldDir(server).resolve(GuestBindings.FILE);
        GuestBindings bindings = GuestBindings.load(file, binding, LOG::info);
        GameProfile owner = server.getSingleplayerProfile();
        String hostName = owner == null ? mc.getUser().getName() : owner.name();
        HostSession s = new HostSession(new HostSession.Publish(id, guestMode,
            Protocol.bool(frame, "commands", false), (int) max, binding), hostName, bindings);
        HostGate.open(s, file);
        boolean ok = false;
        int port = -1;
        // A port picked free a moment ago can be taken by then: one more try.
        for (int attempt = 0; attempt < 2 && !ok; attempt++) {
            port = freeLoopbackPort();
            ok = port > 0 && HostGate.publish(server, guestMode, s.publish().commands(), port);
        }
        if (!ok) {
            s.close();
            HostGate.clear(s);
            LOG.warn("The world could not be opened for friends");
            openingFailed("unavailable");
            overlay.bridge().send(Protocol.hostPublishFailed(id, "error"));
            return;
        }
        HostGate.markPublished(server);
        requestedAt = 0;
        failure = null;
        session = s;
        LOG.info("The world is open to friends on loopback");
        overlay.bridge().send(Protocol.hostPublished(id, port, SharedConstants.getProtocolVersion()));
        view.changed();
    }

    /** This opening ends without a world: the page and the card say so, and a note when the panel is closed. */
    private void openingFailed(String why) {
        requestedAt = 0;
        waiting.clear();
        failure = why == null || why.isEmpty() ? "unavailable" : why;
        failedAt = Util.getMillis();
        overlay.showHostFailure(failureLine(failure));
        view.changed();
    }

    /** The one short line for a failed opening. */
    public static String failureLine(String why) {
        return Texts.str("limit".equals(why) ? "host.failed.limit" : "host.failed.unavailable");
    }

    /**
     * The launcher's mirror changed (H8): once the world is open the player hears it and the friends invited meanwhile
     * get their invites; a failed opening ({@code error}) ends the wait.
     */
    public void mirrorChanged(OverlayState.HostView before, OverlayState.HostView after) {
        if (after == null) {
            return;
        }
        if (after.open() && (before == null || !before.open()) && hosting()) {
            // The paths are up and the invites went: the world is open (once per opening).
            overlay.showHostNote(Texts.str("host.opened"), null, true, "toast.hint.manage", "host-open");
            if (!waiting.isEmpty()) {
                overlay.bridge().send(Protocol.hostInvite(List.copyOf(waiting)));
                waiting.clear();
            }
            return;
        }
        String error = after.error();
        if (!after.open() && error != null && !error.isEmpty() && !hosting()
            && (before == null || !error.equals(before.error()))) {
            boolean told = requestedAt == 0 && failure != null;
            requestedAt = 0;
            waiting.clear();
            failure = error;
            failedAt = Util.getMillis();
            if (!told) {
                overlay.showHostFailure(failureLine(error));
            }
            view.changed();
        }
    }

    /** A port on 127.0.0.1 nobody listens on now, or -1. */
    private static int freeLoopbackPort() {
        try (ServerSocket socket = new ServerSocket(0, 1, InetAddress.getLoopbackAddress())) {
            return socket.getLocalPort();
        } catch (IOException e) {
            return -1;
        }
    }

    /**
     * Ends the session on the render thread: every guest is sent off ("Ayşe paylaşımı kapattı." in their language),
     * the world stops listening (26.2+; 26.1.2 keeps a listener nobody gets through), the launcher hears {@code
     * reason} when it is to be told.
     *
     * @param unpublish false when the world is closing anyway
     */
    private void end(String reason, boolean unpublish) {
        HostSession s = session;
        session = null;
        if (s == null || s.closed()) {
            return;
        }
        sendOff(s, s.close(), KickText.Refusal.CLOSED);
        IntegratedServer server = Minecraft.getInstance().getSingleplayerServer();
        if (unpublish && server != null && HostGate.publishedBy(server)) {
            HostGate.unpublish(server);
        }
        HostGate.clear(s);
        if (reason != null) {
            overlay.bridge().send(Protocol.hostStop(reason));
        }
        overlay.state().hostEnded();
        view.changed();
    }

    /**
     * Sends guests off: one in the world reads why in its own language; one still logging in is just closed (its
     * launcher tells it). Called on the render thread (the game's own unpublish does the same from there) or the
     * world's server thread.
     */
    private void sendOff(HostSession s, List<HostSession.Guest> guests, KickText.Refusal why) {
        // While the world halts the game has already let go of it: our publish still knows the server.
        MinecraftServer server = HostGate.publishedServer();
        if (server == null) {
            server = Minecraft.getInstance().getSingleplayerServer();
        }
        for (HostSession.Guest g : guests) {
            ServerPlayer player = g.uuid() == null || server == null ? null
                : server.getPlayerList().getPlayer(g.uuid());
            if (g.stage() == HostSession.Stage.IN && player != null) {
                String text = KickText.inWorld(why, hostLabel(), HostCompat.language(player));
                player.connection.disconnect(Component.literal(text));
            } else if (g.handle instanceof Channel channel) {
                channel.close();
            }
        }
    }

    /** How guests know the host: the player's friends name, else the game name. */
    private String hostLabel() {
        String me = overlay.state().me();
        if (me != null && !me.isBlank()) {
            return me;
        }
        HostSession s = session;
        return s == null ? Minecraft.getInstance().getUser().getName() : s.hostName();
    }

    // ---- What the world's server reports (any thread) ---------------------------------------------------

    @Override
    public void guestIn(HostSession s, GuestIdentity identity, String name) {
        overlay.bridge().send(Protocol.hostGuest(identity.conn(), identity.guest(), name, "in", null));
        Minecraft.getInstance().execute(() -> {
            if (s == session) {
                view.guest(labelOf(identity.guest(), name), true);
                view.changed();
            }
        });
    }

    @Override
    public void guestOut(HostSession s, GuestIdentity identity, String name, boolean kicked) {
        overlay.bridge().send(Protocol.hostGuest(identity.conn(), identity.guest(), name, "out",
            kicked ? "kicked" : "left"));
        Minecraft.getInstance().execute(() -> {
            if (s == session && !kicked) {
                view.guest(labelOf(identity.guest(), name), false);
            }
            view.changed();
        });
    }

    @Override
    public void refused(HostSession s, GuestIdentity identity, String name, String reason) {
        overlay.bridge().send(Protocol.hostGuest(identity.conn(), identity.guest(), name, "refused", reason));
    }

    @Override
    public void endedByGame(HostSession s, String reason) {
        if (s != session) {
            return;
        }
        if (Minecraft.getInstance().isSameThread()) {
            // The game's own control turned sharing off (26.2+): it closes the listener itself.
            end(reason, false);
            return;
        }
        // The world's server is stopping, the render thread may be waiting for it: everything here, now.
        session = null;
        List<HostSession.Guest> guests = s.close();
        sendOff(s, guests, KickText.Refusal.CLOSED);
        HostGate.clear(s);
        overlay.bridge().send(Protocol.hostStop(reason));
        Minecraft.getInstance().execute(() -> {
            overlay.state().hostEnded();
            view.changed();
        });
    }

    /** The friend's name as the launcher mirrors it for this guest, else the game name. */
    private String labelOf(String guestId, String name) {
        OverlayState.HostView host = overlay.state().host();
        OverlayState.HostGuest g = host == null ? null : host.byGuest(guestId);
        return g != null && !g.label().isBlank() ? g.label() : name;
    }
}
