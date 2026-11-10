package gg.projects.overlay.tweaks;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;
import java.util.function.Supplier;
import java.util.regex.Pattern;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The overlay's client tweaks (DIALOG-FOCUS): small fixes to the game's own screens, each with its own switch in the
 * launcher, on by default there. The launcher names the ones on in one launch property,
 * {@code -Dprojects.overlay.tweaks=dialog-focus} (comma-separated ids); absent means none, so a game started by another
 * launcher, or by a Projects Launcher from before the tweaks, has none. Unknown ids are skipped: a launcher newer than
 * the jar may name tweaks the jar does not have.
 *
 * <p>The tweaks start before anything else of the mod and need nothing of it: they run with the panel, with the chat's
 * emoji picker alone, with neither (the launcher's third mode), and without the launcher's bridge (the launcher closed
 * after the start with "Kapansın", or never reachable). Each runs under its own {@link TweakGuard}.
 *
 * <p>Adding a tweak: a class implementing {@link ClientTweak} in this package, its line in {@link #tweaks()}, and its
 * switch in the launcher (a field of `pl_overlay::tweaks::ClientTweaks` with the same id, its settings row and words).
 */
public final class ClientTweaks {
    /** The launch property naming the tweaks that are on. */
    public static final String PROPERTY = "projects.overlay.tweaks";
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    /** A well-formed id; anything else in the property is skipped. */
    private static final Pattern ID = Pattern.compile("[a-z0-9][a-z0-9-]{0,31}");
    /** Ids read from the property at most (it is the launcher's, never long). */
    private static final int MAX_IDS = 16;

    private ClientTweaks() {
    }

    /**
     * Every tweak this jar has, by id, in the order they start. Each is made only when the launcher switched it on, so a
     * tweak that is off never loads its classes.
     */
    static Map<String, Supplier<ClientTweak>> tweaks() {
        Map<String, Supplier<ClientTweak>> all = new LinkedHashMap<>();
        all.put(DialogFocus.ID, () -> new DialogFocus());
        return Collections.unmodifiableMap(all);
    }

    /**
     * Starts the tweaks the launch property switched on. Never throws: a tweak that cannot start stays off, the others
     * and the game go on.
     */
    public static void start() {
        try {
            start(parse(System.getProperty(PROPERTY)), tweaks(), ClientTweaks::report);
        } catch (RuntimeException | LinkageError problem) {
            LOG.warn("The client tweaks did not start; the game goes on without them", problem);
        }
    }

    /** Starts the tweaks of {@code tweaks} that {@code on} names, each under its guard; answers the ids that started. */
    static List<String> start(Set<String> on, Map<String, Supplier<ClientTweak>> tweaks,
                              BiConsumer<String, Throwable> report) {
        List<String> started = new ArrayList<>();
        for (Map.Entry<String, Supplier<ClientTweak>> entry : tweaks.entrySet()) {
            if (!on.contains(entry.getKey())) {
                continue;
            }
            TweakGuard guard = new TweakGuard(entry.getKey(), report);
            guard.run(() -> entry.getValue().get().install(guard));
            if (guard.on()) {
                started.add(entry.getKey());
            }
        }
        List<String> unknown = on.stream().filter(id -> !tweaks.containsKey(id)).toList();
        if (!started.isEmpty()) {
            LOG.info("Client tweaks on: {}", String.join(", ", started));
        }
        if (!unknown.isEmpty()) {
            LOG.info("Client tweaks this overlay does not have (skipped): {}", String.join(", ", unknown));
        }
        return started;
    }

    /**
     * The ids the property names, in its order, each once: trimmed and in lowercase; empty and malformed ones dropped,
     * at most {@value #MAX_IDS}. Absent or blank: none.
     */
    static Set<String> parse(String property) {
        Set<String> ids = new LinkedHashSet<>();
        if (property == null || property.isBlank()) {
            return ids;
        }
        for (String part : property.split(",")) {
            if (ids.size() == MAX_IDS) {
                break;
            }
            String id = part.strip().toLowerCase(Locale.ROOT);
            if (ID.matcher(id).matches()) {
                ids.add(id);
            }
        }
        return ids;
    }

    private static void report(String id, Throwable problem) {
        LOG.warn("The {} tweak stopped after an error; the game goes on without it", id, problem);
    }
}
