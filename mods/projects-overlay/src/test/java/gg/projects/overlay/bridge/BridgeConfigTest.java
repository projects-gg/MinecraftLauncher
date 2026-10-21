package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class BridgeConfigTest {
    private static final String TOKEN = "A".repeat(43);

    @Test
    void acceptsTheLaunchersValues() {
        BridgeConfig c = BridgeConfig.parse("54321", TOKEN);
        assertNotNull(c);
        assertEquals(54321, c.port());
    }

    @Test
    void rejectsMissingOrOddValues() {
        assertNull(BridgeConfig.parse(null, TOKEN));
        assertNull(BridgeConfig.parse("54321", null));
        assertNull(BridgeConfig.parse("80", TOKEN));
        assertNull(BridgeConfig.parse("70000", TOKEN));
        assertNull(BridgeConfig.parse("port", TOKEN));
        assertNull(BridgeConfig.parse("54321", "short"));
        assertNull(BridgeConfig.parse("54321", TOKEN + " "));
    }

    @Test
    void neverPrintsTheToken() {
        String text = BridgeConfig.parse("54321", TOKEN).toString();
        assertFalse(text.contains(TOKEN));
        assertTrue(text.contains("redacted"));
    }

    @Test
    void removesTheTokenFromSystemProperties() {
        System.setProperty(BridgeConfig.PORT_PROPERTY, "54321");
        System.setProperty(BridgeConfig.TOKEN_PROPERTY, TOKEN);
        try {
            assertNotNull(BridgeConfig.fromSystemProperties());
            assertNull(System.getProperty(BridgeConfig.TOKEN_PROPERTY));
        } finally {
            System.clearProperty(BridgeConfig.PORT_PROPERTY);
            System.clearProperty(BridgeConfig.TOKEN_PROPERTY);
        }
    }
}
