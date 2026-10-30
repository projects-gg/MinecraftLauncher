package gg.projects.overlay.ui.draw;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.EnumMap;
import java.util.List;
import java.util.Map;

/** The overlay's icons: Phosphor (the launcher's family), outlines in {@code icons/phosphor.txt}. */
public enum Icon {
    GEAR("GearSix"),
    BACK("CaretLeft"),
    NEXT("CaretRight"),
    DOWN("CaretDown"),
    SEND("PaperPlaneRight"),
    CLOSE("X"),
    CHECK("Check"),
    CLOCK("Clock"),
    WARNING("WarningCircle"),
    CREW("UsersThree"),
    CALL("MegaphoneSimple"),
    CHATS("ChatsCircle"),
    INVITE("GameController"),
    INFO("Info"),
    BELL("Bell"),
    SOUND("SpeakerHigh"),
    KEYBOARD("Keyboard"),
    RESET("ArrowCounterClockwise"),
    UNPLUGGED("Plugs"),
    JOIN("SignIn"),
    SMILEY("Smiley"),
    REPLY("ArrowBendUpLeft"),
    STICKER("Sticker"),
    GLOBE("Globe"),
    OPEN("ArrowSquareOut"),
    // The friend menu (OVL-UX), with the launcher's person menu signs (person-menu.tsx).
    /** A friend row's "…". */
    DOTS("DotsThree"),
    /** "Mesaj gönder". */
    MESSAGE("ChatCircle"),
    /** "Birlikte oyna": the launcher's play-together invite (its sign there too). */
    PLAY_INVITE("PaperPlaneTilt"),
    /** "Dünyana çağır": an invite to the player's own world (HOST1), the launcher's sign for it. */
    WORLD_INVITE("DoorOpen"),
    /** "Dünyadan çıkar". */
    SEND_OFF("SignOut"),
    /** "Arkadaşlıktan çıkar" (the same outline as {@link #USER_MINUS}). */
    UNFRIEND("UserMinus"),
    /** "Engelle". */
    BLOCK("Prohibit"),
    /** "Rapor et". */
    REPORT("Flag"),
    /** A report reason not picked, and the one picked. */
    RADIO_OFF("Circle"),
    RADIO_ON("RadioButton"),
    /** "Ece ekibe katıldı" in crew chat (TEAM6 §7), as the launcher's line. */
    WAVE("HandWaving"),
    /** The Discord account in that question: the symbol alone, uncoloured, as the launcher's link area. */
    DISCORD("DiscordLogo"),
    /** "Arkadaşlarını çağır" (HOST1): the launcher's "Arkadaşlarınla oyna" carries the same sign. */
    USER_PLUS("UserPlus"),
    /** "Dünyadan çıkar" on a guest's row (HOST1). */
    USER_MINUS("UserMinus"),
    /** The "Paylaşımı kapat" question (HOST1): the world is the player's alone again. */
    LOCK("LockSimple"),
    // The rest of the launcher's person menu (OVL-UX2), with its signs.
    /** "Favorilere ekle", and the list's star (filled: a favourite; "Favorilerden çıkar"). */
    STAR("Star"),
    STAR_FILL("StarFill"),
    /** "Çevrimiçi olunca bildir": filled while it is on (with the row's check). */
    BELL_FILL("BellFill"),
    /** "Kategoriye taşı". */
    FOLDER("FolderSimple"),
    /** "İsteği kabul et" on a crewmate who asked first. */
    USER_CHECK("UserCheck"),
    // A crewmate's crew rows and "Yeni kategori…" (OVL-UX3), with the launcher's signs.
    /** "Moderatör yap" and "Moderatörlükten al". */
    SHIELD_STAR("ShieldStar"),
    /** "Liderliği devret". */
    CROWN("Crown"),
    /** "Yeni kategori…" at the end of "Kategoriye taşı"'s list. */
    FOLDER_PLUS("FolderSimplePlus");

    /** The sizes icons are drawn at, in GUI pixels. */
    public static final int[] SIZES = {8, 10, 12};
    static final float VIEW_BOX = 256f;
    private static final String RESOURCE = "/assets/projects_overlay/icons/phosphor.txt";

    public final String phosphorName;
    /** Atlas names per size, made on first use (icons are looked up every frame). */
    private String[] keys;

    Icon(String phosphorName) {
        this.phosphorName = phosphorName;
    }

    public String key(int size) {
        String[] made = keys;
        if (made == null) {
            made = new String[SIZES.length];
            for (int i = 0; i < SIZES.length; i++) {
                made[i] = "icon:" + name() + ":" + SIZES[i];
            }
            keys = made;
        }
        for (int i = 0; i < SIZES.length; i++) {
            if (SIZES[i] == size) {
                return made[i];
            }
        }
        return "icon:" + name() + ":" + size;
    }

    /** Every icon's outline, read from the mod's resources. */
    public static Map<Icon, String> loadPaths() throws IOException {
        Map<Icon, String> paths = new EnumMap<>(Icon.class);
        try (InputStream in = Icon.class.getResourceAsStream(RESOURCE)) {
            if (in == null) {
                throw new IOException("missing " + RESOURCE);
            }
            BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
            for (String line; (line = reader.readLine()) != null; ) {
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                String[] parts = line.split("\t", 3);
                if (parts.length < 3) {
                    continue;
                }
                for (Icon icon : values()) {
                    if (icon.phosphorName.equals(parts[0])) {
                        paths.put(icon, parts[2]);
                    }
                }
            }
        }
        for (Icon icon : values()) {
            if (!paths.containsKey(icon)) {
                throw new IOException("no outline for " + icon.phosphorName);
            }
        }
        return paths;
    }

    /** Outlines ready to fill at {@code pixels} output pixels. */
    static List<float[]> contours(String path, float pixels) {
        // Curves within a twentieth of an output pixel of their true shape.
        float tolerance = VIEW_BOX / Math.max(1f, pixels) / 20f;
        return SvgPath.parse(path, tolerance).contours();
    }
}
