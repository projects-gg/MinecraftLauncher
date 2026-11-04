package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import gg.projects.overlay.bridge.Protocol;
import org.junit.jupiter.api.Test;

/** OVL-AVATARS: whose picture a toast's face shows (docs/tasks/ovl2-protocol.md). */
class OverlayStateAvatarsTest {
    private final OverlayState state = new OverlayState();

    private OverlayState.ToastRequest apply(String json) {
        return state.apply(Protocol.parse(json), 1_000);
    }

    @Test
    void aToastNamesThePersonItsTitleNames() {
        assertEquals("f-ayse", apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:1\",\"title\":\"PixelAvci\","
            + "\"body\":\"gel\",\"conv\":\"d:1\",\"by\":\"f-ayse\"}").by());
        assertEquals("f-mert", apply("{\"t\":\"toast\",\"kind\":\"invite\",\"key\":\"inv-1\",\"title\":\"NetherKral\","
            + "\"body\":\"Projects'e davet etti\",\"by\":\"f-mert\"}").by());
        // A call's title is its crew, an info toast names nobody; an older launcher sends none.
        assertNull(apply("{\"t\":\"toast\",\"kind\":\"call\",\"key\":\"k1\",\"title\":\"Gece Kuşları\","
            + "\"body\":\"Projects'te buluşalım\",\"by\":\"u-cem\"}").by());
        assertNull(apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:2\",\"title\":\"KorAtesi\","
            + "\"body\":\"selam\",\"conv\":\"d:2\"}").by());
        assertNull(apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:3\",\"title\":\"KorAtesi\","
            + "\"body\":\"selam\",\"conv\":\"d:3\",\"by\":\"" + "x".repeat(65) + "\"}").by(), "no id is that long");
    }
}
