package gg.projects.overlay.host;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HexFormat;

/** The shared test vectors of world sharing ({@code crates/pl-host/testdata}, HOST1-C), read as the build points. */
final class Vectors {
    private Vectors() {
    }

    static Path dir() {
        String configured = System.getProperty("projects.testdata");
        return Path.of(configured != null ? configured : "../../crates/pl-host/testdata");
    }

    static JsonObject read(String relative) {
        try {
            return JsonParser.parseString(Files.readString(dir().resolve(relative), StandardCharsets.UTF_8))
                .getAsJsonObject();
        } catch (IOException e) {
            throw new IllegalStateException("missing test vector " + relative, e);
        }
    }

    static String text(String relative) {
        try {
            return Files.readString(dir().resolve(relative), StandardCharsets.UTF_8);
        } catch (IOException e) {
            throw new IllegalStateException("missing test vector " + relative, e);
        }
    }

    static byte[] hex(String hex) {
        return HexFormat.of().parseHex(hex);
    }
}
