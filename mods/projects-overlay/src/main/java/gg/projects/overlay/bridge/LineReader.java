package gg.projects.overlay.bridge;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/** Reads newline-terminated UTF-8 frames with a hard size limit, blocking without timers between frames. */
final class LineReader {
    private final InputStream in;
    private final int maxBytes;
    private final byte[] buffer = new byte[8192];
    private final ByteArrayOutputStream line = new ByteArrayOutputStream(512);
    private int pos;
    private int len;

    LineReader(InputStream in, int maxBytes) {
        this.in = in;
        this.maxBytes = maxBytes;
    }

    /**
     * Returns the next frame without its newline, or null at the end of the stream (a partial last frame is
     * dropped). Throws when a frame is longer than the limit, which ends the connection.
     */
    String readLine() throws IOException {
        line.reset();
        while (true) {
            if (pos == len) {
                len = in.read(buffer);
                pos = 0;
                if (len <= 0) {
                    len = 0;
                    return null;
                }
            }
            int start = pos;
            while (pos < len && buffer[pos] != '\n') {
                pos++;
            }
            int chunk = pos - start;
            if (line.size() + chunk > maxBytes) {
                throw new IOException("frame longer than " + maxBytes + " bytes");
            }
            line.write(buffer, start, chunk);
            if (pos < len) {
                pos++;
                int size = line.size();
                byte[] bytes = line.toByteArray();
                if (size > 0 && bytes[size - 1] == '\r') {
                    size--;
                }
                return new String(bytes, 0, size, StandardCharsets.UTF_8);
            }
        }
    }
}
