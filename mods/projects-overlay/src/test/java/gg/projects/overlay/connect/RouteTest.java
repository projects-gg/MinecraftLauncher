package gg.projects.overlay.connect;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** The Projects sub-server picked beside "Doğrudan sunucuya bağlan", sent once after the lobby's ready (AUTOJOIN). */
class RouteTest {
    /** The same vector as pl-overlay's route_jvm_arg test. */
    private static final String SKYBLOCK =
        "%7B%22v%22%3A1%2C%22name%22%3A%22SkyBlock%22%2C%22command%22%3A%22%2Faurora%22%7D";

    /** The game as the flow sees it, with a log of what was asked of it. */
    private static final class FakeGame implements Route.Game {
        boolean projects = true;
        final List<String> log = new ArrayList<>();

        @Override
        public boolean onProjects() {
            return projects;
        }

        @Override
        public void sendCommand(String command) {
            log.add("command " + command);
        }

        @Override
        public void showTravel(String name) {
            log.add("travel " + name);
        }

        @Override
        public void endTravel() {
            log.add("end");
        }
    }

    @Test
    void readsTheLaunchersPick() {
        Route route = Route.of(SKYBLOCK);
        assertNotNull(route);
        assertEquals("SkyBlock", route.name());
        assertEquals("/aurora", route.command());
        Route hg = Route.of("%7B%22v%22%3A1%2C%22name%22%3A%22A%C3%A7l%C4%B1k%20Oyunlar%C4%B1%22%2C%22command%22%3A%22%2Fhg%22%7D");
        assertNotNull(hg);
        assertEquals("Açlık Oyunları", hg.name());
    }

    @Test
    void refusesWhatTheLauncherNeverSends() {
        for (String bad : new String[] {null, "", "%", "%zz", "%FF", "{}", "%5B%5D",
            pick(2, "SkyBlock", "/aurora"), pick(1, "", "/aurora"), pick(1, " SkyBlock", "/aurora"),
            pick(1, "SkyBlock", "aurora"), pick(1, "SkyBlock", "//aurora"), pick(1, "SkyBlock", "/ aurora"),
            pick(1, "SkyBlock", "/aurora "), pick(1, "SkyBlock", "/a  b"), pick(1, "SkyBlock", "/a\nb"),
            pick(1, "SkyBlock", "/§aaurora"), pick(1, "SkyBlock", "/a b"), pick(1, "SkyBlock", "/a"),
            pick(1, "SkyBlock", "/" + "a".repeat(Route.MAX_COMMAND_CHARS)),
            pick(1, "x".repeat(Route.MAX_NAME_CHARS + 1), "/aurora")}) {
            assertNull(Route.of(bad), String.valueOf(bad));
        }
        assertTrue(Route.validCommand("/testserver sky"));
        assertFalse(Route.validCommand("/a\tb"));
    }

    @Test
    void sendsTheCommandOnceAfterTheLobbysReady() {
        FakeGame game = new FakeGame();
        Route route = Route.of(SKYBLOCK);
        // Nothing before the game is anywhere, nor before the lobby says so.
        route.ready(game, 0);
        assertTrue(game.log.isEmpty());
        route.joined(game);
        assertTrue(route.waiting());
        route.ready(game, 1_000);
        assertEquals(List.of("command aurora", "travel SkyBlock"), game.log);
        assertFalse(route.waiting());
        // A second ready (the lobby again later) finds nothing to send.
        route.ready(game, 1_500);
        assertEquals(2, game.log.size());
        // The proxy moved the game to SkyBlock: the toast goes and so does the route.
        route.joined(game);
        assertEquals("end", game.log.get(2));
        assertTrue(route.finished());
        route.joined(game);
        route.ready(game, 2_000);
        assertEquals(3, game.log.size());
    }

    @Test
    void nothingWhenThePlayerWentSomewhereFirst() {
        // The game's first place is not Projects (singleplayer, another server).
        FakeGame elsewhere = new FakeGame();
        elsewhere.projects = false;
        Route first = Route.of(SKYBLOCK);
        first.joined(elsewhere);
        assertTrue(first.finished());
        elsewhere.projects = true;
        first.ready(elsewhere, 0);
        assertTrue(elsewhere.log.isEmpty());

        // The proxy moved the game before the lobby's ready.
        FakeGame moved = new FakeGame();
        Route second = Route.of(SKYBLOCK);
        second.joined(moved);
        second.joined(moved);
        second.ready(moved, 0);
        assertTrue(second.finished());
        assertTrue(moved.log.isEmpty());

        // The connection ended before it.
        FakeGame left = new FakeGame();
        Route third = Route.of(SKYBLOCK);
        third.joined(left);
        third.disconnected(left);
        third.ready(left, 0);
        assertTrue(third.finished());
        assertTrue(left.log.isEmpty());

        // A ready while the game is no longer on Projects.
        FakeGame off = new FakeGame();
        Route fourth = Route.of(SKYBLOCK);
        fourth.joined(off);
        off.projects = false;
        fourth.ready(off, 0);
        assertTrue(off.log.isEmpty());
    }

    @Test
    void theToastNeverStaysLong() {
        FakeGame game = new FakeGame();
        Route route = Route.of(SKYBLOCK);
        route.joined(game);
        route.ready(game, 1_000);
        route.tick(game, 1_000 + Route.TRAVEL_MS - 1);
        assertFalse(route.finished());
        // A full server queues the player in the lobby: no move comes.
        route.tick(game, 1_000 + Route.TRAVEL_MS);
        assertTrue(route.finished());
        assertEquals("end", game.log.get(game.log.size() - 1));
    }

    @Test
    void readsTheLobbysMessages() {
        byte[] ready = RouteMessages.bytes("{\"t\":\"ready\",\"v\":1}");
        // What Guava's ByteStreams.newDataOutput().writeUTF writes in the lobby's Skript.
        byte[] json = "{\"t\":\"ready\",\"v\":1}".getBytes(StandardCharsets.UTF_8);
        byte[] expected = new byte[json.length + 2];
        expected[1] = (byte) json.length;
        System.arraycopy(json, 0, expected, 2, json.length);
        assertArrayEquals(expected, ready);
        assertTrue(RouteMessages.ready(RouteMessages.text(ready)));
        assertFalse(RouteMessages.ready(RouteMessages.text(RouteMessages.bytes("{\"t\":\"later\"}"))));
        assertFalse(RouteMessages.ready("not json"));
        assertFalse(RouteMessages.ready("[]"));
        assertEquals("", RouteMessages.text(new byte[] {0}));
        assertEquals("", RouteMessages.text(new byte[] {0, 5, 'a'}));
        assertEquals("", RouteMessages.text(new byte[RouteMessages.MAX_BYTES + 3]));
    }

    /** A pick in the launcher's encoding: its JSON (Gson escapes what needs it), percent-encoded byte by byte. */
    private static String pick(int v, String name, String command) {
        JsonObject o = new JsonObject();
        o.addProperty("v", v);
        o.addProperty("name", name);
        o.addProperty("command", command);
        StringBuilder out = new StringBuilder();
        for (byte b : o.toString().getBytes(StandardCharsets.UTF_8)) {
            int c = b & 0xFF;
            if (c < 0x80 && (Character.isLetterOrDigit(c) || c == '-' || c == '.' || c == '_' || c == '~')) {
                out.append((char) c);
            } else {
                out.append(String.format("%%%02X", c));
            }
        }
        return out.toString();
    }
}
