package gg.projects.overlay.keys;

import com.sun.jna.Library;
import com.sun.jna.Memory;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;

/**
 * Whether this computer's keyboard may give F1 to F12 only with Fn ({@link KeyHint}; F7-OPEN): a laptop, or a Mac,
 * whose keyboards' top row is media keys by default. A laptop is a computer with a system battery: on Windows the
 * system's own power status (kernel32's GetSystemPowerStatus through JNA, which the game ships for OSHI), on Linux a
 * battery in /sys/class/power_supply that is not a device's (a wireless mouse's). Whatever cannot tell answers no: the
 * line is help where it may be the reason, never a guess at every desk.
 */
public final class Laptop {
    /** {@code SYSTEM_POWER_STATUS.BatteryFlag}: "no system battery". */
    static final int NO_BATTERY = 128;
    /** {@code SYSTEM_POWER_STATUS.BatteryFlag}: "unknown status, unable to read the battery flag information". */
    static final int UNKNOWN = 255;

    private Laptop() {
    }

    /** Looks once (a few system calls; the caller keeps the answer). Never throws. */
    public static boolean detect() {
        try {
            String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
            if (os.startsWith("mac")) {
                return true;
            }
            if (os.startsWith("windows")) {
                return windowsBattery();
            }
            if (os.startsWith("linux")) {
                return linuxBattery(Path.of("/sys/class/power_supply"));
            }
        } catch (Throwable e) {
            // JNA missing or refused (a LinkageError), a file that cannot be read: no answer.
        }
        return false;
    }

    /** What {@code SYSTEM_POWER_STATUS.BatteryFlag} says: a system battery is there. */
    static boolean hasBattery(int batteryFlag) {
        return batteryFlag != NO_BATTERY && batteryFlag != UNKNOWN;
    }

    /** A system battery among Linux's power supplies ({@code type} "Battery", {@code scope} not "Device"). */
    static boolean linuxBattery(Path supplies) throws IOException {
        if (!Files.isDirectory(supplies)) {
            return false;
        }
        try (DirectoryStream<Path> all = Files.newDirectoryStream(supplies)) {
            for (Path supply : all) {
                if ("Battery".equals(read(supply.resolve("type"))) && !"Device".equals(read(supply.resolve("scope")))) {
                    return true;
                }
            }
        }
        return false;
    }

    private static String read(Path file) {
        try {
            return Files.readString(file).strip();
        } catch (IOException e) {
            return null;
        }
    }

    private static boolean windowsBattery() {
        // SYSTEM_POWER_STATUS: ACLineStatus, BatteryFlag, BatteryLifePercent, SystemStatusFlag (a byte each), then two
        // DWORDs; 12 bytes.
        Memory status = new Memory(12);
        status.clear();
        return Kernel32.INSTANCE.GetSystemPowerStatus(status) && hasBattery(status.getByte(1) & 0xFF);
    }

    /** The one kernel32 call this needs. */
    interface Kernel32 extends Library {
        Kernel32 INSTANCE = Native.load("kernel32", Kernel32.class);

        boolean GetSystemPowerStatus(Pointer status);
    }
}
