package gg.projects.overlay.host;

/**
 * Game names as world sharing reads them (PROTOCOL.md §6.8, §9): 3 to 16 of {@code [A-Za-z0-9_]}, compared ASCII
 * case-insensitively. Never through the JVM's locale: in Turkish {@code "I".toLowerCase()} is a dotless ı, and a name
 * would stop matching itself. Pure, so the rules have plain tests.
 */
public final class Names {
    public static final int MIN = 3;
    public static final int MAX = 16;

    private Names() {
    }

    /** A name a guest may play under in a shared world. */
    public static boolean valid(String name) {
        if (name == null || name.length() < MIN || name.length() > MAX) {
            return false;
        }
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            boolean ok = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
            if (!ok) {
                return false;
            }
        }
        return true;
    }

    /** The same name, ASCII letters folded; null never equals anything. */
    public static boolean same(String a, String b) {
        if (a == null || b == null || a.length() != b.length()) {
            return false;
        }
        for (int i = 0; i < a.length(); i++) {
            if (fold(a.charAt(i)) != fold(b.charAt(i))) {
                return false;
            }
        }
        return true;
    }

    /** The name as a map key: ASCII letters lowered, everything else as it is. */
    public static String key(String name) {
        StringBuilder b = new StringBuilder(name.length());
        for (int i = 0; i < name.length(); i++) {
            b.append(fold(name.charAt(i)));
        }
        return b.toString();
    }

    private static char fold(char c) {
        return c >= 'A' && c <= 'Z' ? (char) (c + ('a' - 'A')) : c;
    }
}
