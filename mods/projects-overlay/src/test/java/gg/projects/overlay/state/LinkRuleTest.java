package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.net.URI;
import org.junit.jupiter.api.Test;

/** The mod's own last check before it opens a link the launcher let the player press. */
class LinkRuleTest {
    @Test
    void onlyPlainHttpsAddressesWithAHostName() {
        assertTrue(LinkRule.safe("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assertTrue(LinkRule.safe("HTTPS://Modrinth.com/mod/sodium"));
        assertTrue(LinkRule.safe("https://tr.minecraft.wiki/w/Ağaç"));
        assertTrue(LinkRule.safe("https://projects.gg/"));
        assertFalse(LinkRule.safe("http://youtube.com/x"), "http");
        assertFalse(LinkRule.safe("javascript:alert(1)"));
        assertFalse(LinkRule.safe("file:///C:/Windows/system32/calc.exe"));
        assertFalse(LinkRule.safe("https://youtube.com@kotu.example/x"), "a user name");
        assertFalse(LinkRule.safe("https://kotu.example:8080/x"), "a port");
        assertFalse(LinkRule.safe("https://192.168.1.1/x"), "an IP address");
        assertFalse(LinkRule.safe("https://[::1]/x"));
        assertFalse(LinkRule.safe("https://localhost/x"), "no dot");
        assertFalse(LinkRule.safe("https://уoutube.com/x"), "a look-alike letter");
        assertFalse(LinkRule.safe("https://youtube.com/a b"), "a space");
        assertFalse(LinkRule.safe("https://youtube.com/\u202Egnp.exe"), "a direction override");
        assertFalse(LinkRule.safe("https://a.com/" + "x".repeat(LinkRule.MAX_LENGTH)), "too long");
        assertFalse(LinkRule.safe(null));
    }

    @Test
    void theDialogNamesWhoseSiteItIs() {
        assertEquals("youtube.com", LinkRule.domain(LinkRule.host("https://www.youtube.com/watch?v=1")));
        assertEquals("twitch.tv", LinkRule.domain(LinkRule.host("https://clips.twitch.tv/x")));
        assertEquals("example.com.tr", LinkRule.domain(LinkRule.host("https://forum.example.com.tr/a")));
        assertEquals("projects.gg", LinkRule.domain(LinkRule.host("https://Projects.gg./")));
        assertNull(LinkRule.host("https://user@site.com/"));
    }

    @Test
    void theAddressOpensInAscii() {
        URI wiki = LinkRule.uri("https://tr.minecraft.wiki/w/Ağaç");
        assertNotNull(wiki);
        assertEquals("https://tr.minecraft.wiki/w/A%C4%9Fa%C3%A7", wiki.toString());
        URI brackets = LinkRule.uri("https://example.com/a[1]?q=x");
        assertNotNull(brackets);
        assertEquals("example.com", brackets.getHost());
        assertTrue(brackets.toString().startsWith("https://example.com/a%5B1%5D"));
        assertNull(LinkRule.uri("https://user@example.com/"));
    }
}
