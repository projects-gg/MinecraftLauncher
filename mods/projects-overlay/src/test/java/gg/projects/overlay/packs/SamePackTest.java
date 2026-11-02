package gg.projects.overlay.packs;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.common.hash.HashCode;
import java.net.URI;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.Predicate;
import net.minecraft.client.resources.server.PackLoadFeedback;
import net.minecraft.client.resources.server.PackReloadConfig;
import net.minecraft.client.resources.server.ServerPackManager;
import net.minecraft.server.packs.DownloadQueue;
import org.junit.jupiter.api.Test;

/**
 * FASTPACK-LEAVE: a server pack pushed again unchanged within one connection, against the game's own pack manager
 * (its downloads, reloads and answers to the server recorded instead of done).
 */
class SamePackTest {
    private static final UUID PACK = UUID.fromString("3c6f8a52-1d0e-4b7a-9f21-6a5e0c4d8b17");
    private static final UUID OTHER = UUID.fromString("9d2e4f60-7a1b-4c3d-8e5f-0a1b2c3d4e5f");
    private static final HashCode SHA1 = HashCode.fromString("0123456789abcdef0123456789abcdef01234567");
    private static final HashCode NEWER = HashCode.fromString("89abcdef0123456789abcdef0123456789abcdef");

    /** The game's pack manager with what it does outside recorded: downloads, reloads, answers to the server. */
    private static final class Game {
        final List<String> said = new ArrayList<>();
        /** Every download batch the manager asked for. */
        final List<Map<UUID, DownloadQueue.DownloadRequest>> asked = new ArrayList<>();
        /** The batches not finished yet, each finishing with a file for every pack it asked for. */
        final List<Runnable> downloading = new ArrayList<>();
        final List<PackReloadConfig.Callbacks> reloads = new ArrayList<>();
        final ServerPackManager manager;
        /** The server packs the game's resources have selected (the game's pack repository): all of them by default. */
        Predicate<UUID> selected = id -> true;

        Game() {
            PackLoadFeedback feedback = new PackLoadFeedback() {
                @Override
                public void reportUpdate(UUID id, PackLoadFeedback.Update update) {
                    said.add(name(id) + " " + update);
                }

                @Override
                public void reportFinalResult(UUID id, PackLoadFeedback.FinalResult result) {
                    said.add(name(id) + " " + result);
                }
            };
            manager = new ServerPackManager(
                (requests, output) -> {
                    asked.add(requests);
                    downloading.add(() -> output.accept(cached(requests)));
                },
                feedback,
                reloads::add,
                () -> {
                },
                ServerPackManager.PackPromptStatus.ALLOWED);
        }

        private static DownloadQueue.BatchResult cached(Map<UUID, DownloadQueue.DownloadRequest> requests) {
            Map<UUID, Path> files = new HashMap<>();
            for (UUID id : requests.keySet()) {
                files.put(id, Path.of("downloads", id.toString(), "pack.zip"));
            }
            return new DownloadQueue.BatchResult(files, Set.of());
        }

        /** The server pushes a pack the way the game handles it with the mod: the mod's answer first. */
        void push(UUID id, HashCode hash) throws Exception {
            if (!answer(id, hash)) {
                manager.pushPack(id, URI.create("https://packs.example/" + id).toURL(), hash);
            }
            manager.tick();
        }

        boolean answer(UUID id, HashCode hash) {
            return SamePack.answer(manager, id, hash, selected);
        }

        /** Every download under way finishes (from the game's cache). */
        void downloaded() {
            List<Runnable> finishing = List.copyOf(downloading);
            downloading.clear();
            finishing.forEach(Runnable::run);
            manager.tick();
        }

        /** The resource reloads the game started succeed. */
        void reloaded() {
            List<PackReloadConfig.Callbacks> started = List.copyOf(reloads);
            reloads.clear();
            started.forEach(PackReloadConfig.Callbacks::onSuccess);
            manager.tick();
        }

        /** A pack pushed, downloaded and applied, the way a server's first push ends. */
        Game applied(UUID id, HashCode hash) throws Exception {
            push(id, hash);
            downloaded();
            reloaded();
            assertEquals(List.of(name(id) + " ACCEPTED", name(id) + " DOWNLOADED", name(id) + " APPLIED"), said);
            said.clear();
            return this;
        }

        private static String name(UUID id) {
            return id.equals(PACK) ? "pack" : "other";
        }
    }

    @Test
    void theSamePackAgainIsAppliedWithoutAReload() throws Exception {
        Game game = new Game().applied(PACK, SHA1);
        int downloads = game.asked.size();

        // Another of the network's servers sends it again: answered at once, no download, no reload.
        game.push(PACK, SHA1);
        assertEquals(List.of("pack ACCEPTED", "pack DOWNLOADED", "pack APPLIED"), game.said);
        assertEquals(downloads, game.asked.size());
        assertTrue(game.reloads.isEmpty());
        // And again: still the applied pack.
        game.said.clear();
        game.push(PACK, SHA1);
        assertEquals(List.of("pack ACCEPTED", "pack DOWNLOADED", "pack APPLIED"), game.said);
        assertTrue(game.reloads.isEmpty());
    }

    @Test
    void aChangedPackOrOneWithoutAHashLoadsAsUsual() throws Exception {
        Game game = new Game().applied(PACK, SHA1);
        game.push(PACK, NEWER);
        assertEquals(2, game.asked.size());
        assertEquals(List.of("pack ACCEPTED"), game.said);
        game.downloaded();
        assertEquals(1, game.reloads.size());
        game.reloaded();

        Game unhashed = new Game().applied(PACK, SHA1);
        unhashed.push(PACK, null);
        assertEquals(2, unhashed.asked.size());
    }

    @Test
    void anotherPackLoadsAsUsual() throws Exception {
        Game game = new Game().applied(PACK, SHA1);
        game.push(OTHER, SHA1);
        assertEquals(2, game.asked.size());
        assertEquals(List.of("other ACCEPTED"), game.said);
    }

    @Test
    void aPackStillLoadingIsLeftToTheGame() throws Exception {
        Game game = new Game();
        game.push(PACK, SHA1);
        // Downloading.
        assertFalse(game.answer(PACK, SHA1));
        game.downloaded();
        // Its reload under way.
        assertFalse(game.answer(PACK, SHA1));
        game.reloaded();
        assertTrue(game.answer(PACK, SHA1));
    }

    @Test
    void aPackUnderAnotherIsLeftToTheGame() throws Exception {
        // Two packs applied: the later one stacks above the first.
        Game game = new Game().applied(PACK, SHA1);
        game.push(OTHER, NEWER);
        game.downloaded();
        game.reloaded();
        game.said.clear();

        // The first again: the game would put it on top, so it reloads as the game does.
        game.push(PACK, SHA1);
        assertEquals(List.of("pack ACCEPTED"), game.said);
        assertEquals(3, game.asked.size());

        // The topmost again, unchanged: kept.
        Game top = new Game().applied(PACK, SHA1);
        top.push(OTHER, NEWER);
        top.downloaded();
        top.reloaded();
        top.said.clear();
        top.push(OTHER, NEWER);
        assertEquals(List.of("other ACCEPTED", "other DOWNLOADED", "other APPLIED"), top.said);
        assertTrue(top.reloads.isEmpty());
    }

    @Test
    void aPackTheGameDoesNotShowIsLoadedAgain() throws Exception {
        // A reload and its recovery both failed: the game still marks the pack active, its resources lack it.
        Game game = new Game().applied(PACK, SHA1);
        game.selected = id -> false;
        game.push(PACK, SHA1);
        assertEquals(List.of("pack ACCEPTED"), game.said);
        assertEquals(2, game.asked.size());
    }

    @Test
    void leavingTakesThePackAwayAndJoiningAgainLoadsIt() throws Exception {
        Game game = new Game().applied(PACK, SHA1);
        // What the game does when the player leaves (DownloadedPackSource.cleanupAfterDisconnect).
        game.manager.popAll();
        game.manager.resetPromptStatus();
        game.manager.tick();
        assertFalse(game.answer(PACK, SHA1));
        game.reloaded();
        assertFalse(game.answer(PACK, SHA1));

        // Joining again: the game's normal load, from its download cache.
        game.manager.allowServerPacks();
        game.push(PACK, SHA1);
        assertEquals(2, game.asked.size());
        game.downloaded();
        assertEquals(1, game.reloads.size());
    }

    @Test
    void aPackTheServerTookAwayLoadsAgain() throws Exception {
        Game game = new Game().applied(PACK, SHA1);
        game.manager.popPack(PACK);
        game.manager.tick();
        game.reloaded();
        game.push(PACK, SHA1);
        assertEquals(2, game.asked.size());
    }

    @Test
    void declinedPacksStayDeclined() throws Exception {
        Game game = new Game().applied(PACK, SHA1);
        // The player said no to a later prompt: the game declines every pack the server sends from then on.
        game.manager.rejectServerPacks();
        game.push(PACK, SHA1);
        assertEquals(List.of("pack DECLINED"), game.said);
    }

    @Test
    void onlyTheLaunchersOnSwitchesItOn() {
        assertTrue(SamePack.switchedOn("on"));
        assertTrue(SamePack.switchedOn(" ON "));
        assertFalse(SamePack.switchedOn(null));
        assertFalse(SamePack.switchedOn(""));
        assertFalse(SamePack.switchedOn("off"));
        assertFalse(SamePack.switchedOn("true"));
    }

    @Test
    void theRivalsAreTheModsThatKeepServerPacks() {
        assertEquals(List.of("rpc", "resourcepackchecker"), SamePack.RIVALS);
    }
}
