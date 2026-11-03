package gg.projects.overlay.emoji;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;

/**
 * The panel's emoji search (OVL-SOC), the launcher's ({@code features/emoji/search.ts}) in Java: in the overlay's
 * language, and a Turkish search finds English too ("fire", "lol"). Folded as every search ({@link EmojiCatalog#fold}):
 * "gulmek" finds "gülmek". Each word of the query must start a word of the emoji; a query of three letters or more also
 * matches inside a name or a keyword. Ranked: the name itself, the name's words, a name that starts with the query, its
 * words, its keywords, then the same in English, then inside a word; within the bands of whole words, keywords and
 * English the emoji chats use most come first, Unicode's order after. A pasted emoji finds itself. Pure.
 */
public final class EmojiFinder {
    /**
     * The emoji chats use most (the launcher's {@code POPULAR}, Unicode's frequency list and a game server's own), most
     * used first; the first twenty come first among the emoji a query finds the same way.
     */
    private static final String POPULAR = "😂❤🤣👍😭🙏😘🥰😍😊🎉😁💕🥺😅🔥☺🤦♥🤷";
    private static final int BOOSTED = 20;
    private static final Map<String, Integer> POPULARITY = new HashMap<>();
    private static final WeakHashMap<EmojiCatalog, Folded[]> CACHE = new WeakHashMap<>();

    static {
        int i = 0;
        for (int at = 0; at < POPULAR.length() && i < BOOSTED; ) {
            int cp = POPULAR.codePointAt(at);
            POPULARITY.putIfAbsent(new String(Character.toChars(cp)), i++);
            at += Character.charCount(cp);
        }
    }

    private record FoldedName(String whole, String words) {
    }

    private record Folded(FoldedName name, FoldedName english) {
    }

    private record Found(EmojiCatalog.Entry entry, int band, int popular, int tier) {
    }

    private EmojiFinder() {
    }

    /** The entries a query finds, best first; nothing for an empty query. */
    public static List<EmojiCatalog.Entry> search(EmojiCatalog catalog, String text) {
        String trimmed = text == null ? "" : text.strip();
        if (trimmed.isEmpty()) {
            return List.of();
        }
        int first = trimmed.codePointAt(0);
        if (Character.isExtendedPictographic(first) || (first >= 0x1F1E6 && first <= 0x1F1FF)) {
            // A pasted emoji finds itself.
            EmojiCatalog.Entry e = catalog.find(new String(Character.toChars(first)));
            return e == null ? List.of() : List.of(e);
        }
        List<String> tokens = split(EmojiCatalog.fold(trimmed));
        if (tokens.isEmpty()) {
            return List.of();
        }
        String query = String.join(" ", tokens);
        Folded[] names = folded(catalog);
        List<Found> found = new ArrayList<>();
        for (EmojiCatalog.Entry entry : catalog.entries()) {
            int[] match = rank(names[entry.index()], entry, query, tokens);
            if (match == null) {
                continue;
            }
            int band = match[1];
            int popular = band == 1 ? BOOSTED : POPULARITY.getOrDefault(entry.emoji(), BOOSTED);
            found.add(new Found(entry, band, popular, match[0]));
        }
        found.sort(Comparator.comparingInt(Found::band).thenComparingInt(Found::popular).thenComparingInt(Found::tier)
            .thenComparingInt(f -> f.entry().index()));
        List<EmojiCatalog.Entry> out = new ArrayList<>(found.size());
        for (Found f : found) {
            out.add(f.entry());
        }
        return out;
    }

    /** The search's words: letters, digits, "+", "#" and "*" between anything else. */
    static List<String> split(String text) {
        List<String> out = new ArrayList<>();
        for (String part : text.split("[^\\p{L}\\p{N}+#*]+")) {
            if (!part.isEmpty()) {
                out.add(part);
            }
        }
        return out;
    }

    private static Folded[] folded(EmojiCatalog catalog) {
        synchronized (CACHE) {
            Folded[] names = CACHE.get(catalog);
            if (names == null) {
                List<EmojiCatalog.Entry> entries = catalog.entries();
                names = new Folded[entries.size()];
                for (EmojiCatalog.Entry e : entries) {
                    names[e.index()] = new Folded(foldName(e.name()), foldName(e.english()));
                }
                CACHE.put(catalog, names);
            }
            return names;
        }
    }

    private static FoldedName foldName(String name) {
        String whole = EmojiCatalog.fold(name);
        return new FoldedName(whole, " " + String.join(" ", split(whole)) + " ");
    }

    private static boolean startsAWord(String words, String token) {
        return words.contains(" " + token);
    }

    private static boolean hasWord(String words, String token) {
        return words.contains(" " + token + " ");
    }

    /** How an entry matches (its tier and band, as the launcher's {@code rank}), or null. */
    private static int[] rank(Folded folded, EmojiCatalog.Entry entry, String query, List<String> tokens) {
        FoldedName name = folded.name();
        FoldedName english = folded.english();
        if (name.whole().equals(query)) {
            return new int[] {0, 0};
        }
        if (tokens.stream().allMatch(t -> hasWord(name.words(), t))) {
            return new int[] {1, 0};
        }
        if (name.whole().startsWith(query)) {
            return new int[] {2, 1};
        }
        if (tokens.stream().allMatch(t -> startsAWord(name.words(), t))) {
            return new int[] {3, 1};
        }
        if (tokens.stream().allMatch(t -> startsAWord(name.words(), t) || startsAWord(entry.words(), t))) {
            return new int[] {4, 2};
        }
        if (!english.whole().isEmpty()) {
            if (english.whole().equals(query)) {
                return new int[] {5, 3};
            }
            if (tokens.stream().allMatch(t -> hasWord(english.words(), t))) {
                return new int[] {6, 3};
            }
            if (english.whole().startsWith(query)) {
                return new int[] {7, 3};
            }
            if (tokens.stream().allMatch(t -> startsAWord(english.words(), t))) {
                return new int[] {8, 3};
            }
            if (tokens.stream().allMatch(t -> startsAWord(name.words(), t) || startsAWord(entry.words(), t)
                || startsAWord(english.words(), t) || startsAWord(entry.fallback(), t))) {
                return new int[] {9, 3};
            }
        }
        if (query.length() >= 3 && (name.whole().contains(query) || entry.words().contains(query))) {
            return new int[] {10, 4};
        }
        return null;
    }
}
