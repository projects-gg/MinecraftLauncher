package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

/** TEAM6's frames and fields (docs/tasks/ovl2-protocol.md, TEAM6-O: T1-T7) as the mod mirrors them. */
class OverlayStateTeam6Test {
    private static final String GK = "{\"tag\":\"GK\",\"color\":\"navy\",\"frame\":\"shield\",\"emblem\":\"moon\","
        + "\"name\":\"Gece Kartalları\",\"motto\":\"Gece bizimdir\",\"since\":\"2026-09\",\"members\":12}";
    private final OverlayState state = new OverlayState();

    private OverlayState.ToastRequest apply(String json) {
        return state.apply(Protocol.parse(json), 1_000);
    }

    private void welcome(String name, String features) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"" + name + "\"},\"features\":" + features + "}");
    }

    @Test
    void theModSaysItDrawsTags() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"tags\""));
    }

    @Test
    void cardsComeWholeThenChangeOneByOne() {
        welcome("Ece", "[\"friends\",\"chat\",\"tags\"]");
        int before = state.badgeRevision();
        apply("{\"t\":\"badges\",\"all\":true,\"cards\":{\"t9\":" + GK + ",\"t1\":{\"tag\":\"GKŞ\",\"name\":\"Gece "
            + "Kuşları\",\"members\":3,\"mine\":true}}}");
        assertNotEquals(before, state.badgeRevision());
        OverlayState.Badge gk = state.badge("t9");
        assertEquals(new OverlayState.Badge("t9", "GK", "navy", "shield", "moon", "Gece Kartalları", "Gece bizimdir",
            "2026-09", 12, false), gk);
        assertTrue(state.badge("t1").mine());
        assertEquals("", state.badge("t1").since(), "no month, no founding line");
        // A change names one crew; null takes one away.
        apply("{\"t\":\"badges\",\"all\":false,\"cards\":{\"t9\":" + GK.replace("Gece bizimdir", "Şafakta buluşuruz")
            + ",\"t1\":null}}");
        assertEquals("Şafakta buluşuruz", state.badge("t9").motto());
        assertNull(state.badge("t1"));
        // The whole set again replaces what is here.
        apply("{\"t\":\"badges\",\"all\":true,\"cards\":{}}");
        assertNull(state.badge("t9"));
    }

    @Test
    void aCardWithoutATagOrAFormattingTrickIsNoCard() {
        welcome("Ece", "[\"friends\",\"tags\"]");
        apply("{\"t\":\"badges\",\"all\":true,\"cards\":{\"a\":{\"tag\":\"\",\"name\":\"X\"},"
            + "\"b\":{\"tag\":\"§cAB\\nCDEF\",\"name\":\"§aGece\",\"since\":\"9/2026\",\"members\":-4},"
            + "\"c\":7}}");
        assertNull(state.badge("a"));
        assertNull(state.badge("c"));
        OverlayState.Badge b = state.badge("b");
        assertEquals("cAB", b.tag(), "plain text (no formatting sign, no line break), four characters at most");
        assertEquals("aGece", b.name());
        assertEquals("", b.since());
        assertEquals(0, b.members());
    }

    @Test
    void tagsShowOnlyWhileTheLauncherSaysTheyAreOn() {
        welcome("Ece", "[\"friends\",\"tags\"]");
        apply("{\"t\":\"badges\",\"all\":true,\"cards\":{\"t9\":" + GK + "}}");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"joinable\":true,"
            + "\"badge\":\"t9\"}");
        assertEquals("t9", state.friend("f1").badge());
        assertEquals("GK", state.badge(state.friend("f1").badge()).tag());
        // Studio's switch: the next welcome has no "tags", and nothing shows.
        int before = state.badgeRevision();
        welcome("Ece", "[\"friends\"]");
        assertFalse(state.tagsOn());
        assertNull(state.badge("t9"));
        assertNotEquals(before, state.badgeRevision(), "laid out lines are laid out again");
    }

    @Test
    void crewMessagesCarryTheirSendersTagAndTheLaunchersWordStands() {
        welcome("Ece", "[\"friends\",\"chat\",\"tags\"]");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:1\",\"from\":\"Mert\",\"text\":\"selam\",\"at\":5,"
            + "\"badge\":\"t9\"}");
        assertEquals("t9", state.line("c:t1", "i:1").badge);
        // The same message again without it: they took it off.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:1\",\"from\":\"Mert\",\"text\":\"selam\",\"at\":5}");
        assertNull(state.line("c:t1", "i:1").badge);
        // The player's own never carries one.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:2\",\"from\":\"Ece\",\"text\":\"hey\",\"at\":6,"
            + "\"mine\":true,\"badge\":\"t9\"}");
        assertNull(state.line("c:t1", "i:2").badge);
    }

    @Test
    void aMessageToastCarriesTheSendersCard() {
        welcome("Ece", "[\"friends\",\"chat\",\"tags\"]");
        apply("{\"t\":\"badges\",\"all\":true,\"cards\":{\"t9\":" + GK + "}}");
        OverlayState.ToastRequest message = apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:1\","
            + "\"title\":\"Ayşe\",\"body\":\"selam\",\"conv\":\"d:1\",\"badge\":\"t9\"}");
        assertEquals("GK", message.badge().tag());
        OverlayState.ToastRequest call = apply("{\"t\":\"toast\",\"kind\":\"call\",\"key\":\"call:1\","
            + "\"title\":\"Gece Kuşları\",\"body\":\"Mert\",\"badge\":\"t9\"}");
        assertNull(call.badge(), "a call is a sentence: no chip");
        OverlayState.ToastRequest unknown = apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:2\","
            + "\"title\":\"Can\",\"body\":\"x\",\"badge\":\"nope\"}");
        assertNull(unknown.badge());
    }

    @Test
    void joinLinesKeepTheirOrderOnceEachAndGoWithTheAccount() {
        welcome("Ece", "[\"friends\",\"chat\",\"tags\"]");
        int revision = state.lineRevision("c:t1");
        apply("{\"t\":\"note\",\"conv\":\"c:t1\",\"id\":\"j:b:20\",\"kind\":\"joined\",\"name\":\"Deniz\",\"at\":20000}");
        apply("{\"t\":\"note\",\"conv\":\"c:t1\",\"id\":\"j:a:10\",\"kind\":\"joined\",\"name\":\"Ece\",\"mine\":true,"
            + "\"at\":10000}");
        apply("{\"t\":\"note\",\"conv\":\"c:t1\",\"id\":\"j:b:20\",\"kind\":\"joined\",\"name\":\"Deniz\",\"at\":20000}");
        apply("{\"t\":\"note\",\"conv\":\"c:t1\",\"id\":\"x\",\"kind\":\"left\",\"name\":\"Can\",\"at\":30000}");
        assertNotEquals(revision, state.lineRevision("c:t1"));
        List<OverlayState.Arrival> joins = state.arrivals("c:t1");
        assertEquals(List.of(new OverlayState.Arrival("j:a:10", "Ece", true, 10_000),
            new OverlayState.Arrival("j:b:20", "Deniz", false, 20_000)), joins);
        for (int i = 0; i < OverlayState.MAX_ARRIVALS + 5; i++) {
            apply("{\"t\":\"note\",\"conv\":\"c:t1\",\"id\":\"j:" + i + "\",\"kind\":\"joined\",\"name\":\"N" + i
                + "\",\"at\":" + (40_000 + i) + "}");
        }
        assertEquals(OverlayState.MAX_ARRIVALS, state.arrivals("c:t1").size(), "the oldest go first");
        welcome("Mert", "[\"friends\",\"chat\",\"tags\"]");
        assertTrue(state.arrivals("c:t1").isEmpty(), "another account sees none of them");
    }
}
