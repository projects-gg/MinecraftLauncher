package gg.projects.overlay.packs;

import com.google.common.hash.HashCode;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.function.Predicate;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.server.PackLoadFeedback;
import net.minecraft.client.resources.server.ServerPackManager;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * "Hızlı kaynak paketi yükleme" within one connection (FASTPACK-LEAVE). A server that pushes again the pack the game
 * already applied for this connection, unchanged (the same id and SHA-1: a network moving the player to another of its
 * servers, a transfer to a server with the same pack), is answered "applied" at once; the game would otherwise reload
 * every resource for the same bytes. Only when the game's own handling would end the same: the pack is the topmost of
 * the server's packs (the game moves a pushed pack to the top) and the game's resources have it selected now.
 * Everything else stays the game's own: a server's packs go when the player leaves (the reason the launcher no longer
 * installs ResourcePackCached), a changed pack, one without a hash or one under another loads as usual, declined packs
 * stay declined, and joining again loads the pack from the game's download cache.
 *
 * <p>On only while the launcher passes {@code -Dprojects.overlay.fastpacks=on} (its switch and the signed config's
 * {@code features.fastPacks} on, an official pack install) and no mod of the player's keeps server packs itself
 * ({@link #RIVALS}). The game keeps its pack list private: it is read through reflection, and should it change in a
 * later version the game simply reloads, as without the mod. Render thread only: the game pushes packs there.
 */
public final class SamePack {
    /** The JVM argument's system property: {@code on} while the launcher's switch is on. */
    public static final String PROPERTY = "projects.overlay.fastpacks";

    /** Mods that keep server packs themselves (ResourcePackCached, Server Resourcepack Checker): they decide alone. */
    static final List<String> RIVALS = List.of("rpc", "resourcepackchecker");

    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");

    private static Boolean enabled;
    private static Access access;
    private static boolean lookedUp;

    private SamePack() {
    }

    /** Whether the launcher switched it on and no rival mod is loaded; decided at the first push. */
    public static boolean enabled() {
        if (enabled == null) {
            enabled = switchedOn(System.getProperty(PROPERTY))
                && RIVALS.stream().noneMatch(FabricLoader.getInstance()::isModLoaded);
        }
        return enabled;
    }

    /** The property as the launcher passes it: on only for {@code on}. */
    static boolean switchedOn(String property) {
        return property != null && property.strip().equalsIgnoreCase("on");
    }

    /**
     * A server pushes pack {@code id} with {@code hash} (null: none, or not a SHA-1) to {@code manager}. When the game
     * holds that very pack applied for this connection, on top, the push is answered accepted, downloaded and applied,
     * and true says the caller drops it. False, with nothing said to the server, in every other case.
     */
    public static boolean answer(ServerPackManager manager, UUID id, HashCode hash) {
        return answer(manager, id, hash, SamePack::selected);
    }

    /** {@link #answer(ServerPackManager, UUID, HashCode)} with what the game's resources have selected told apart. */
    static boolean answer(ServerPackManager manager, UUID id, HashCode hash, Predicate<UUID> selected) {
        Access fields = access();
        if (fields == null || hash == null) {
            return false;
        }
        PackLoadFeedback feedback;
        try {
            if (!alreadyApplied(id, hash, fields.allowed(manager), fields.held(manager)) || !selected.test(id)) {
                return false;
            }
            feedback = fields.feedback(manager);
        } catch (ReflectiveOperationException | RuntimeException e) {
            LOG.warn("A server's pack list could not be read; its packs load as usual.", e);
            access = null;
            return false;
        }
        feedback.reportUpdate(id, PackLoadFeedback.Update.ACCEPTED);
        feedback.reportUpdate(id, PackLoadFeedback.Update.DOWNLOADED);
        feedback.reportFinalResult(id, PackLoadFeedback.FinalResult.APPLIED);
        LOG.info("Server resource pack {} came again unchanged: kept without a reload.", id);
        return true;
    }

    /** One pack the game holds for the server, as {@link #answer} reads it. */
    record Held(UUID id, HashCode hash, boolean removed, boolean accepted, boolean downloaded, boolean active) {
    }

    /**
     * Whether a push of {@code id} with {@code hash} asks for a pack the game holds applied for this connection,
     * unchanged and where the push would put it: server packs are allowed, every pack of that id the game still holds is
     * accepted, downloaded, active and of that hash (the game holds one at most: a push replaces the earlier ones), and
     * no pack the game still holds comes after it. A push puts its pack last, and later packs stack above earlier ones,
     * so a pack under another is left to the game, which reloads with it on top.
     */
    static boolean alreadyApplied(UUID id, HashCode hash, boolean allowed, List<Held> held) {
        if (!allowed || hash == null) {
            return false;
        }
        boolean found = false;
        for (Held pack : held) {
            if (pack.removed()) {
                continue;
            }
            if (!pack.id().equals(id)) {
                if (found) {
                    return false;
                }
                continue;
            }
            if (!pack.accepted() || !pack.downloaded() || !pack.active() || !hash.equals(pack.hash())) {
                return false;
            }
            found = true;
        }
        return found;
    }

    /**
     * Whether the game's resources have server pack {@code id} selected now, as the last reload applied it (the game
     * names it {@code server/<serial>/<id>}). After a reload whose recovery failed too, the game still marks its packs
     * active without them: such a pack is loaded again instead.
     */
    static boolean selected(UUID id) {
        String tail = "/" + id;
        for (String pack : Minecraft.getInstance().getResourcePackRepository().getSelectedIds()) {
            if (pack.startsWith("server/") && pack.endsWith(tail)) {
                return true;
            }
        }
        return false;
    }

    private static Access access() {
        if (!lookedUp) {
            lookedUp = true;
            access = Access.lookUp();
        }
        return access;
    }

    /** The game's private pack state ({@code ServerPackManager} and its {@code ServerPackData}), looked up once. */
    private static final class Access {
        private final Field packs;
        private final Field feedback;
        private final Field prompt;
        private final Field id;
        private final Field hash;
        private final Field removal;
        private final Field download;
        private final Field activation;
        private final Field accepted;

        private Access(Class<?> manager, Class<?> data) throws NoSuchFieldException {
            packs = open(manager, "packs");
            feedback = open(manager, "packLoadFeedback");
            prompt = open(manager, "packPromptStatus");
            id = open(data, "id");
            hash = open(data, "hash");
            removal = open(data, "removalReason");
            download = open(data, "downloadStatus");
            activation = open(data, "activationStatus");
            accepted = open(data, "promptAccepted");
        }

        /** Null, said once in the log, when the game's classes are not the shape this mod knows. */
        static Access lookUp() {
            try {
                Class<?> manager = ServerPackManager.class;
                Class<?> data = Class.forName(manager.getName() + "$ServerPackData", false, manager.getClassLoader());
                return new Access(manager, data);
            } catch (ReflectiveOperationException | RuntimeException e) {
                LOG.warn("The game's server pack list is not the one this mod knows; a server's packs load as usual.", e);
                return null;
            }
        }

        private static Field open(Class<?> owner, String name) throws NoSuchFieldException {
            Field field = owner.getDeclaredField(name);
            field.setAccessible(true);
            return field;
        }

        boolean allowed(ServerPackManager manager) throws IllegalAccessException {
            return prompt.get(manager) == ServerPackManager.PackPromptStatus.ALLOWED;
        }

        PackLoadFeedback feedback(ServerPackManager manager) throws IllegalAccessException {
            return (PackLoadFeedback) feedback.get(manager);
        }

        List<Held> held(ServerPackManager manager) throws IllegalAccessException {
            List<Held> held = new ArrayList<>();
            for (Object pack : (List<?>) packs.get(manager)) {
                held.add(new Held(
                    (UUID) id.get(pack),
                    (HashCode) hash.get(pack),
                    removal.get(pack) != null,
                    accepted.getBoolean(pack),
                    named(download.get(pack), "DONE"),
                    named(activation.get(pack), "ACTIVE")));
            }
            return held;
        }

        private static boolean named(Object value, String name) {
            return value instanceof Enum<?> constant && constant.name().equals(name);
        }
    }
}
