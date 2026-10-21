package gg.projects.overlay.connect;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

class ConnectorTest {
    /** The game as the flow sees it, with a log of what was asked of it. */
    private static class FakeGame implements Connector.Game {
        boolean busy;
        boolean inWorld = true;
        ServerTarget current;
        boolean travelling;
        boolean valid = true;
        boolean refuse;
        final List<String> log = new ArrayList<>();

        @Override
        public boolean busy() {
            return busy;
        }

        @Override
        public boolean inWorld() {
            return inWorld;
        }

        @Override
        public ServerTarget currentServer() {
            return current;
        }

        @Override
        public boolean valid(ServerTarget target) {
            return valid;
        }

        @Override
        public boolean leaveAndConnect(ServerTarget target) {
            log.add("connect " + target.address());
            if (refuse) {
                return false;
            }
            inWorld = false;
            travelling = true;
            return true;
        }

        @Override
        public void showTravel(ServerTarget target) {
            log.add("travel " + target.displayName());
        }

        @Override
        public void endTravel() {
            log.add("end");
        }

        @Override
        public boolean travelling() {
            return travelling;
        }

        @Override
        public void showAlready(ServerTarget target) {
            log.add("already " + target.displayName());
        }
    }

    private final FakeGame game = new FakeGame();
    private final List<JsonObject> replies = new ArrayList<>();
    private final Connector connector = new Connector(game, replies::add);

    private static JsonObject connect(String id, String host, int port) {
        JsonObject o = Protocol.message("connect");
        o.addProperty("id", id);
        o.addProperty("host", host);
        o.addProperty("port", port);
        return o;
    }

    private JsonObject lastReply() {
        return replies.get(replies.size() - 1);
    }

    @Test
    void fromAWorldTheToastComesFirstThenTheGameLeaves() {
        connector.request(connect("c1", "play.projects.gg", 25565), 1_000);
        assertEquals(List.of("travel Projects"), game.log);
        assertTrue(replies.isEmpty(), "no answer before the game leaves");
        connector.tick(1_000 + Connector.BEAT_MS - 1);
        assertEquals(1, game.log.size());
        connector.tick(1_000 + Connector.BEAT_MS);
        assertEquals("connect play.projects.gg", game.log.get(1));
        assertEquals("c1", Protocol.str(lastReply(), "id", ""));
        assertTrue(Protocol.bool(lastReply(), "ok", false));
        // The toast stays while the game is on its way, and leaves once it arrived.
        connector.tick(3_000);
        assertEquals(2, game.log.size());
        game.travelling = false;
        connector.tick(9_000);
        assertEquals("end", game.log.get(2));
    }

    @Test
    void theAnswerNeverWaitsForTheSave() {
        // A big world: leaving takes long, the launcher already has its answer.
        Connector slow = new Connector(new FakeGame() {
            @Override
            public boolean leaveAndConnect(ServerTarget target) {
                assertEquals(1, replies.size(), "ok goes out before the world is saved");
                return super.leaveAndConnect(target);
            }
        }, replies::add);
        slow.request(connect("c1", "play.projects.gg", 25565), 0);
        slow.tick(Connector.BEAT_MS);
        assertTrue(Protocol.bool(lastReply(), "ok", false));
        assertTrue(Connector.BEAT_MS + Connector.WAIT_MS < 5_000, "inside the launcher's five seconds");
    }

    @Test
    void fromTheMenusItGoesAtOnce() {
        game.inWorld = false;
        connector.request(connect("c1", "mc.ornek.net", 25570), 1_000);
        assertEquals(List.of("travel mc.ornek.net:25570", "connect mc.ornek.net:25570"), game.log);
        assertTrue(Protocol.bool(lastReply(), "ok", false));
    }

    @Test
    void busyWhileLoadingOrAnotherConnect() {
        game.busy = true;
        connector.request(connect("c1", "play.projects.gg", 25565), 1_000);
        assertEquals("busy", Protocol.str(lastReply(), "reason", ""));
        assertTrue(game.log.isEmpty());
        game.busy = false;
        connector.request(connect("c2", "play.projects.gg", 25565), 1_000);
        connector.request(connect("c3", "mc.ornek.net", 25565), 1_100);
        assertEquals("c3", Protocol.str(lastReply(), "id", ""));
        assertEquals("busy", Protocol.str(lastReply(), "reason", ""));
        assertFalse(connector.go(ServerTarget.parse("mc.ornek.net"), 1_200));
    }

    @Test
    void waitsForASaveThatStartedMeanwhileThenGivesUp() {
        connector.request(connect("c1", "play.projects.gg", 25565), 0);
        game.busy = true;
        connector.tick(Connector.BEAT_MS);
        connector.tick(Connector.BEAT_MS + Connector.WAIT_MS - 1);
        assertTrue(replies.isEmpty());
        game.busy = false;
        connector.tick(Connector.BEAT_MS + Connector.WAIT_MS - 1);
        assertTrue(Protocol.bool(lastReply(), "ok", false));

        replies.clear();
        game.inWorld = true;
        game.travelling = false;
        connector.tick(20_000);
        connector.request(connect("c2", "mc.ornek.net", 25565), 30_000);
        game.busy = true;
        connector.tick(30_000 + Connector.BEAT_MS + Connector.WAIT_MS);
        assertFalse(Protocol.bool(lastReply(), "ok", true));
        assertEquals("busy", Protocol.str(lastReply(), "reason", ""));
        assertEquals("end", game.log.get(game.log.size() - 1));
    }

    @Test
    void alreadyThereIsAnOkWithoutLeaving() {
        game.current = ServerTarget.parse("mc.projects.gg");
        connector.request(connect("c1", "play.projects.gg", 25565), 1_000);
        assertEquals(List.of("already Projects"), game.log);
        assertTrue(Protocol.bool(lastReply(), "ok", false));
        assertTrue(Protocol.bool(lastReply(), "already", false));
    }

    @Test
    void badRequestsAreInvalid() {
        connector.request(connect("c1", "mc ornek", 25565), 0);
        assertEquals("invalid", Protocol.str(lastReply(), "reason", ""));
        connector.request(connect("", "play.projects.gg", 25565), 0);
        assertEquals("invalid", Protocol.str(lastReply(), "reason", ""));
        JsonObject noHost = Protocol.message("connect");
        noHost.addProperty("id", "c3");
        connector.request(noHost, 0);
        assertEquals("c3", Protocol.str(lastReply(), "id", ""));
        assertEquals("invalid", Protocol.str(lastReply(), "reason", ""));
        // The game's own address rules have the last word.
        game.valid = false;
        connector.request(connect("c4", "play.projects.gg", 25565), 0);
        assertEquals("invalid", Protocol.str(lastReply(), "reason", ""));
        assertTrue(game.log.isEmpty());
        assertFalse(connector.go(ServerTarget.parse("play.projects.gg"), 0));
    }

    @Test
    void aGameThatRefusesEndsTheToast() {
        game.refuse = true;
        game.inWorld = false;
        connector.request(connect("c1", "mc.ornek.net", 25565), 0);
        assertEquals("end", game.log.get(game.log.size() - 1));
        assertFalse(connector.travelShown());
    }

    @Test
    void theOverlaysOwnJoinGoesAtOnceWithoutAnAnswer() {
        assertTrue(connector.go(ServerTarget.parse("play.projects.gg"), 0));
        assertEquals(List.of("travel Projects", "connect play.projects.gg"), game.log);
        assertTrue(replies.isEmpty());
    }

    @Test
    void theToastNeverOutstaysItsWelcome() {
        game.inWorld = false;
        connector.request(connect("c1", "mc.ornek.net", 25565), 0);
        connector.tick(Connector.TRAVEL_MAX_MS - 1);
        assertTrue(connector.travelShown());
        connector.tick(Connector.TRAVEL_MAX_MS + 1);
        assertFalse(connector.travelShown());
    }
}
