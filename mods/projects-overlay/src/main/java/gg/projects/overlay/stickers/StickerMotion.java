package gg.projects.overlay.stickers;

/**
 * How a sticker moves once when it arrives in the panel (CHAT2 §8, the owner's choice: one still picture moved
 * in code, a pattern per sticker, transform only, never looping; with reduced motion it only fades in). The
 * patterns are the launcher's ({@code packages/stickers/pl1/manifest.json} {@code motions}), sized for a sticker
 * of {@code size} GUI pixels. Every pattern starts and ends at rest. Pure and allocation free: a pose is written
 * into the caller's array.
 */
public final class StickerMotion {
    /** Indexes of a pose: offset, scale (x and y apart, for a squash), angle in degrees, and the pivot. */
    public static final int DX = 0;
    public static final int DY = 1;
    public static final int SX = 2;
    public static final int SY = 3;
    public static final int ANGLE = 4;
    /** 0: the pivot is the sticker's centre; 1: the middle of its base (a wave, a swing). */
    public static final int PIVOT = 5;
    public static final int SIZE = 6;
    /** The size the amplitudes are made for. */
    private static final float BASE = 48f;

    private StickerMotion() {
    }

    /** How long a pattern runs, in milliseconds. */
    public static long duration(String pattern) {
        return switch (pattern == null ? "" : pattern) {
            case "bounce", "beat", "nod", "swing" -> 700;
            case "wobble", "tilt" -> 800;
            case "shake" -> 560;
            case "shiver", "glitch" -> 600;
            case "wave", "float", "sink" -> 900;
            case "sway" -> 1100;
            default -> 520;
        };
    }

    /** Writes the rest pose. */
    public static void rest(float[] pose) {
        pose[DX] = 0f;
        pose[DY] = 0f;
        pose[SX] = 1f;
        pose[SY] = 1f;
        pose[ANGLE] = 0f;
        pose[PIVOT] = 0f;
    }

    /** The pose of {@code pattern} at progress {@code t} (0 to 1) for a sticker of {@code size} GUI pixels. */
    public static void at(String pattern, float t, float size, float[] pose) {
        rest(pose);
        if (t <= 0f || t >= 1f) {
            return;
        }
        float k = size / BASE;
        double pi = Math.PI;
        switch (pattern == null ? "" : pattern) {
            case "bounce" -> {
                // Two hops, the second lower, with a little squash where the first one lands.
                float hop = t < 0.5f ? (float) Math.sin(pi * t / 0.5) : 0.45f * (float) Math.sin(pi * (t - 0.5) / 0.5);
                pose[DY] = -6f * k * hop;
                float landing = Math.max(0f, 1f - Math.abs(t - 0.5f) / 0.08f);
                pose[SX] = 1f + 0.07f * landing;
                pose[SY] = 1f - 0.07f * landing;
                pose[PIVOT] = 1f;
            }
            case "wobble" -> pose[ANGLE] = 9f * (float) (Math.sin(4 * pi * t) * Math.pow(1 - t, 1.5));
            case "beat" -> {
                float s = 1f + 0.13f * pulse(t, 0.18f) + 0.09f * pulse(t, 0.52f);
                pose[SX] = s;
                pose[SY] = s;
            }
            case "shake" -> pose[DX] = 3f * k * (float) (Math.sin(6 * pi * t) * (1 - t));
            case "shiver" -> pose[DX] = 1.2f * k * (float) (Math.sin(14 * pi * t) * Math.sqrt(1 - t));
            case "wave" -> {
                pose[ANGLE] = 12f * (float) (Math.sin(3 * pi * t) * (1 - t));
                pose[PIVOT] = 1f;
            }
            case "nod" -> pose[DY] = 2.5f * k * (float) (Math.abs(Math.sin(2 * pi * t)) * (1 - 0.3 * t));
            case "sway" -> {
                pose[ANGLE] = 7f * (float) Math.sin(pi * smooth(t));
                pose[PIVOT] = 1f;
            }
            case "float" -> {
                float lift = (float) Math.sin(pi * t);
                pose[DY] = -4f * k * lift;
                pose[SX] = 1f + 0.03f * lift;
                pose[SY] = 1f + 0.03f * lift;
            }
            case "tilt" -> pose[ANGLE] = 10f * (float) Math.sin(pi * t);
            case "swing" -> {
                pose[ANGLE] = t < 0.3f ? between(0f, -12f, t / 0.3f)
                    : t < 0.6f ? between(-12f, 14f, (t - 0.3f) / 0.3f) : between(14f, 0f, (t - 0.6f) / 0.4f);
                pose[PIVOT] = 1f;
            }
            case "sink" -> pose[DY] = 4f * k * (float) Math.sin(pi * t);
            case "glitch" -> {
                // Jumps between offset positions, like a frame that lags.
                pose[DX] = k * (t < 0.1f ? 0f : t < 0.25f ? 2f : t < 0.4f ? -2f : t < 0.55f ? 1.5f
                    : t < 0.7f ? -1f : 0f);
                pose[DY] = k * (t >= 0.25f && t < 0.4f ? 1f : 0f);
            }
            default -> {
                // "pop": grows quickly and settles back.
                float s = t < 0.35f ? (float) Math.sin(pi / 2 * t / 0.35) : (float) Math.cos(pi / 2 * (t - 0.35) / 0.65);
                pose[SX] = 1f + 0.14f * s;
                pose[SY] = 1f + 0.14f * s;
            }
        }
    }

    /** A bump centred on {@code centre}, 0.28 wide. */
    private static float pulse(float t, float centre) {
        float u = (t - centre + 0.14f) / 0.28f;
        return u <= 0f || u >= 1f ? 0f : (float) Math.sin(Math.PI * u);
    }

    /** From {@code a} to {@code b} on a cosine curve. */
    private static float between(float a, float b, float u) {
        return a + (b - a) * (float) (1 - Math.cos(Math.PI * Math.max(0f, Math.min(1f, u)))) / 2f;
    }

    private static float smooth(float t) {
        return t * t * (3f - 2f * t);
    }
}
