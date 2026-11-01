package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.lang.Language;
import java.util.List;
import org.junit.jupiter.api.Test;

/** BADGES-UI's {@code marks} catalogue and the badge keys of friends, messages and toasts, as the mod keeps them. */
class OverlayStateMarksTest {
    /** The built-in catalogue's first five badges, as the launcher sends them. */
    private static final String CATALOGUE = "{\"t\":\"marks\",\"defs\":["
        + def("admin", "Projects yöneticisi", "Projects admin", "red", "check", "seal") + ","
        + def("moderator", "Projects moderatörü", "Projects moderator", "purple", "check", "seal") + ","
        + def("staff", "Projects personeli", "Projects staff", "green", "check", "seal") + ","
        + def("verified", "Doğrulanmış hesap", "Verified account", "blue", "check", "seal") + ","
        + def("premium", "Premium Club üyesi", "Premium Club member", "orange", "star", "seal") + "]}";
    private final OverlayState state = new OverlayState();

    private static String def(String key, String tr, String en, String color, String glyph, String shape) {
        return "{\"key\":\"" + key + "\",\"name\":{\"tr\":\"" + tr + "\",\"en\":\"" + en + "\"},\"color\":\"" + color
            + "\",\"glyph\":\"" + glyph + "\",\"shape\":\"" + shape + "\"}";
    }

    private OverlayState.ToastRequest apply(String json) {
        return state.apply(Protocol.parse(json), 1_000);
    }

    private List<String> keys(List<OverlayState.MarkDef> marks) {
        return marks.stream().map(OverlayState.MarkDef::key).toList();
    }

    @Test
    void theModSaysItDrawsBadges() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"marks\""));
    }

    @Test
    void theCatalogueComesWholeAndInOrder() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Kuzey_77\"},\"features\":[\"friends\"]}");
        int before = state.badgeRevision();
        apply(CATALOGUE);
        assertNotEquals(before, state.badgeRevision(), "laid out lines are laid out again");
        assertEquals(List.of("admin", "moderator", "staff", "verified", "premium"), keys(state.markDefs()));
        assertEquals(new OverlayState.MarkDef("premium", "Premium Club üyesi", "Premium Club member", "orange", "star",
            "seal"), state.markDefs().get(4));
        // Studio's owner changes it: the next one replaces all of it.
        apply("{\"t\":\"marks\",\"defs\":[" + def("verified", "Doğrulanmış", "Verified", "blue", "check", "round")
            + "]}");
        assertEquals(List.of("verified"), keys(state.markDefs()));
        assertEquals("round", state.markDefs().get(0).shape());
        apply("{\"t\":\"marks\",\"defs\":[]}");
        assertTrue(state.markDefs().isEmpty());
    }

    @Test
    void aBadgeThisBuildCannotDrawOrNameIsLeftOut() {
        apply("{\"t\":\"marks\",\"defs\":["
            + def("gold", "Altın", "Gold", "gold", "check", "seal") + ","
            + def("skull", "Kafatası", "Skull", "red", "skull", "seal") + ","
            + def("hex", "Altıgen", "Hexagon", "red", "check", "hexagon") + ","
            + def("Bad Key", "Kötü", "Bad", "red", "check", "seal") + ","
            + def("nameless", "", "", "red", "check", "seal") + ","
            + "{\"key\":\"bare\",\"color\":\"red\",\"glyph\":\"check\",\"shape\":\"seal\"},"
            + "7,"
            + def("m12", "§cAr-Ge\\nekibi", "R&D team", "pink", "flask", "round") + ","
            + def("m12", "İkinci", "Second", "teal", "cloud", "round") + ","
            + def("m13", "İçerik üreticisi", "", "slate", "play", "shield") + "]}");
        assertEquals(List.of("m12", "m13"), keys(state.markDefs()), "only what this build draws and names, once");
        assertEquals("cAr-Ge ekibi", state.markDefs().get(0).tr(), "plain text: no formatting sign, no line break");
        assertEquals("İçerik üreticisi", state.markDefs().get(1).en(), "a missing name reads the other language's");
        // At most 31 badges, as the service's bit set holds.
        StringBuilder many = new StringBuilder("{\"t\":\"marks\",\"defs\":[");
        for (int i = 1; i <= 40; i++) {
            many.append(i == 1 ? "" : ",").append(def("m" + i, "R" + i, "B" + i, "blue", "star", "round"));
        }
        apply(many.append("]}").toString());
        assertEquals(Protocol.MAX_MARKS, state.markDefs().size());
    }

    @Test
    void theNameFollowsTheOverlaysLanguage() {
        apply(CATALOGUE);
        OverlayState.MarkDef admin = state.markDefs().get(0);
        assertEquals("Projects yöneticisi", admin.name(Language.TURKISH));
        assertEquals("Projects admin", admin.name(Language.ENGLISH));
    }

    @Test
    void aFriendsFirstKnownBadgeComesFirstInTheCataloguesOrder() {
        apply(CATALOGUE);
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kuzey_77\",\"where\":\"projects\","
            + "\"marks\":[\"m20\",\"premium\",\"admin\",\"admin\",\"NOT A KEY\",7]}");
        assertEquals(List.of("m20", "premium", "admin"), state.friend("f1").marks(), "readable keys, each once");
        List<OverlayState.MarkDef> marks = state.marks(state.friend("f1").marks());
        assertEquals(List.of("admin", "premium"), keys(marks), "a newer catalogue's badge is left out");
        assertEquals("admin", marks.get(0).key(), "the most important one is drawn");
        // A frame without them: they have none now.
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kuzey_77\",\"where\":\"projects\"}");
        assertTrue(state.friend("f1").marks().isEmpty());
        assertTrue(state.marks(state.friend("f1").marks()).isEmpty());
        // Friends in a snapshot carry them too.
        apply("{\"t\":\"snapshot\",\"friends\":[{\"id\":\"f2\",\"name\":\"NightOwl_42\",\"where\":\"offline\","
            + "\"marks\":[\"verified\"]}],\"convs\":[]}");
        assertEquals(List.of("verified"), keys(state.marks(state.friend("f2").marks())));
    }

    @Test
    void beforeTheCatalogueNoBadgeIsKnown() {
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kuzey_77\",\"where\":\"projects\",\"marks\":[\"admin\"]}");
        assertEquals(List.of("admin"), state.friend("f1").marks());
        assertTrue(state.marks(state.friend("f1").marks()).isEmpty());
        // The catalogue comes later (Studio's owner turned a badge on): the same keys name it now.
        apply(CATALOGUE);
        assertEquals(List.of("admin"), keys(state.marks(state.friend("f1").marks())));
        assertTrue(state.marks(null).isEmpty());
    }

    @Test
    void crewMessagesCarryTheirSendersBadgesAndTheLaunchersWordStands() {
        apply(CATALOGUE);
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:1\",\"from\":\"NightOwl_42\",\"text\":\"selam\",\"at\":5,"
            + "\"marks\":[\"staff\",\"verified\"]}");
        assertEquals(List.of("staff", "verified"), state.line("c:t1", "i:1").marks);
        // The same message again without them: they have none now.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:1\",\"from\":\"NightOwl_42\",\"text\":\"selam\",\"at\":5}");
        assertTrue(state.line("c:t1", "i:1").marks.isEmpty());
        // The player's own never carries them.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:2\",\"from\":\"Kuzey_77\",\"text\":\"hey\",\"at\":6,"
            + "\"mine\":true,\"marks\":[\"admin\"]}");
        assertTrue(state.line("c:t1", "i:2").marks.isEmpty());
        // In a history too.
        apply("{\"t\":\"history\",\"conv\":\"c:t1\",\"msgs\":[{\"id\":\"i:3\",\"from\":\"NightOwl_42\",\"text\":\"x\","
            + "\"at\":7,\"marks\":[\"premium\"]}]}");
        assertEquals(List.of("premium"), state.line("c:t1", "i:3").marks);
    }

    @Test
    void toastsCarryTheBadgesOfThePersonTheirTitleNames() {
        apply(CATALOGUE);
        OverlayState.ToastRequest message = apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:1\","
            + "\"title\":\"Kuzey_77\",\"body\":\"selam\",\"conv\":\"d:1\",\"marks\":[\"premium\",\"m20\",\"moderator\"]}");
        assertEquals(List.of("moderator", "premium"), keys(message.marks()));
        OverlayState.ToastRequest invite = apply("{\"t\":\"toast\",\"kind\":\"invite\",\"key\":\"invite:1\","
            + "\"title\":\"Kuzey_77\",\"body\":\"\",\"marks\":[\"verified\"]}");
        assertEquals(List.of("verified"), keys(invite.marks()));
        OverlayState.ToastRequest world = apply("{\"t\":\"toast\",\"kind\":\"world\",\"key\":\"world:1\","
            + "\"title\":\"Kuzey_77\",\"body\":\"\",\"marks\":[\"admin\"]}");
        assertEquals(List.of("admin"), keys(world.marks()));
        // A call's title is its crew, an info's is no one.
        OverlayState.ToastRequest call = apply("{\"t\":\"toast\",\"kind\":\"call\",\"key\":\"call:1\","
            + "\"title\":\"Gece Kuşları\",\"body\":\"NightOwl_42\",\"marks\":[\"admin\"]}");
        assertTrue(call.marks().isEmpty());
        OverlayState.ToastRequest info = apply("{\"t\":\"toast\",\"kind\":\"info\",\"key\":\"x\",\"title\":\"Bilgi\","
            + "\"body\":\"x\",\"marks\":[\"admin\"]}");
        assertTrue(info.marks().isEmpty());
        OverlayState.ToastRequest bare = apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:2\","
            + "\"title\":\"NightOwl_42\",\"body\":\"x\"}");
        assertTrue(bare.marks().isEmpty());
    }

    @Test
    void anotherAccountKeepsTheCatalogue() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Kuzey_77\"},\"features\":[\"friends\"]}");
        apply(CATALOGUE);
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"NightOwl_42\"},\"features\":[\"friends\"]}");
        assertEquals(5, state.markDefs().size(), "the catalogue is the service's, not the account's");
    }
}
