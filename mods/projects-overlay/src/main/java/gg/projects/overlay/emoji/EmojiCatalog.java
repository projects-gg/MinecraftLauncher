package gg.projects.overlay.emoji;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.text.Normalizer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.EnumMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.IntPredicate;

/**
 * The panel's emoji (OVL-SOC; the owner: "çıkartmalar var da emoji yok"): the launcher's own picker data
 * ({@code apps/launcher/src/features/emoji/data/emoji.<lang>.tsv}, Unicode Emoji 17.0 and CLDR 48.2.0, copied into the
 * jar by the build), in its order and its nine groups, with the names and folded words a search finds them by. Only the
 * emoji the overlay draws as a picture: one code point (the style selector after it does not count) the faces have a
 * picture of ({@code drawable}, {@link gg.projects.overlay.ui.EmojiGlyphs}); sequences (skin tones, families, most
 * flags) would draw as their parts, so the panel offers none. Read once per language on the emoji tab's first open,
 * never at start, and kept for the game (about 1400 short entries; reading it again on every open would hitch the frame).
 * Pure, so it is tested on its own.
 */
public final class EmojiCatalog {
    /** The launcher's groups, in its order ({@code data.ts} {@code EMOJI_GROUPS}). */
    public enum Group {
        SMILEYS("smileys"),
        PEOPLE("people"),
        NATURE("nature"),
        FOOD("food"),
        TRAVEL("travel"),
        ACTIVITIES("activities"),
        OBJECTS("objects"),
        SYMBOLS("symbols"),
        FLAGS("flags");

        /** Its name in the lang files ({@code chat.emoji.group.<id>}). */
        public final String id;

        Group(String id) {
            this.id = id;
        }
    }

    /**
     * One emoji: as written into a message (its single code point, no style selector: the game draws that), its group,
     * CLDR's name in the data's language ("sevinç gözyaşları"), its folded words between spaces (" kahkaha komik "),
     * and for Turkish data the English name and the English words the Turkish ones do not find.
     */
    public record Entry(String emoji, Group group, String name, String words, String english, String fallback,
        int index) {
    }

    static final String MAGIC = "PL-EMOJI";
    private static final int VS16 = 0xFE0F;
    private static final Locale TURKISH = Locale.forLanguageTag("tr");
    private static final Map<String, EmojiCatalog> LOADED = new java.util.HashMap<>();

    private final List<Entry> entries;
    private final Map<Group, Integer> firsts;

    private EmojiCatalog(List<Entry> entries) {
        this.entries = Collections.unmodifiableList(entries);
        Map<Group, Integer> at = new EnumMap<>(Group.class);
        for (int i = 0; i < entries.size(); i++) {
            at.putIfAbsent(entries.get(i).group(), i);
        }
        this.firsts = at;
    }

    public List<Entry> entries() {
        return entries;
    }

    /** The groups that have any emoji here, in order. */
    public List<Group> groups() {
        List<Group> out = new ArrayList<>(firsts.size());
        for (Group g : Group.values()) {
            if (firsts.containsKey(g)) {
                out.add(g);
            }
        }
        return out;
    }

    /** The entry that writes {@code emoji} (its bare form: without style selectors and skin tones), or null. */
    public Entry find(String emoji) {
        String bare = bare(emoji);
        for (Entry e : entries) {
            if (e.emoji().equals(bare)) {
                return e;
            }
        }
        return null;
    }

    /**
     * Reads one data file ({@code text}), keeping the rows whose emoji is one code point {@code drawable} has a picture
     * of. A file of another format is no emoji at all; a broken row is left out.
     */
    public static EmojiCatalog parse(String text, IntPredicate drawable) {
        List<Entry> out = new ArrayList<>();
        String[] lines = text.split("\r?\n");
        if (lines.length == 0 || !lines[0].startsWith(MAGIC + "\t1\t")) {
            return new EmojiCatalog(out);
        }
        Group[] groups = Group.values();
        for (int i = 1; i < lines.length; i++) {
            String line = lines[i];
            if (line.isEmpty()) {
                continue;
            }
            String[] cols = line.split("\t", -1);
            if (cols.length < 4) {
                continue;
            }
            String emoji = bare(cols[0]);
            if (emoji.isEmpty() || emoji.codePointCount(0, emoji.length()) != 1
                || !drawable.test(emoji.codePointAt(0))) {
                continue;
            }
            int group;
            try {
                group = Integer.parseInt(cols[1]);
            } catch (NumberFormatException e) {
                continue;
            }
            String name = cols[3];
            if (group < 0 || group >= groups.length || name.isEmpty()) {
                continue;
            }
            String words = cols.length > 4 && !cols[4].isEmpty() ? " " + cols[4] + " " : "";
            String english = cols.length > 5 ? cols[5] : "";
            String fallback = cols.length > 6 && !cols[6].isEmpty() ? " " + cols[6] + " " : "";
            out.add(new Entry(emoji, groups[group], name, words, english, fallback, out.size()));
        }
        return new EmojiCatalog(out);
    }

    /**
     * The jar's data in a language ({@code tr} or {@code en}), read on first use and kept (an empty catalog when the
     * build left it out).
     */
    public static synchronized EmojiCatalog load(String lang, IntPredicate drawable) {
        String code = "en".equals(lang) ? "en" : "tr";
        EmojiCatalog made = LOADED.get(code);
        if (made == null) {
            String text = "";
            try (InputStream in = EmojiCatalog.class.getResourceAsStream(
                "/assets/projects_overlay/emoji/emoji." + code + ".tsv")) {
                if (in != null) {
                    text = new String(in.readAllBytes(), StandardCharsets.UTF_8);
                }
            } catch (IOException e) {
                text = "";
            }
            made = parse(text, drawable);
            LOADED.put(code, made);
        }
        return made;
    }

    /**
     * Whether the jar carries the data in a language ({@code tr} or {@code en}), without reading it: the picker shows its
     * emoji tab before the first open reads anything.
     */
    public static boolean available(String lang) {
        String code = "en".equals(lang) ? "en" : "tr";
        return EmojiCatalog.class.getResource("/assets/projects_overlay/emoji/emoji." + code + ".tsv") != null;
    }

    /** An emoji without what the overlay's pictures do not need: style selectors (VS16) and skin tones. */
    public static String bare(String emoji) {
        if (emoji == null || emoji.isEmpty()) {
            return "";
        }
        StringBuilder b = new StringBuilder(emoji.length());
        emoji.codePoints().filter(cp -> cp != VS16 && cp != 0xFE0E && (cp < 0x1F3FB || cp > 0x1F3FF))
            .forEach(b::appendCodePoint);
        return b.toString();
    }

    /**
     * Search folding as the launcher's ({@code @pl/ui} {@code fold}): lower case in Turkish, "I", "İ" and "ı" all "i",
     * marks gone ("ş" "s", "ğ" "g", "ö" "o").
     */
    public static String fold(String text) {
        String lower = text.toLowerCase(TURKISH);
        String stripped = Normalizer.normalize(lower, Normalizer.Form.NFD).replaceAll("\\p{M}", "");
        return stripped.replace('ı', 'i');
    }

    /** The name as the picker shows it: its first letter in upper case ("Sevinç gözyaşları"). */
    public static String displayName(String name, boolean turkish) {
        if (name == null || name.isEmpty()) {
            return "";
        }
        int first = name.codePointAt(0);
        String head = new String(Character.toChars(first));
        return head.toUpperCase(turkish ? TURKISH : Locale.ENGLISH) + name.substring(head.length());
    }
}
