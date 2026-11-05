package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

/** OVL-SOC2's lists, answers and switches (docs/tasks/ovl2-protocol.md, "OVL-SOC2") as the mod mirrors them. */
class OverlayStateSoc2Test {
    private final OverlayState state = new OverlayState();

    private void apply(String json, long now) {
        state.apply(Protocol.parse(json), now);
    }

    private void welcome(String features) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":[" + features + "]}", 1_000);
    }

    private static final String ALL = "\"friends\",\"people\",\"people2\",\"people3\",\"people4\",\"people5\","
        + "\"crews2\",\"plans\",\"chat3\",\"cards2\"";

    @Test
    void theHelloSaysEveryNewFeature() {
        String hello = Protocol.hello("t", "1", "26.1.2", "tr_tr").toString();
        for (String feature : List.of("people5", "crews2", "plans", "chat3", "cards2")) {
            assertTrue(hello.contains("\"" + feature + "\""), feature);
        }
        String pickerOnly = Protocol.hello("t", "1", "26.1.2", "tr_tr", false).toString();
        assertFalse(pickerOnly.contains("people5"), "the emoji picker alone has no panel");
    }

    @Test
    void theManagementComesWithTheListsWholeEachTime() {
        welcome(ALL);
        assertTrue(state.people5() && state.crews2() && state.plansOn() && state.chat3() && state.cards2());
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],"
            + "\"outgoing\":[{\"id\":\"u-sent\",\"name\":\"BlokUstasi\"},{\"name\":\"no id\"}],"
            + "\"blocked\":[{\"id\":\"u-kotu\",\"name\":\"KotuOyuncu\"}],\"code\":\"PRJ-ABCD-2345\","
            + "\"invites\":[{\"crew\":\"t9\",\"name\":\"Akıncılar\",\"from\":\"Mert\",\"by\":\"f-mert\",\"members\":4}],"
            + "\"asks\":[{\"crew\":\"t1\",\"crewName\":\"Gece Kuşları\",\"id\":\"u-ask\",\"name\":\"YeniGelen\"}],"
            + "\"call\":\"needs-server\",\"callers\":[\"t1\"],"
            + "\"plans\":[{\"id\":\"p1\",\"crew\":\"t1\",\"at\":5000,\"start\":5000,\"end\":9000,\"by\":\"Ayşe\","
            + "\"note\":\"Bedwars\",\"my\":\"maybe\",\"yes\":2,\"maybe\":1,\"no\":0},"
            + "{\"id\":\"p2\",\"crew\":\"t1\",\"at\":1500,\"start\":1500,\"end\":1800,\"by\":\"\",\"my\":\"later\","
            + "\"yes\":0,\"maybe\":0,\"no\":0}],\"planners\":[\"t1\"],\"notes\":true}", 1_000);
        assertEquals(List.of(new OverlayState.Request("u-sent", "BlokUstasi")), state.outgoing());
        assertEquals("KotuOyuncu", state.blocked().get(0).name());
        assertEquals("PRJ-ABCD-2345", state.myCode());
        assertEquals(new OverlayState.Invite("t9", "Akıncılar", "Mert", "f-mert", 4), state.invites().get(0));
        assertEquals("Gece Kuşları", state.asks().get(0).crewName());
        assertEquals("needs-server", state.callPlace());
        assertTrue(state.mayCall("t1") && !state.mayCall("t2"));
        // A plan over by now is gone; one ahead counts against the three.
        List<OverlayState.Plan> plans = state.plans("t1", 2_000);
        assertEquals(1, plans.size());
        assertEquals("maybe", plans.get(0).my());
        assertNull(state.plans("t1", 1_000).get(1).my(), "an answer this build does not know is none");
        assertTrue(state.mayPlan("t1", 2_000) && state.planNotes());
        // The next lists without them: everything goes.
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[]}", 1_000);
        assertTrue(state.outgoing().isEmpty() && state.invites().isEmpty() && state.plans("t1", 0).isEmpty());
        assertNull(state.callPlace());
        assertEquals("", state.myCode());
    }

    @Test
    void aPressGoesOnceAndItsAnswerSaysHowItWent() {
        welcome(ALL);
        assertTrue(state.pressed("cancel-request", "u-sent", "BlokUstasi", null, "request_canceled"));
        assertFalse(state.pressed("cancel-request", "u-sent", "BlokUstasi", null, "request_canceled"),
            "a second press waits for the first one's answer");
        assertTrue(state.pressing("cancel-request", "u-sent"));
        apply("{\"t\":\"done\",\"act\":\"cancel-request\",\"id\":\"u-sent\",\"ok\":true}", 2_000);
        assertFalse(state.pressing("cancel-request", "u-sent"));
        OverlayState.PersonNote note = state.personNote(2_000);
        assertNotNull(note);
        assertEquals("request_canceled", note.key());
        assertEquals("BlokUstasi", note.name());
        assertFalse(note.error());
        // A request that waits for them, a crew that approves newcomers.
        state.pressed("add-code", "PRJ-1", "YeniArkadas", null, "now_friends");
        apply("{\"t\":\"done\",\"act\":\"add-code\",\"id\":\"PRJ-1\",\"ok\":true,\"queued\":true}", 3_000);
        assertEquals("request_sent", state.personNote(3_000).key());
        state.pressed("crew-join", "EKP-1", "Akıncılar", null, "crew_joined");
        apply("{\"t\":\"done\",\"act\":\"crew-join\",\"id\":\"EKP-1\",\"ok\":true,\"queued\":true}", 4_000);
        assertEquals("crew_requested", state.personNote(4_000).key());
        // A call refused for now: the wait is kept and said in minutes, calmly (a wait, not an error).
        state.pressed("crew-call", "t1", "Gece Kuşları", "c:t1", "crew_called");
        apply("{\"t\":\"done\",\"act\":\"crew-call\",\"id\":\"t1\",\"ok\":false,\"reason\":\"wait\",\"wait\":90}",
            5_000);
        note = state.personNote(5_000);
        assertEquals("call_wait", note.key());
        assertEquals("2", note.detail());
        assertFalse(note.error());
        assertEquals(95_000, state.callWaitUntil(5_000));
        // Reasons this build does not know read as a general failure.
        state.pressed("plan-make", "t1", "Bugün 21:00", null, "plan_made");
        apply("{\"t\":\"done\",\"act\":\"plan-make\",\"id\":\"t1\",\"ok\":false,\"reason\":\"meteor\"}", 6_000);
        assertEquals("failed", state.personNote(6_000).key());
        assertEquals("plan_full", OverlayState.failKey("plan-make", "full"));
        assertEquals("crew_full", OverlayState.failKey("crew-join", "full"));
        assertEquals("edit_old", OverlayState.failKey("edit", "old"));
    }

    @Test
    void whatBulFoundAndTheSwitches() {
        welcome(ALL);
        int before = state.foundRevision();
        apply("{\"t\":\"found\",\"code\":\"ekp abcd\",\"kind\":\"crew\",\"id\":\"t9\",\"name\":\"Akıncılar\","
            + "\"relation\":\"none\",\"members\":4,\"tag\":\"AKN\",\"approval\":true}", 1_000);
        OverlayState.Found found = state.found();
        assertEquals("crew", found.kind());
        assertTrue(found.approval() && !found.requested());
        assertEquals(4, found.members());
        assertTrue(state.foundRevision() > before);
        apply("{\"t\":\"found\",\"code\":\"x\",\"kind\":\"alien\"}", 1_000);
        assertEquals("none", state.found().kind());
        assertEquals("failed", state.found().reason());
        state.clearFound();
        assertNull(state.found());
        assertNull(state.prefs());
        apply("{\"t\":\"prefs\",\"favoritesOnline\":true,\"favoritesProjects\":false,\"crewCalls\":true,"
            + "\"crewChat\":false,\"joinRequests\":true}", 1_000);
        assertFalse(state.prefs().favoritesProjects());
        state.setPref("crewChat", true);
        assertTrue(state.prefs().crewChat());
        assertTrue(state.prefs().joinRequests(), "the others stay");
    }

    @Test
    void aPlanCardCarriesItsTime() {
        welcome(ALL);
        OverlayState.ToastRequest toast = state.apply(Protocol.parse("{\"t\":\"toast\",\"kind\":\"plan\","
            + "\"key\":\"plan:p1\",\"title\":\"Ayşe\",\"body\":\"Bedwars\",\"crew\":\"Gece Kuşları\","
            + "\"conv\":\"c:t1\",\"by\":\"f-ayse\",\"at\":1800000000000}"), 1_000);
        assertEquals("plan", toast.kind());
        assertEquals(1_800_000_000_000L, toast.at());
        assertEquals("f-ayse", toast.by());
        OverlayState.ToastRequest online = state.apply(Protocol.parse("{\"t\":\"toast\",\"kind\":\"online\","
            + "\"key\":\"online:f-ayse\",\"title\":\"Ayşe\",\"body\":\"\",\"by\":\"f-ayse\"}"), 1_000);
        assertEquals(0, online.at());
        assertEquals("f-ayse", online.by());
    }
}
