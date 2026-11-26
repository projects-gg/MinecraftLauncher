package gg.projects.overlay.tweaks;

/**
 * PASSWORD-EYE's hold on a text box ({@code EditBoxMixin} gives every box this): {@link PasswordEye} marks a server
 * window's password boxes, and a marked box shows filled dots and an eye that shows and hides what was typed.
 */
public interface SecretBox {
    /** Marks this box as a password box ({@code true}) or a plain one. */
    void projects$secret(boolean secret);

    /** This is a password box and the tweak is on: it has the eye. */
    boolean projects$isSecret();

    /** This is a password box, the tweak is on and the player has not shown what was typed: it shows dots. */
    boolean projects$hidden();
}
