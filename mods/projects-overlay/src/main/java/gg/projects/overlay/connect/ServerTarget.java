package gg.projects.overlay.connect;

import java.util.Locale;

/**
 * A server the game can go to: a host and a port, checked before anything happens in the game. Projects is
 * one network under several names (play.projects.gg, mc.projects.gg, ...), so all of them are the same place.
 * A friend's world (HOST1, H12) is the launcher's own way there on 127.0.0.1 with a {@code label} the player
 * reads instead ("Arkadaş dünyası: Ayşe"); the address is never shown then. Pure: no Minecraft classes, so the
 * rules have plain tests.
 */
public record ServerTarget(String host, int port, String label) {
    /** Where "Katıl" and "Bağlan" go for Projects. */
    public static final String PROJECTS_HOST = "play.projects.gg";
    public static final int DEFAULT_PORT = 25565;
    private static final int MAX_HOST_CHARS = 253;

    public ServerTarget(String host, int port) {
        this(host, port, null);
    }

    /** The same place, read as {@code label} (null or blank: as its address). */
    public ServerTarget withLabel(String label) {
        return new ServerTarget(host, port, label == null || label.isBlank() ? null : label);
    }

    /**
     * The target named by launcher fields, or null when they do not name a server.
     *
     * @param port the port, or a negative number when the launcher left it out (the host may then carry it
     *     as {@code host:port})
     */
    public static ServerTarget of(String host, long port) {
        if (host == null) {
            return null;
        }
        if (port < 0) {
            return parse(host);
        }
        String h = host.strip();
        if (h.length() > 2 && h.startsWith("[") && h.endsWith("]")) {
            h = h.substring(1, h.length() - 1);
        }
        if (h.isEmpty() || h.length() > MAX_HOST_CHARS || !validHost(h) || port < 1 || port > 65535) {
            return null;
        }
        return new ServerTarget(h.toLowerCase(Locale.ROOT), (int) port);
    }

    /** Reads an address the way the game keeps one ("host", "host:port", "[v6]:port"), or null. */
    public static ServerTarget parse(String address) {
        if (address == null) {
            return null;
        }
        String a = address.strip();
        if (a.startsWith("[")) {
            int close = a.indexOf(']');
            if (close < 0) {
                return null;
            }
            String rest = a.substring(close + 1);
            if (rest.isEmpty()) {
                return of(a.substring(1, close), DEFAULT_PORT);
            }
            return rest.startsWith(":") ? of(a.substring(1, close), portOf(rest.substring(1))) : null;
        }
        int colon = a.indexOf(':');
        if (colon >= 0 && colon == a.lastIndexOf(':')) {
            return of(a.substring(0, colon), portOf(a.substring(colon + 1)));
        }
        // No port, or a bare IPv6 address.
        return of(a, DEFAULT_PORT);
    }

    /** The port written after the colon; 0 (not a port) when it is not a number or negative. */
    private static long portOf(String s) {
        try {
            return Math.max(0L, Long.parseLong(s.strip()));
        } catch (NumberFormatException e) {
            return 0;
        }
    }

    private static boolean validHost(String h) {
        boolean v6 = h.indexOf(':') >= 0;
        if (!v6 && (h.startsWith(".") || h.startsWith("-"))) {
            return false;
        }
        for (int i = 0; i < h.length(); ) {
            int cp = h.codePointAt(i);
            boolean ok = v6 ? Character.digit(cp, 16) >= 0 || cp == ':' || cp == '.'
                : Character.isLetterOrDigit(cp) || cp == '-' || cp == '.' || cp == '_';
            if (!ok) {
                return false;
            }
            i += Character.charCount(cp);
        }
        return true;
    }

    /** One of Projects' own names. */
    public boolean projects() {
        return host.equals("projects.gg") || host.endsWith(".projects.gg");
    }

    /** The same server: the same host and port, or both on Projects. */
    public boolean sameServer(ServerTarget other) {
        if (other == null) {
            return false;
        }
        if (projects() && other.projects()) {
            return true;
        }
        return host.equals(other.host) && port == other.port;
    }

    /** What the game's own address reader takes; the port only when it is not the default. */
    public String address() {
        String h = host.indexOf(':') >= 0 ? "[" + host + "]" : host;
        return port == DEFAULT_PORT ? h : h + ":" + port;
    }

    /** How the player reads it: its label, "Projects", or the address. */
    public String displayName() {
        return label != null ? label : projects() ? "Projects" : address();
    }
}
