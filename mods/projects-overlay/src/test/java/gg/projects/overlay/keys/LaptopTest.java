package gg.projects.overlay.keys;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** F7-OPEN: which computers may give F1 to F12 only with Fn (a laptop: a system battery). */
class LaptopTest {
    @TempDir
    Path dir;

    @Test
    void windowsBatteryFlagsTellALaptop() {
        // SYSTEM_POWER_STATUS.BatteryFlag: high, low, critical, charging (and their sums) are a battery.
        for (int flag : new int[] {0, 1, 2, 4, 8, 9, 10}) {
            assertTrue(Laptop.hasBattery(flag), Integer.toString(flag));
        }
        assertFalse(Laptop.hasBattery(Laptop.NO_BATTERY), "a desktop");
        assertFalse(Laptop.hasBattery(Laptop.UNKNOWN), "no answer is no laptop");
    }

    @Test
    void linuxCountsTheSystemBatteryOnly() throws IOException {
        assertFalse(Laptop.linuxBattery(dir.resolve("missing")));
        supply("AC", "Mains", null);
        assertFalse(Laptop.linuxBattery(dir), "mains power alone");
        // A wireless mouse's battery is a device's, not the computer's.
        supply("hidpp_battery_0", "Battery", "Device");
        assertFalse(Laptop.linuxBattery(dir));
        supply("BAT0", "Battery", "System");
        assertTrue(Laptop.linuxBattery(dir));
        Path older = dir.resolve("older");
        Files.createDirectories(older.resolve("BAT1"));
        Files.writeString(older.resolve("BAT1").resolve("type"), "Battery\n");
        assertTrue(Laptop.linuxBattery(older), "a kernel without the scope file");
    }

    private void supply(String name, String type, String scope) throws IOException {
        Path at = Files.createDirectories(dir.resolve(name));
        Files.writeString(at.resolve("type"), type + "\n");
        if (scope != null) {
            Files.writeString(at.resolve("scope"), scope + "\n");
        }
    }
}
