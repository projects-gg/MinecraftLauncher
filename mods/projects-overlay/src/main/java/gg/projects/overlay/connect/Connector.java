package gg.projects.overlay.connect;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.Protocol;
import java.util.function.Consumer;

/**
 * Going to a server from inside the game. The launcher's "Bağlan" (a crew call, an invite, a plan) sends
 * {@code connect}: the player already confirmed there, so nothing asks again. A short toast says where; a
 * moment later the current world is left the way the pause menu's "Save and Quit to Title" leaves it (a
 * singleplayer world is saved), and the game's own connect screen takes over. The overlay's own "Katıl"
 * goes the same way after its question.
 *
 * <p>The launcher waits five seconds for the answer and otherwise restarts the game the old way (L19), so
 * the answer never waits for a world to save: {@code ok} goes out the moment the game leaves for the server;
 * {@code busy} when a world is loading or saving, resources are reloading or another connect is under way
 * (also when that does not end within {@link #WAIT_MS} after the toast); {@code invalid} when the request
 * names no usable server; {@code ok} with {@code already} when the game is on that server.
 *
 * <p>The game side sits behind {@link Game}, so the flow has plain tests.
 */
public final class Connector {
    /** What the flow needs from the game. */
    public interface Game {
        /** A world or server is loading or saving, resources are reloading, or the game is connecting. */
        boolean busy();

        /** In a world, singleplayer or a server: leaving it comes first. */
        boolean inWorld();

        /** The server the game is on, or null (singleplayer, menus). */
        ServerTarget currentServer();

        /** The game's own address rules accept the target. */
        boolean valid(ServerTarget target);

        /** Leaves the current world the normal way and opens the connect screen; false if the game refused. */
        boolean leaveAndConnect(ServerTarget target);

        /** Shows "... sunucusuna bağlanılıyor…", kept above every screen until {@link #endTravel()}. */
        void showTravel(ServerTarget target);

        void endTravel();

        /** The game is still on its way: a connect, loading, saving or disconnecting screen. */
        boolean travelling();

        /** "Zaten ... sunucusundasın." */
        void showAlready(ServerTarget target);
    }

    /** Between the toast and leaving the world: long enough to see where the game is going. */
    public static final long BEAT_MS = 600;
    /**
     * How long a confirmed connect waits for a loading or saving screen (one that started after the request)
     * to finish; with the beat it stays well inside the launcher's five seconds.
     */
    public static final long WAIT_MS = 3_000;
    /** The travel toast never stays longer than this, whatever the game does. */
    public static final long TRAVEL_MAX_MS = 60_000;
    private static final int MAX_ID_CHARS = 64;

    private final Game game;
    private final Consumer<JsonObject> reply;
    private ServerTarget pending;
    private String pendingId;
    private long dueAt;
    private long giveUpAt;
    private boolean travelShown;
    private boolean departed;
    private long travelSince;

    public Connector(Game game, Consumer<JsonObject> reply) {
        this.game = game;
        this.reply = reply;
    }

    /** The launcher's {@code connect} frame. */
    public void request(JsonObject frame, long now) {
        String id = Protocol.str(frame, "id", "");
        ServerTarget target = ServerTarget.of(Protocol.str(frame, "host", null), Protocol.num(frame, "port", -1));
        if (target != null) {
            // A friend's world (H12): what the player reads instead of the launcher's local address.
            target = target.withLabel(Protocol.plain(Protocol.str(frame, "label", ""), Protocol.MAX_NAME_CHARS));
        }
        if (id.isEmpty() || id.length() > MAX_ID_CHARS || target == null || !game.valid(target)) {
            answer(id.length() > MAX_ID_CHARS ? "" : id, false, "invalid", false);
            return;
        }
        if (pending != null || game.busy()) {
            answer(id, false, "busy", false);
            return;
        }
        if (target.sameServer(game.currentServer())) {
            game.showAlready(target);
            answer(id, true, null, true);
            return;
        }
        start(id, target, now, game.inWorld() ? BEAT_MS : 0);
    }

    /**
     * The overlay's own "Bağlan", after its question: goes at once.
     *
     * @return false when the game is busy (a world loading or saving, another connect) or the address is unusable
     */
    public boolean go(ServerTarget target, long now) {
        if (target == null || !game.valid(target) || pending != null || game.busy()) {
            return false;
        }
        start(null, target, now, 0);
        return true;
    }

    /** A connect is waiting for its moment (the toast is up, the world not yet left). */
    public boolean pending() {
        return pending != null;
    }

    /** The travel toast is up. */
    public boolean travelShown() {
        return travelShown;
    }

    private void start(String id, ServerTarget target, long now, long beat) {
        pending = target;
        pendingId = id;
        dueAt = now + beat;
        giveUpAt = dueAt + WAIT_MS;
        departed = false;
        travelSince = now;
        travelShown = true;
        game.showTravel(target);
        if (beat == 0) {
            tick(now);
        }
    }

    /** Once per client tick. */
    public void tick(long now) {
        if (pending != null) {
            if (now < dueAt) {
                return;
            }
            if (game.busy()) {
                // The player pressed "Save and Quit to Title" meanwhile, or a reload started: wait for it.
                if (now >= giveUpAt) {
                    answer(pendingId, false, "busy", false);
                    pending = null;
                    endTravel();
                }
                return;
            }
            ServerTarget target = pending;
            String id = pendingId;
            pending = null;
            // Answered before leaving: saving a big world can take longer than the launcher waits.
            answer(id, true, null, false);
            if (game.leaveAndConnect(target)) {
                departed = true;
            } else {
                endTravel();
            }
            return;
        }
        if (travelShown && departed && (!game.travelling() || now - travelSince > TRAVEL_MAX_MS)) {
            endTravel();
        }
    }

    private void endTravel() {
        if (travelShown) {
            travelShown = false;
            departed = false;
            game.endTravel();
        }
    }

    private void answer(String id, boolean ok, String reason, boolean already) {
        if (id != null) {
            reply.accept(Protocol.connectResult(id, ok, reason, already));
        }
    }
}
