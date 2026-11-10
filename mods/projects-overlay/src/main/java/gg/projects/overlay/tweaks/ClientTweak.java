package gg.projects.overlay.tweaks;

/**
 * One client tweak: a small fix to the game's own screens or controls, with its own switch in the launcher (DIALOG-FOCUS;
 * {@link ClientTweaks} says how to add one). It needs neither the launcher's bridge nor the overlay's panel: it runs
 * whenever the launcher named it at the game's start.
 */
public interface ClientTweak {
    /**
     * Its id in the launch property ({@link ClientTweaks#PROPERTY}): lowercase letters, digits and '-', the same as the
     * launcher's (`pl_overlay::tweaks`).
     */
    String id();

    /**
     * Hooks the tweak into the game, once, as the game starts. Each hook it registers runs its work through
     * {@code guard} ({@link TweakGuard#run}), so a failure turns this tweak off and nothing else.
     */
    void install(TweakGuard guard);
}
