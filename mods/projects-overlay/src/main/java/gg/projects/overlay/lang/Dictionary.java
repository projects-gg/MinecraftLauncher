package gg.projects.overlay.lang;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Function;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The overlay's words in both its languages, read once from its own lang files (the jar's
 * {@code assets/projects_overlay/lang/tr_tr.json} and {@code en_us.json}), not through the game's language: the
 * overlay speaks the launcher's language, which may be another than the game's (OVL-LANG). A word missing in one
 * language comes from the other; one missing in both reads as its key, as the game shows a missing translation.
 *
 * <p>Templates take {@code %s} (the next argument), {@code %2$s} (a numbered one) and {@code %%} (a percent sign), the
 * way the game's translations do; an argument that is not there reads as nothing, never as a format code.
 */
public final class Dictionary {
    private static final String FOLDER = "/assets/projects_overlay/lang/";
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    private static volatile Dictionary shared;

    private final Map<String, String> turkish;
    private final Map<String, String> english;
    private final Map<String, List<Part>> parsed = new ConcurrentHashMap<>();

    /** A piece of a template: literal text, or (text null) the argument at {@code index}. */
    public record Part(String text, int index) {
        public boolean argument() {
            return text == null;
        }
    }

    private Dictionary(Map<String, String> turkish, Map<String, String> english) {
        this.turkish = Map.copyOf(turkish);
        this.english = Map.copyOf(english);
    }

    /** The jar's words, read on first use. */
    public static Dictionary shared() {
        Dictionary d = shared;
        if (d == null) {
            synchronized (Dictionary.class) {
                d = shared;
                if (d == null) {
                    d = new Dictionary(read(Language.TURKISH), read(Language.ENGLISH));
                    shared = d;
                }
            }
        }
        return d;
    }

    /** A dictionary of these words (tests). */
    public static Dictionary of(Map<String, String> turkish, Map<String, String> english) {
        return new Dictionary(turkish, english);
    }

    private static Map<String, String> read(Language language) {
        String file = FOLDER + language.gameCode + ".json";
        try (InputStream in = Dictionary.class.getResourceAsStream(file)) {
            if (in == null) {
                LOG.warn("The overlay's {} words are missing", language.code);
                return Map.of();
            }
            try (Reader reader = new InputStreamReader(in, StandardCharsets.UTF_8)) {
                return parse(JsonParser.parseReader(reader));
            }
        } catch (IOException | JsonParseException | IllegalStateException e) {
            LOG.warn("The overlay's {} words could not be read: {}", language.code, e.toString());
            return Map.of();
        }
    }

    /** A lang file's object: its string entries (anything else is left out). */
    static Map<String, String> parse(JsonElement root) {
        Map<String, String> out = new HashMap<>();
        if (root != null && root.isJsonObject()) {
            JsonObject o = root.getAsJsonObject();
            for (Map.Entry<String, JsonElement> e : o.entrySet()) {
                if (e.getValue().isJsonPrimitive() && e.getValue().getAsJsonPrimitive().isString()) {
                    out.put(e.getKey(), e.getValue().getAsString());
                }
            }
        }
        return out;
    }

    /** The template of {@code key} in {@code language}, else in the other language, else the key itself. */
    public String template(Language language, String key) {
        Map<String, String> first = language == Language.TURKISH ? turkish : english;
        String found = first.get(key);
        if (found == null) {
            found = (first == turkish ? english : turkish).get(key);
        }
        return found == null ? key : found;
    }

    /** Whether {@code language} has its own word for {@code key} (tests: both files name the same keys). */
    public boolean has(Language language, String key) {
        return (language == Language.TURKISH ? turkish : english).containsKey(key);
    }

    /** Every key of {@code language} (tests). */
    public Set<String> keys(Language language) {
        return (language == Language.TURKISH ? turkish : english).keySet();
    }

    /** The template's pieces, taken apart once. */
    public List<Part> parts(String template) {
        return parsed.computeIfAbsent(template, Dictionary::split);
    }

    /** The template of {@code key} in {@code language} with its arguments filled in, as plain text. */
    public String format(Language language, String key, Object... args) {
        return fill(parts(template(language, key)), args, String::valueOf);
    }

    /** The pieces as text: each argument through {@code asText}, a missing one as nothing. */
    public static String fill(List<Part> parts, Object[] args, Function<Object, String> asText) {
        StringBuilder b = new StringBuilder();
        for (Part part : parts) {
            if (!part.argument()) {
                b.append(part.text());
            } else if (args != null && part.index() < args.length && args[part.index()] != null) {
                b.append(asText.apply(args[part.index()]));
            }
        }
        return b.toString();
    }

    /** Literal runs and argument slots: {@code %s} takes the next argument, {@code %n$s} the n-th, {@code %%} is "%". */
    static List<Part> split(String template) {
        List<Part> out = new ArrayList<>();
        StringBuilder text = new StringBuilder();
        int next = 0;
        int i = 0;
        while (i < template.length()) {
            char c = template.charAt(i);
            if (c != '%' || i + 1 >= template.length()) {
                text.append(c);
                i++;
                continue;
            }
            char d = template.charAt(i + 1);
            if (d == '%') {
                text.append('%');
                i += 2;
                continue;
            }
            int index = -1;
            int end = -1;
            if (d == 's') {
                index = next++;
                end = i + 2;
            } else if (Character.isDigit(d)) {
                int j = i + 1;
                while (j < template.length() && Character.isDigit(template.charAt(j))) {
                    j++;
                }
                if (j + 1 < template.length() && template.charAt(j) == '$' && template.charAt(j + 1) == 's') {
                    index = Integer.parseInt(template.substring(i + 1, j)) - 1;
                    end = j + 2;
                }
            }
            if (index < 0) {
                // Not a slot the game knows: the sign stays as written.
                text.append(c);
                i++;
                continue;
            }
            if (!text.isEmpty()) {
                out.add(new Part(text.toString(), -1));
                text.setLength(0);
            }
            out.add(new Part(null, index));
            i = end;
        }
        if (!text.isEmpty() || out.isEmpty()) {
            out.add(new Part(text.toString(), -1));
        }
        return List.copyOf(out);
    }
}
