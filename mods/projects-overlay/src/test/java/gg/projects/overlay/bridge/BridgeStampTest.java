package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import org.junit.jupiter.api.Test;

/**
 * The jar says which bridge protocol it speaks (OVL-AUTO): the launcher takes a jar from the signed content only for
 * its own protocol, and Studio reads the number from the jar's fabric.mod.json, never from CI's report. The stamp is
 * written in fabric.mod.json by hand, so it must stay the protocol the code speaks.
 */
class BridgeStampTest {
    @Test
    void theJarStampsTheProtocolItSpeaks() throws IOException {
        JsonObject mod = null;
        // Fabric API's modules on the test classpath carry a fabric.mod.json of their own.
        for (URL url : Collections.list(BridgeStampTest.class.getClassLoader().getResources("fabric.mod.json"))) {
            try (InputStream in = url.openStream()) {
                JsonObject json = JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8))
                    .getAsJsonObject();
                if ("projects_overlay".equals(json.get("id").getAsString())) {
                    mod = json;
                }
            }
        }
        assertNotNull(mod, "the mod's own fabric.mod.json");
        int proto = mod.getAsJsonObject("custom").getAsJsonObject("projects:bridge").get("proto").getAsInt();
        assertEquals(Protocol.VERSION, proto);
    }
}
