package gg.projects.overlay.keys;

/**
 * The one line a toast's key hint says on this computer (KEY-HINT; the owner, 4 Nov: "tek satır bulundurmak lazım 2
 * satır yerine", and "gerçekten f7 mi gerekli fn 7 mi gerekli onu anlamamız lazım"): "Açmak için [F7] tuşuna bas", or,
 * where this computer gives F1 to F12 only with Fn held, "Açmak için [Fn] + [F7] tuşlarına bas". Never both; none while
 * the player turned "Açma ipucunu göster" off.
 *
 * <p>What the game's window hears of a laptop's top row (GLFW's key callback as the game gets it, nothing from the
 * system):
 * <ul>
 * <li>Windows: in the top row's media mode a key sends what its maker chose. Sound and media keys reach the window as
 * keys GLFW does not name (an unknown key with its scancode: 0x12E and 0x130 sound down and up, 0x120 mute,
 * 0x110/0x119/0x122 tracks and play); the touchpad and Copilot keys as F23 or F24 with Ctrl, Shift and Windows held.
 * Brightness, flight mode and the display key reach no program (the firmware and the shell take them). Fn itself
 * reaches no program at all; with it held the key comes as F7, as from a desktop keyboard.</li>
 * <li>macOS: brightness, Mission Control, media and sound are the system's and never reach the game; F7 comes only with
 * fn held (or with "Use F1, F2, etc. keys as standard function keys" on). The fn key itself does reach GLFW, as a
 * modifier it does not name: an unknown key with scancode 63 (kVK_Function), told as a release on press and let-go
 * alike.</li>
 * <li>Linux: media keys come as XF86 keys GLFW does not name, unless the desktop takes them first (GNOME and KDE do);
 * brightness is the system's.</li>
 * </ul>
 *
 * <p>So on a laptop or a Mac with the overlay key on F1 to F12 (elsewhere: the plain line): Windows and Linux start with
 * the plain line (right for gaming laptops and Fn Lock) and change to the Fn line at once when, while a hint is on
 * screen and the key never opened the panel here, an unnamed key or F13 to F25 goes down: the top row sent something
 * else ({@link #media}). A Mac starts with the Fn line (Macs come with media keys first, and those never reach the game:
 * nothing could tell it otherwise); the key opening the panel with no fn just before it means its F-keys are standard,
 * and the plain line stays from then on ({@link #macFn}). Once the key has opened the panel the hint learns nothing
 * more. Only key events the game's window already hears, only while a hint is on screen, and only the conclusion is
 * kept ({@code keyUsed}, {@code fnNeeded} with the overlay settings). Pure.
 */
public final class KeyHint {
    /** What a key hint says. */
    public enum Line {
        /** No hint ("Açma ipucunu göster" off). */
        NONE,
        /** "Açmak için [F7] tuşuna bas". */
        KEY,
        /** "Açmak için [Fn] + [F7] tuşlarına bas". */
        FN
    }

    /** GLFW's key for one it does not name, and its press. */
    static final int UNKNOWN = -1;
    static final int PRESS = 1;
    /** GLFW's F13 and F25: no keyboard key sends them but firmware keys. */
    static final int F13 = 302;
    static final int F25 = 314;
    /** macOS's fn key's scancode (kVK_Function), as GLFW passes it on. */
    static final int MAC_FN = 63;
    /** On a Mac, fn this soon before the overlay key went down with it. */
    public static final long FN_HELD_MS = 1500;

    private KeyHint() {
    }

    /**
     * The line for the overlay key on this computer: {@code shown} the player's "Açma ipucunu göster", {@code
     * functionKey} the key is one of F1 to F12, {@code laptop} a laptop or a Mac, {@code keyUsed} the key opened the
     * panel here, {@code fnNeeded} this computer's F-keys came only with Fn.
     */
    public static Line line(boolean shown, boolean functionKey, boolean laptop, boolean mac, boolean keyUsed,
        boolean fnNeeded) {
        if (!shown) {
            return Line.NONE;
        }
        if (!functionKey || !laptop) {
            return Line.KEY;
        }
        return fnNeeded || (mac && !keyUsed) ? Line.FN : Line.KEY;
    }

    /**
     * A key event the game's window heard ({@code key} GLFW's, {@code action} its press, release or repeat) that tells
     * the top row sent something else than an F-key: a key GLFW does not name, or F13 to F25, going down. Never on a Mac
     * (its media keys never reach the game; an unnamed key there is fn or another modifier).
     */
    public static boolean media(int key, int action, boolean mac) {
        return !mac && action == PRESS && (key == UNKNOWN || (key >= F13 && key <= F25));
    }

    /** macOS's fn key going down or up, as GLFW tells it (an unnamed key, scancode 63, either action). */
    public static boolean macFn(int key, int scancode, boolean mac) {
        return mac && key == UNKNOWN && scancode == MAC_FN;
    }

    /**
     * Whether the key opening the panel at {@code now} went down with fn (a Mac; {@code fnAt} when fn last moved): the
     * F-keys need it.
     */
    public static boolean openedWithFn(boolean mac, long fnAt, long now) {
        return mac && now - fnAt >= 0 && now - fnAt <= FN_HELD_MS;
    }
}
