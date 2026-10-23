package gg.projects.overlay.state;

import java.net.URI;
import java.net.URISyntaxException;
import java.util.Locale;

/**
 * The mod's own last check before it opens a link the launcher marked as one the player may press (CHAT2 §9;
 * the launcher decides with the shared rule table): only a plain {@code https} address with a host name, no
 * user name, no port, no spaces or invisible characters. Anything else is never opened, whatever the launcher
 * said. Pure, so it is tested on its own.
 */
public final class LinkRule {
    /** The longest link the rules let through (Unicode code points, as written). */
    public static final int MAX_LENGTH = 200;

    private LinkRule() {
    }

    /** Whether {@code url} may be opened at all. */
    public static boolean safe(String url) {
        if (url == null || url.length() < 12 || url.codePointCount(0, url.length()) > MAX_LENGTH
            || !url.regionMatches(true, 0, "https://", 0, 8)) {
            return false;
        }
        for (int i = 0; i < url.length(); ) {
            int cp = url.codePointAt(i);
            i += Character.charCount(cp);
            int kind = Character.getType(cp);
            if (Character.isWhitespace(cp) || kind == Character.CONTROL || kind == Character.FORMAT
                || kind == Character.PRIVATE_USE || kind == Character.SURROGATE || kind == Character.UNASSIGNED
                || cp == '\\' || cp == '"' || cp == '<' || cp == '>') {
                return false;
            }
        }
        String host = host(url);
        if (host == null || host.isEmpty() || host.startsWith(".") || host.endsWith("-") || !host.contains(".")) {
            return false;
        }
        for (int i = 0; i < host.length(); i++) {
            char c = host.charAt(i);
            boolean ok = (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '.' || c == '-';
            if (!ok) {
                return false;
            }
        }
        if (host.chars().allMatch(c -> c == '.' || (c >= '0' && c <= '9'))) {
            // An IP address.
            return false;
        }
        return uri(url) != null;
    }

    /**
     * What lies between {@code //} and the first {@code /}, {@code ?} or {@code #}, in lower case without a
     * trailing dot; null when it carries a user name or a port.
     */
    public static String host(String url) {
        int start = url.indexOf("://");
        if (start < 0) {
            return null;
        }
        start += 3;
        int end = url.length();
        for (int i = start; i < url.length(); i++) {
            char c = url.charAt(i);
            if (c == '/' || c == '?' || c == '#') {
                end = i;
                break;
            }
        }
        String authority = url.substring(start, end);
        if (authority.contains("@") || authority.contains(":") || authority.contains("[")) {
            return null;
        }
        String host = authority.toLowerCase(Locale.ROOT);
        return host.endsWith(".") ? host.substring(0, host.length() - 1) : host;
    }

    /**
     * The registrable part of a host, as the dialog names the site: "www.youtube.com" reads "youtube.com",
     * "forum.example.com.tr" reads "example.com.tr" (whose site it is, not the subdomain in front).
     */
    public static String domain(String host) {
        if (host == null) {
            return "";
        }
        String[] labels = host.split("\\.");
        int n = labels.length;
        if (n <= 2) {
            return host;
        }
        String last = labels[n - 1];
        String second = labels[n - 2];
        boolean countrySecond = last.length() == 2 && second.length() <= 3 && n >= 3;
        int keep = countrySecond ? 3 : 2;
        StringBuilder b = new StringBuilder();
        for (int i = n - keep; i < n; i++) {
            if (b.length() > 0) {
                b.append('.');
            }
            b.append(labels[i]);
        }
        return b.toString();
    }

    /**
     * The link as a URI the platform can open, every part in ASCII (letters of other scripts and the characters a
     * URI may not carry as they are, such as {@code [ ]}, percent-encoded), or null.
     */
    public static URI uri(String url) {
        URI parsed;
        try {
            parsed = new URI(url);
        } catch (URISyntaxException e) {
            parsed = quoted(url);
        }
        if (parsed == null || !"https".equalsIgnoreCase(parsed.getScheme()) || parsed.getHost() == null
            || parsed.getUserInfo() != null || parsed.getPort() != -1) {
            return null;
        }
        try {
            return new URI(parsed.toASCIIString());
        } catch (URISyntaxException e) {
            return null;
        }
    }

    /** {@code url} split by hand, so that URI quotes what it must. */
    private static URI quoted(String url) {
        if (!url.regionMatches(true, 0, "https://", 0, 8)) {
            return null;
        }
        String rest = url.substring(8);
        int cut = rest.length();
        for (int i = 0; i < rest.length(); i++) {
            char c = rest.charAt(i);
            if (c == '/' || c == '?' || c == '#') {
                cut = i;
                break;
            }
        }
        String authority = rest.substring(0, cut);
        String path = rest.substring(cut);
        String fragment = null;
        int hash = path.indexOf('#');
        if (hash >= 0) {
            fragment = path.substring(hash + 1);
            path = path.substring(0, hash);
        }
        String query = null;
        int question = path.indexOf('?');
        if (question >= 0) {
            query = path.substring(question + 1);
            path = path.substring(0, question);
        }
        try {
            return new URI("https", authority, path.isEmpty() ? null : path, query, fragment);
        } catch (URISyntaxException | RuntimeException e) {
            return null;
        }
    }
}
