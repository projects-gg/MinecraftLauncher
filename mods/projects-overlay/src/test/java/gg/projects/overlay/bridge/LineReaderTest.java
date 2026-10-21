package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

class LineReaderTest {
    private static LineReader reader(String s, int max) {
        return new LineReader(new ByteArrayInputStream(s.getBytes(StandardCharsets.UTF_8)), max);
    }

    @Test
    void splitsFramesAndHandlesCrLf() throws IOException {
        LineReader r = reader("{\"t\":\"a\"}\r\n{\"t\":\"ğ\"}\n", 100);
        assertEquals("{\"t\":\"a\"}", r.readLine());
        assertEquals("{\"t\":\"ğ\"}", r.readLine());
        assertNull(r.readLine());
    }

    @Test
    void dropsAPartialLastFrame() throws IOException {
        LineReader r = reader("{\"t\":\"a\"}\n{\"t\":", 100);
        assertEquals("{\"t\":\"a\"}", r.readLine());
        assertNull(r.readLine());
    }

    @Test
    void rejectsFramesOverTheLimit() {
        LineReader r = reader("x".repeat(65) + "\n", 64);
        assertThrows(IOException.class, r::readLine);
    }

    @Test
    void acceptsAFrameExactlyAtTheLimit() throws IOException {
        LineReader r = reader("y".repeat(64) + "\n", 64);
        assertEquals(64, r.readLine().length());
    }
}
