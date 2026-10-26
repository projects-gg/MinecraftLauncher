package gg.projects.overlay.emoji;

/** The picker's groups (docs/content-format.md, "Chat emojis"). */
public enum EmojiKind {
    /** A single symbol ("❤"): a square cell. */
    SYMBOL,
    /** An expression or a short word ("¯\_(ツ)_/¯", "GG"): as wide as its words. */
    TEXT,
    /** A picture of the server's resource pack, with its look for players without the pack. */
    PACK,
    /** A code the server fills in with the player's own value or a link (":ping:"): a row with its meaning. */
    SHORTCUT;

    /** The content format's name of a kind; one this build does not know is shown with the symbols. */
    public static EmojiKind read(String name) {
        if (name == null) {
            return SYMBOL;
        }
        return switch (name) {
            case "text" -> TEXT;
            case "pack" -> PACK;
            case "shortcut" -> SHORTCUT;
            default -> SYMBOL;
        };
    }
}
