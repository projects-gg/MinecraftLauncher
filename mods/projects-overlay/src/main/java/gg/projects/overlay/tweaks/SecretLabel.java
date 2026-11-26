package gg.projects.overlay.tweaks;

import java.text.Normalizer;
import java.util.List;
import java.util.Locale;
import java.util.regex.Pattern;

/**
 * PASSWORD-EYE's rules, free of the game (unit-tested): which text names a password, and which text boxes of a server
 * window are password boxes. A box is one when its label names a password ("Şifre:", "Parola", "Password"), or, for a
 * box whose label the window does not show, when the text right above it does ("Lütfen parolanızı yazınız:"); a label
 * shown above a box that names something else ("E-posta") keeps it clear, whatever the window says higher up.
 */
final class SecretLabel {
    /**
     * Words that name a password, folded ({@link #fold}): Turkish and English first, then the languages players meet
     * on servers most.
     */
    private static final List<String> WORDS = List.of(
        "sifre", "parola", "password", "passwd", "passcode", "passwort", "kennwort", "contrasena", "senha",
        "mot de passe", "wachtwoord", "haslo", "hasło", "heslo", "jelszo", "salasana", "losenord", "adgangskode",
        "passord", "пароль", "密码", "密碼", "パスワード", "비밀번호");
    /** The game's old colour and format codes, which a server's text may carry. */
    private static final Pattern FORMAT = Pattern.compile("\u00a7.");
    private static final Pattern MARKS = Pattern.compile("\\p{M}+");

    private SecretLabel() {
    }

    /** What a widget of a server window is, for the rules. */
    enum Kind {
        /** A text: the window's body, a box's label, its title. */
        TEXT,
        /** A one-line text box; its text is its label (the game gives a box its label even when it hides it). */
        BOX,
        /** Anything else: a check box, a slider, a box of several lines, a button. */
        OTHER
    }

    /** One widget of a server window, in the order the window lays them out. */
    record Node(Kind kind, String text) {
    }

    /** The text names a password. */
    static boolean names(String text) {
        if (text == null || text.isBlank()) {
            return false;
        }
        String folded = fold(text);
        for (String word : WORDS) {
            if (folded.contains(word)) {
                return true;
            }
        }
        return false;
    }

    /**
     * The text in lowercase without format codes or accents, Turkish letters folded the way players type them without
     * a Turkish keyboard ("Şİfre", "ŞİFRE", "sifre" all read "sifre").
     */
    static String fold(String text) {
        String plain = FORMAT.matcher(text).replaceAll("").replace('ı', 'i').replace('İ', 'I');
        String bare = MARKS.matcher(Normalizer.normalize(plain, Normalizer.Form.NFD)).replaceAll("");
        return bare.toLowerCase(Locale.ROOT);
    }

    /**
     * For each node, whether it is a password box: a box whose label names a password, or whose text right above it
     * (with nothing in between) does. Any other node answers false.
     */
    static boolean[] secret(List<Node> nodes) {
        boolean[] secret = new boolean[nodes.size()];
        String above = null;
        for (int i = 0; i < nodes.size(); i++) {
            Node node = nodes.get(i);
            switch (node.kind()) {
                case TEXT -> above = node.text();
                case BOX -> {
                    secret[i] = names(node.text()) || (above != null && !labelled(node, above) && names(above));
                    above = null;
                }
                case OTHER -> above = null;
            }
        }
        return secret;
    }

    /** The text right above a box is the box's own label (shown), not a text of the window. */
    private static boolean labelled(Node box, String above) {
        return box.text() != null && !box.text().isBlank() && fold(box.text()).equals(fold(above));
    }
}
