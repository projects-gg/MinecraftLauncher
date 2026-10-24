package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.ui.draw.AtlasPacker;
import gg.projects.overlay.ui.draw.Emblems;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import org.junit.jupiter.api.Test;

/** The crews' looks as the launcher's crew-look.ts has them, and their marks in the shapes atlas (TEAM6). */
class CrewLookTest {
    @Test
    void unknownLooksFallBackAsTheLaunchersDo() {
        assertEquals(CrewLook.tone("gold"), CrewLook.tone("teal-ish"), "an unknown colour is gold");
        assertEquals(CrewLook.tone("gold"), CrewLook.tone(null));
        assertEquals("cut", CrewLook.shape("star").key(), "an unknown frame is the cut");
        assertEquals("shield", CrewLook.emblem("dragon"), "an unknown emblem is a shield");
        assertEquals("moon", CrewLook.emblem("moon"));
    }

    @Test
    void theChipIsTheEdgeToneAtAQuarter() {
        int tint = CrewLook.tagTint("navy");
        assertEquals(0x6F8EE0, tint & 0xFFFFFF);
        assertEquals(Math.round(0.24f * 255), Theme.alpha(tint));
    }

    @Test
    void everyEmblemHasAnOutlineAndEveryMarkIsBaked() throws Exception {
        Set<String> keys = new HashSet<>();
        for (String[] e : CrewLook.EMBLEMS) {
            assertTrue(keys.add(e[0]), "one of each: " + e[0]);
        }
        assertEquals(36, keys.size());
        Map<String, String> paths = Emblems.loadPaths();
        assertEquals(keys, paths.keySet());
        AtlasPacker.Atlas atlas = ShapeSet.bake(2, Icon.loadPaths(), paths);
        for (String emblem : keys) {
            assertTrue(atlas.regions().containsKey(Emblems.glyphKey(emblem)), emblem);
        }
        for (CrewLook.Shape s : CrewLook.SHAPES) {
            for (String part : new String[] {"fill", "edge", "light"}) {
                assertTrue(atlas.regions().containsKey("crewframe:" + s.key() + ":" + part), s.key() + " " + part);
            }
        }
        AtlasPacker.Region moon = atlas.regions().get(Emblems.glyphKey("moon"));
        assertEquals(Emblems.GLYPH * 2, moon.width(), "baked at the glyph's size, screen pixels");
    }
}
