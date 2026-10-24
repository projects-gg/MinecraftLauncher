package gg.projects.overlay.bridge;

import java.util.regex.Pattern;

/**
 * Where this game session finds the launcher's bridge. Projects Launcher sets both values as JVM system
 * properties for each launch; any other launcher leaves them out and the overlay stays off.
 */
public final class BridgeConfig {
    public static final String PORT_PROPERTY = "projects.overlay.port";
    public static final String TOKEN_PROPERTY = "projects.overlay.token";

    private static final Pattern TOKEN = Pattern.compile("[A-Za-z0-9_-]{32,128}");

    private final int port;
    private final String token;

    BridgeConfig(int port, String token) {
        this.port = port;
        this.token = token;
    }

    /**
     * Reads the launcher's properties and removes the token from the system properties so that mods which dump
     * them into logs cannot leak it. Returns null when the game was not started by Projects Launcher.
     */
    public static BridgeConfig fromSystemProperties() {
        String portText = System.getProperty(PORT_PROPERTY);
        String token = System.getProperty(TOKEN_PROPERTY);
        System.clearProperty(TOKEN_PROPERTY);
        return parse(portText, token);
    }

    static BridgeConfig parse(String portText, String token) {
        if (portText == null || token == null || !TOKEN.matcher(token).matches()) {
            return null;
        }
        int port;
        try {
            port = Integer.parseInt(portText.trim());
        } catch (NumberFormatException e) {
            return null;
        }
        if (port < 1024 || port > 65535) {
            return null;
        }
        return new BridgeConfig(port, token);
    }

    public int port() {
        return port;
    }

    String token() {
        return token;
    }

    /**
     * The world sharing gate's key (PROTOCOL.md §7.1), derived from the token: the token itself never leaves this
     * class.
     */
    public byte[] gateKey() {
        return gg.projects.overlay.host.PlgHeader.gateKey(token);
    }

    @Override
    public String toString() {
        return "BridgeConfig[port=" + port + ", token=<redacted>]";
    }
}
