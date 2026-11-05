package gg.projects.overlay.ui.draw;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.EnumMap;
import java.util.List;
import java.util.Map;

/**
 * The overlay's icons: Phosphor (the launcher's family), outlines in {@code icons/phosphor.txt}; brand marks (Discord's
 * own symbol) in {@code icons/brands.txt}, the same 256 box.
 */
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
    /**
     * Discord's own symbol (its brand assets, the launcher's {@code discord-symbol-white.png}), uncoloured: the Discord
     * account in that question and "Discord'dan mesaj at". Phosphor's drawing of it read as a blob at the menu's size
     * (the owner: "discord logosu orijinali gibi olmamış").
     */
    DISCORD("DiscordMark"),
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
    FOLDER_PLUS("FolderSimplePlus"),
    // Voice chat (VOICE-O), with the launcher's signs.
    /** A voice room (the panel's strip). */
    HEADSET("Headset"),
    /** The microphone open ("Mikrofon"; "Herkes için susturmayı kaldır"). */
    MIC("Microphone"),
    /** The microphone closed, and "Herkes için sustur". */
    MIC_OFF("MicrophoneSlash"),
    /** Hearing ("Sağırlaştır" off): the headphones, as players say it ("kulaklık"). */
    HEADPHONES("Headphones"),
    /**
     * Deafened ("Sağırlaştır" on; the microphone is off with it): the headphones with Phosphor's slash, cut as its
     * slashed icons are (VOICE-KEYS; Phosphor has no HeadphonesSlash; the launcher draws the same).
     */
    DEAF("Headphones", true),
    /** "Benim için sustur". */
    MUTE_FOR_ME("SpeakerSimpleSlash"),
    /** "Ara", and a ring's "Katıl". */
    PHONE("Phone"),
    /** "Odaya davet et". */
    PHONE_PLUS("PhonePlus"),
    /** A ring's "Reddet". */
    PHONE_X("PhoneX"),
    /** "Ayrıl" ("İptal" while the player's call rings). */
    PHONE_OFF("PhoneDisconnect"),
    // Crew chat's "Sesli" ("Yeni oda") and the chat's emoji picker (OVL-SOC), with the launcher's signs.
    PLUS("Plus"),
    SEARCH("MagnifyingGlass"),
    RECENT("ClockCounterClockwise"),
    PAW("PawPrint"),
    FOOD("Hamburger"),
    TRAVEL("AirplaneTilt"),
    BALL("SoccerBall"),
    BULB("Lightbulb"),
    SHAPES("Shapes"),
    /** Screen sharing in a voice room (OVL-SHARE). */
    SCREENCAST("Screencast");

    /** The sizes icons are drawn at, in GUI pixels. */
    public static final int[] SIZES = {8, 10, 12};
    static final float VIEW_BOX = 256f;
    private static final String[] RESOURCES = {"/assets/projects_overlay/icons/phosphor.txt",
        "/assets/projects_overlay/icons/brands.txt"};

    public final String phosphorName;
    /** Drawn with Phosphor's slash across it ({@link ShapeBaker#slashed}). */
    public final boolean slashed;
    /** Atlas names per size, made on first use (icons are looked up every frame). */
    private String[] keys;

    Icon(String phosphorName) {
        this(phosphorName, false);
    }

    Icon(String phosphorName, boolean slashed) {
        this.phosphorName = phosphorName;
        this.slashed = slashed;
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
        for (String resource : RESOURCES) {
            read(resource, paths);
        }
        for (Icon icon : values()) {
            if (!paths.containsKey(icon)) {
                throw new IOException("no outline for " + icon.phosphorName);
            }
        }
        return paths;
    }

    /** The outlines of one resource file ("name<TAB>weight<TAB>path" lines) for the icons that name them. */
    private static void read(String resource, Map<Icon, String> paths) throws IOException {
        try (InputStream in = Icon.class.getResourceAsStream(resource)) {
            if (in == null) {
                throw new IOException("missing " + resource);
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
    }

    /** Outlines ready to fill at {@code pixels} output pixels. */
    static List<float[]> contours(String path, float pixels) {
        // Curves within a twentieth of an output pixel of their true shape.
        float tolerance = VIEW_BOX / Math.max(1f, pixels) / 20f;
        return SvgPath.parse(path, tolerance).contours();
    }
}
