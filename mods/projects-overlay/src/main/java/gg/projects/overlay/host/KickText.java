package gg.projects.overlay.host;

/**
 * What a guest reads when the shared world turns them away or sends them off (host1.md §4.6, PROTOCOL.md §6.9). Guests
 * have no mod, so plain text goes instead of a translation key: in the guest's own language once the game knows it
 * (its client information, Turkish or English, anything else English). A refusal at login comes before the game
 * hears the guest's language, so it says both, Turkish first; the guest's launcher has usually told them already.
 */
public final class KickText {
    /** Why a connection does not get in. */
    public enum Refusal { NAME, NAME_TAKEN, FULL, REMOVED, CLOSED }

    private KickText() {
    }

    /** A game language code ({@code tr_tr}, {@code en_us}) reads Turkish. */
    public static boolean turkish(String language) {
        return language != null && language.regionMatches(true, 0, "tr", 0, 2)
            && (language.length() == 2 || language.charAt(2) == '_' || language.charAt(2) == '-');
    }

    /** At login, before the guest's language is known: both languages, Turkish first. */
    public static String atLogin(Refusal refusal, String host) {
        return line(refusal, host, true) + "\n" + line(refusal, host, false);
    }

    /** In the world, in the guest's language. */
    public static String inWorld(Refusal refusal, String host, String language) {
        return line(refusal, host, turkish(language));
    }

    static String line(Refusal refusal, String host, boolean turkish) {
        String who = host == null || host.isBlank() ? null : host;
        return switch (refusal) {
            case NAME -> turkish ? "Bu adla giremezsin." : "You can't join with this name.";
            case NAME_TAKEN -> turkish ? "Bu ad bu dünyada başka birine ait." : "Someone else uses this name in this world.";
            case FULL -> turkish ? "Dünya dolu." : "The world is full.";
            case REMOVED -> turkish ? "Bu dünyadan çıkarıldın." : "You were removed from this world.";
            case CLOSED -> who == null
                ? (turkish ? "Dünya paylaşımı kapandı." : "The world stopped being shared.")
                : (turkish ? who + " paylaşımı kapattı." : who + " stopped sharing their world.");
        };
    }
}
