package gg.projects.overlay.lang;

import java.util.Locale;

/**
 * The two languages the overlay speaks (OVL-LANG). {@link #code} is the bridge's ({@code tr}, {@code en}: the
 * launcher's language codes), {@link #gameCode} the game's code of the same language, and {@link #ownName} how the
 * settings' "Dil" row names it: in that language itself, so a player finds theirs whatever the panel speaks now.
 */
public enum Language {
    TURKISH("tr", "tr_tr", "Türkçe", Locale.forLanguageTag("tr")),
    ENGLISH("en", "en_us", "English", Locale.ROOT);

    public final String code;
    public final String gameCode;
    public final String ownName;
    /** Capitals and percentages follow it (a name's İ, "%70" or "70%"). */
    public final Locale locale;

    Language(String code, String gameCode, String ownName, Locale locale) {
        this.code = code;
        this.gameCode = gameCode;
        this.ownName = ownName;
        this.locale = locale;
    }

    /** The language the bridge names ({@code tr}, {@code en}), or null for anything else. */
    public static Language ofCode(String code) {
        if (code == null) {
            return null;
        }
        for (Language language : values()) {
            if (language.code.equals(code.strip())) {
                return language;
            }
        }
        return null;
    }

    /**
     * The overlay's language for a game language code: Turkish for {@code tr_tr} (any {@code tr…}), English for every
     * other one (the overlay ships these two; the game used English for its words in any other language too).
     */
    public static Language ofGame(String languageCode) {
        return languageCode != null && languageCode.strip().toLowerCase(Locale.ROOT).startsWith("tr") ? TURKISH
            : ENGLISH;
    }
}
