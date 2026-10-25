package gg.projects.overlay.connect;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** "Devam et" into a world through the overlay, once a frame followed the title screen (OVL-FIX). */
class WorldResumeTest {
    /** The game as the flow sees it, with a log of what was asked of it. */
    private static final class FakeGame implements WorldResume.Game {
        boolean title;
        boolean elsewhere;
        boolean exists = true;
        final List<String> log = new ArrayList<>();

        @Override
        public boolean onTitleScreen() {
            return title;
        }

        @Override
        public boolean elsewhere() {
            return elsewhere;
        }

        @Override
        public boolean exists(String world) {
            return exists;
        }

        @Override
        public void open(String world) {
            log.add("open " + world);
        }

        @Override
        public void missing(String world) {
            log.add("missing " + world);
        }
    }

    @Test
    void decodesTheLaunchersEncoding() {
        // The same vectors as pl-overlay's world_jvm_arg test.
        assertEquals("Yeni Dünya", WorldResume.decode("Yeni%20D%C3%BCnya"));
        assertEquals("Dünya (1)", WorldResume.decode("D%C3%BCnya%20%281%29"));
        assertEquals("a+b&c", WorldResume.decode("a%2Bb%26c"));
        assertEquals("New World-2_x.~", WorldResume.decode("New%20World-2_x.~"));
        assertEquals("Мир", WorldResume.decode("%D0%9C%D0%B8%D1%80"));
    }

    @Test
    void refusesWhatIsNotAWorldFolder() {
        for (String bad : new String[] {null, "", "%", "%2", "%zz", "%FF", "%C3", "Dünya", "..", ".", "a%2Fb",
            "a%5Cb", "%0A", "%20%20"}) {
            assertNull(WorldResume.of(bad), String.valueOf(bad));
        }
        assertNull(WorldResume.of("a".repeat(256)));
        assertNotNull(WorldResume.of("a".repeat(255)));
    }

    @Test
    void withAnEarlyTitleScreenTheWorldWaitsForTheFirstFrame() {
        // Remove Reloading Screen: the title screen is set up inside the game's start, before any frame.
        WorldResume resume = WorldResume.of("Yeni%20D%C3%BCnya");
        FakeGame game = new FakeGame();
        game.title = true;
        resume.tick(game, 0, 0);
        resume.titleScreen(0, 0);
        resume.tick(game, 0, 10);
        assertTrue(game.log.isEmpty());
        resume.tick(game, 1, 60);
        assertEquals(List.of("open Yeni Dünya"), game.log);
        assertNull(resume.pending());
        resume.tick(game, 2, 120);
        assertEquals(1, game.log.size());
    }

    @Test
    void inTheGamesOwnOrderTheWorldOpensRightAfterTheTitleScreen() {
        // The title screen comes under the fading loading screen, 40 frames in: one more frame, and the world opens.
        WorldResume resume = WorldResume.of("w");
        FakeGame game = new FakeGame();
        game.title = true;
        resume.titleScreen(40, 0);
        resume.tick(game, 40, 5);
        assertTrue(game.log.isEmpty());
        resume.tick(game, 41, 30);
        assertEquals(List.of("open w"), game.log);
    }

    @Test
    void withoutTheFrameCountTheTitleScreensOwnFrameStandsIn() {
        WorldResume resume = WorldResume.of("w");
        FakeGame game = new FakeGame();
        game.title = true;
        resume.titleScreen(0, 0);
        resume.tick(game, 0, 500);
        assertTrue(game.log.isEmpty());
        resume.titleFrame();
        resume.tick(game, 0, 2_000);
        assertEquals(List.of("open w"), game.log);
    }

    @Test
    void waitsForTheTitleScreenWhileAQuestionIsUp() {
        WorldResume resume = WorldResume.of("w");
        FakeGame game = new FakeGame();
        resume.titleScreen(10, 0);
        // A mod's question over the title screen: the world waits.
        resume.tick(game, 50, 1_000);
        assertTrue(game.log.isEmpty());
        game.title = true;
        resume.tick(game, 900, 30_000);
        assertEquals(List.of("open w"), game.log);
    }

    @Test
    void dropsItWhenTheTitleScreenComesTooLate() {
        WorldResume resume = WorldResume.of("w");
        FakeGame game = new FakeGame();
        resume.titleScreen(10, 0);
        resume.tick(game, 20, WorldResume.WAIT_MS + 1);
        assertNull(resume.pending());
        game.title = true;
        resume.tick(game, 30, WorldResume.WAIT_MS + 50);
        assertTrue(game.log.isEmpty());
    }

    @Test
    void dropsItWhenThePlayerWentElsewhereFirst() {
        WorldResume resume = WorldResume.of("w");
        FakeGame game = new FakeGame();
        game.elsewhere = true;
        resume.tick(game, 5, 0);
        assertNull(resume.pending());
        game.elsewhere = false;
        game.title = true;
        resume.titleScreen(6, 10);
        resume.tick(game, 7, 20);
        assertTrue(game.log.isEmpty());
    }

    @Test
    void aWorldThatIsGoneGetsANoteAndTheTitleScreenStays() {
        WorldResume resume = WorldResume.of("eski");
        FakeGame game = new FakeGame();
        game.exists = false;
        game.title = true;
        resume.titleScreen(3, 0);
        // Under the fading loading screen nobody would see the note: it waits for the title screen's own frame.
        resume.tick(game, 4, 50);
        assertTrue(game.log.isEmpty());
        resume.titleFrame();
        resume.tick(game, 5, 100);
        assertEquals(List.of("missing eski"), game.log);
        assertNull(resume.pending());
    }

    @Test
    void theFrameCountGoesUp() {
        long before = WorldResume.frames();
        WorldResume.frameStarted();
        assertEquals(before + 1, WorldResume.frames());
    }
}
