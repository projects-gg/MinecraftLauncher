package gg.projects.overlay.tweaks;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;
import org.junit.jupiter.api.Test;

/** The launch property's ids and the registry's start: the switches the launcher passes, each tweak on its own guard. */
class ClientTweaksTest {
    @Test
    void theLaunchersIdsAndPropertyAreTheMods() {
        // The launcher's side: pl_overlay::tweaks (TWEAKS_ARG, DIALOG_FOCUS).
        assertEquals("projects.overlay.tweaks", ClientTweaks.PROPERTY);
        assertEquals(List.of("dialog-focus"), List.copyOf(ClientTweaks.tweaks().keySet()));
    }

    @Test
    void parsesTheIdsTheLauncherNames() {
        assertEquals(Set.of(), ClientTweaks.parse(null));
        assertEquals(Set.of(), ClientTweaks.parse(""));
        assertEquals(Set.of(), ClientTweaks.parse("  "));
        assertEquals(List.of("dialog-focus"), List.copyOf(ClientTweaks.parse("dialog-focus")));
        // Trimmed, lowercase (whatever the system's language: no Turkish dotless i), each once, in order.
        assertEquals(List.of("dialog-focus", "later-tweak"),
            List.copyOf(ClientTweaks.parse(" DIALOG-FOCUS , later-tweak,,dialog-focus ")));
    }

    @Test
    void dropsMalformedIdsAndStopsAtSixteen() {
        assertEquals(List.of("ok"), List.copyOf(ClientTweaks.parse("a b,x=y,ünlü,-dash,ok," + "a".repeat(33))));
        StringBuilder many = new StringBuilder();
        for (int i = 0; i < 40; i++) {
            many.append("t").append(i).append(',');
        }
        assertEquals(16, ClientTweaks.parse(many.toString()).size());
    }

    /** A tweak for the registry's tests: counts its installs, or fails in it. */
    private static final class Fake implements ClientTweak {
        private final String id;
        private final boolean fails;
        int installs;

        Fake(String id, boolean fails) {
            this.id = id;
            this.fails = fails;
        }

        @Override
        public String id() {
            return id;
        }

        @Override
        public void install(TweakGuard guard) {
            installs++;
            if (fails) {
                throw new NoClassDefFoundError("net/minecraft/client/gui/screens/Gone");
            }
        }
    }

    @Test
    void startsOnlyTheTweaksSwitchedOnAndSkipsUnknownOnes() {
        Fake first = new Fake("first", false);
        Fake second = new Fake("second", false);
        Map<String, Supplier<ClientTweak>> tweaks = new LinkedHashMap<>();
        tweaks.put("first", () -> first);
        tweaks.put("second", () -> second);
        List<String> reported = new ArrayList<>();
        List<String> started = ClientTweaks.start(Set.of("second", "newer-than-this-jar"), tweaks,
            (id, problem) -> reported.add(id));
        assertEquals(List.of("second"), started);
        assertEquals(0, first.installs);
        assertEquals(1, second.installs);
        assertTrue(reported.isEmpty());
        // None named (another launcher, an older Projects Launcher): none starts.
        assertTrue(ClientTweaks.start(ClientTweaks.parse(null), tweaks, (id, problem) -> reported.add(id)).isEmpty());
        assertEquals(1, second.installs);
    }

    @Test
    void aTweakThatCannotStartStaysOffAndTheOthersGoOn() {
        Fake broken = new Fake("broken", true);
        Fake fine = new Fake("fine", false);
        Map<String, Supplier<ClientTweak>> tweaks = new LinkedHashMap<>();
        tweaks.put("broken", () -> broken);
        tweaks.put("fine", () -> fine);
        List<String> reported = new ArrayList<>();
        List<String> started = ClientTweaks.start(Set.of("broken", "fine"), tweaks, (id, problem) -> reported.add(id));
        assertEquals(List.of("fine"), started);
        assertEquals(List.of("broken"), reported);
        assertFalse(started.contains("broken"));
    }
}
