package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** OVL-AVATARS: who the faces ask for, what the launcher's answers keep, and what goes when (docs/tasks/ovl2-protocol.md). */
class AvatarBookTest {
    private final AvatarBook<String, String> book = new AvatarBook<>();
    private final List<String> released = new ArrayList<>();
    private final List<String> abandoned = new ArrayList<>();
    private final AvatarBook.Sink<String, String> sink = new AvatarBook.Sink<>() {
        @Override
        public void release(String texture) {
            released.add(texture);
        }

        @Override
        public void abandon(String pending) {
            abandoned.add(pending);
        }
    };

    private AvatarBook.Entry<String, String> draw(String id, long now) {
        return book.drawn(id, now, sink);
    }

    @Test
    void aFaceAsksOnceAndTheAsksGoTogether() {
        for (int i = 0; i < 40; i++) {
            draw("u-" + i, 1_000);
        }
        draw("u-0", 1_016);
        List<String> first = book.takeWants(32, 1_016);
        assertEquals(32, first.size());
        assertEquals("u-0", first.get(0));
        assertEquals(8, book.takeWants(32, 1_516).size(), "the rest with the next ask");
        assertTrue(book.takeWants(32, 2_016).isEmpty(), "nobody twice");
        // Asks that could not go are asked again when next drawn.
        book.unask(List.of("u-1"));
        draw("u-1", 1_032);
        assertEquals(List.of("u-1"), book.takeWants(32, 2_516));
    }

    @Test
    void anAskTheLauncherDidNotAnswerGoesAgainIfItsFaceStillShows() {
        draw("f-ayse", 1_000);
        draw("f-mert", 1_000);
        assertEquals(List.of("f-ayse", "f-mert"), book.takeWants(32, 1_000));
        // Mert's answer came, Ayşe's did not (the launcher did not hear it).
        book.answered("f-mert", AvatarBook.NONE, 1, null, 1_100, sink);
        draw("f-ayse", 1_000 + AvatarBook.ASK_AGAIN_MS - 1);
        draw("f-mert", 1_000 + AvatarBook.ASK_AGAIN_MS - 1);
        assertTrue(book.takeWants(32, 1_000 + AvatarBook.ASK_AGAIN_MS - 1).isEmpty(), "not yet");
        draw("f-ayse", 1_000 + AvatarBook.ASK_AGAIN_MS);
        draw("f-mert", 1_000 + AvatarBook.ASK_AGAIN_MS);
        // Waiting to go, it is not asked twice.
        draw("f-ayse", 1_010 + AvatarBook.ASK_AGAIN_MS);
        assertEquals(List.of("f-ayse"), book.takeWants(32, 1_010 + AvatarBook.ASK_AGAIN_MS),
            "only the unanswered, once");
        // Answered now: never again.
        book.answered("f-ayse", "v1", 2, "PNG", 1_100 + AvatarBook.ASK_AGAIN_MS, sink);
        draw("f-ayse", 1_000 + 3 * AvatarBook.ASK_AGAIN_MS);
        assertTrue(book.takeWants(32, 1_000 + 3 * AvatarBook.ASK_AGAIN_MS).isEmpty());
    }

    @Test
    void anAnswerKeepsThePictureAndAChangeLetsTheOldOneGo() {
        AvatarBook.Entry<String, String> e = draw("f-ayse", 0);
        book.answered("f-ayse", "v1", 1, "PNG1", 10, sink);
        assertEquals("PNG1", e.png);
        e.texture = "tex1";
        // The same picture again (asked again after a reconnect): the texture stays.
        book.answered("f-ayse", "v1", 2, "PNG1", 20, sink);
        assertEquals("tex1", e.texture);
        assertTrue(released.isEmpty());
        // Another picture: the old texture goes, the new one waits to be decoded.
        e.pending = "decoding";
        book.answered("f-ayse", "v2", 3, "PNG2", 30, sink);
        assertNull(e.texture);
        assertNull(e.pending);
        assertEquals(List.of("tex1"), released);
        assertEquals(List.of("decoding"), abandoned);
        assertEquals("PNG2", e.png);
        // An older answer coming after a newer one is dropped.
        book.answered("f-ayse", "v1", 2, "PNG1", 40, sink);
        assertEquals("v2", e.version);
        // None any more: the letter.
        book.answered("f-ayse", AvatarBook.NONE, 4, null, 50, sink);
        assertNull(e.png);
        assertEquals(0, e.missedAt, "nothing to ask again for");
        // Someone the book does not keep is not kept by an answer.
        book.answered("x-other", "v1", 5, "PNG", 60, sink);
        assertNull(book.entry("x-other"));
    }

    @Test
    void aPictureThatCouldNotBeHadIsAskedAgainLaterIfItStillShows() {
        draw("f-zey", 0);
        book.takeWants(32, 0);
        book.answered("f-zey", "v1", 1, null, 100, sink);
        assertTrue(book.entry("f-zey").missedAt > 0);
        draw("f-zey", 100 + AvatarBook.RETRY_MS - 1);
        assertTrue(book.takeWants(32, 100 + AvatarBook.RETRY_MS - 1).isEmpty(), "not yet");
        draw("f-zey", 100 + AvatarBook.RETRY_MS);
        assertEquals(List.of("f-zey"), book.takeWants(32, 100 + AvatarBook.RETRY_MS));
        // Had now: kept.
        book.answered("f-zey", "v1", 2, "PNG", 100 + AvatarBook.RETRY_MS + 50, sink);
        assertEquals("PNG", book.entry("f-zey").png);
        assertEquals(0, book.entry("f-zey").missedAt);
    }

    @Test
    void unseenPicturesGiveTheirTexturesBackAndKeepTheirBytes() {
        AvatarBook.Entry<String, String> seen = draw("f-ayse", 0);
        AvatarBook.Entry<String, String> gone = draw("f-mert", 0);
        seen.png = "PNG";
        seen.texture = "tex-ayse";
        gone.png = "PNG";
        gone.texture = "tex-mert";
        draw("f-ayse", AvatarBook.KEEP_MS);
        book.sweep(AvatarBook.KEEP_MS, sink);
        assertEquals(List.of("tex-mert"), released);
        assertNull(gone.texture);
        assertEquals("PNG", gone.png, "it comes back without asking");
        assertNotNull(seen.texture);
        // At most once a second.
        draw("f-ayse", 0);
        seen.drawnAt = 0;
        book.sweep(AvatarBook.KEEP_MS + 10, sink);
        assertNotNull(seen.texture);
    }

    @Test
    void withoutTheLauncherEveryFaceAsksAgainAndTheNewCountsAreHeard() {
        AvatarBook.Entry<String, String> e = draw("f-ayse", 0);
        book.takeWants(32, 0);
        book.answered("f-ayse", "v1", 7, "PNG", 10, sink);
        e.texture = "tex";
        book.offline();
        assertTrue(book.takeWants(32, 15).isEmpty());
        draw("f-ayse", 20);
        assertEquals(List.of("f-ayse"), book.takeWants(32, 20), "asked again");
        // The new connection counts from one; its answer is heard, the texture of the same picture stays.
        book.answered("f-ayse", "v1", 1, "PNG", 30, sink);
        assertEquals(1, e.n);
        assertEquals("tex", e.texture);
        // The world closed: textures go, pictures stay.
        book.releaseTextures(sink);
        assertEquals(List.of("tex"), released);
        assertEquals("PNG", e.png);
    }

    @Test
    void aHalvingWeighsByOpacity() {
        int red = 0xFFFF0000;
        int blue = 0xFF0000FF;
        assertEquals(0xFF800080, AvatarBook.halve(red, blue, red, blue));
        // A see-through pixel (black under it) neither darkens nor tints: the colour stays, the opacity goes down.
        assertEquals(0xBFFF0000, AvatarBook.halve(red, red, red, 0x00000000));
        assertEquals(0, AvatarBook.halve(0, 0, 0, 0));
        assertEquals(0xFF1B1820, AvatarBook.halve(0xFF1B1820, 0xFF1B1820, 0xFF1B1820, 0xFF1B1820));
    }

    @Test
    void pastTheCapThoseDrawnLongestAgoGo() {
        for (int i = 0; i < AvatarBook.CAP; i++) {
            draw("u-" + i, 1_000 + i).texture = "tex-" + i;
        }
        // u-0 shows again: u-1 is now the one drawn longest ago.
        draw("u-0", 5_000);
        draw("u-new", 5_001);
        assertEquals(AvatarBook.CAP, book.size());
        assertNull(book.entry("u-1"));
        assertNotNull(book.entry("u-0"));
        assertEquals(List.of("tex-1"), released);
    }
}
