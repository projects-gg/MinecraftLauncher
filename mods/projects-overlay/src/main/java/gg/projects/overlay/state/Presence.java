package gg.projects.overlay.state;

/**
 * Where a person stands, as the launcher's presence dot says it (OVL-SOC; docs/design.md "Friends (S2)", the launcher's
 * {@code PresenceDot}): in a game, in the launcher, on "Rahatsız etmeyin", or offline (no dot, the face dims). The
 * overlay draws it with {@link gg.projects.overlay.ui.PresenceDot}, the same in every list of people.
 */
public enum Presence {
    PLAYING,
    LAUNCHER,
    DND,
    OFFLINE;

    /** It has a dot: anything but offline. */
    public boolean online() {
        return this != OFFLINE;
    }
}
