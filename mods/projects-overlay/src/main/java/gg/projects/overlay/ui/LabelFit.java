package gg.projects.overlay.ui;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A label in a box (a button, a chip; LABEL-FIT, the owner, 4 Nov: "eğer butondan taşacak gibi olursa font küçültelim
 * sığsın içine"): its icon and words at the box's own size when they fit the box's inner width at the current GUI
 * scale; else the words a size smaller, as far as the smallest (about three quarters of a button's: the overlay's
 * faces are made at their own sizes for every scale, so smaller words stay sharp, never a scaled picture of bigger
 * ones); then without the icon, at the largest size that fits; only then cut with "…", the whole label then the box's
 * tooltip. The game's own pixel font (GUI scale 1, "Force Unicode Font") has one size: there the icon goes first. The
 * words never cross the box's edge. Pure: the caller measures.
 */
public final class LabelFit {
    /**
     * How a label is drawn in its box: in {@code face}, with its icon or not, {@code text} (the label, or cut with "…":
     * {@code cut}), {@code width} wide with the icon's room.
     */
    public record Fit(Fonts.Face face, boolean icon, String text, boolean cut, int width) {
    }

    /** Measures {@code text} in {@code face}, GUI pixels. */
    public interface Measure {
        int width(Fonts.Face face, String text);
    }

    /** Measures in the overlay's own faces, as the game draws them now. */
    public static final Measure FONTS = (face, text) -> Fonts.get(face).width(text);
    private static final String ELLIPSIS = "…";
    /** The dev scenarios' before-and-after: off lays every label out at its own size, as before LABEL-FIT. */
    private static boolean fitting = true;
    private static int generation;
    /** The dev scenarios' audit: the labels that did not fit at their own size, and how they fit (null: not kept). */
    private static Map<String, String> notes;

    private LabelFit() {
    }

    /** Lays labels out as LABEL-FIT does, or (off) at their own size as before it (the dev scenarios). */
    public static void fittingForDriver(boolean on) {
        fitting = on;
        generation++;
    }

    /** Changes whenever {@link #fittingForDriver} does: a widget keeping a fit works it out again. */
    public static int generation() {
        return generation;
    }

    /** Starts (or ends, {@code false}) keeping the labels that did not fit at their own size (the dev scenarios). */
    public static void recordForDriver(boolean on) {
        notes = on ? new LinkedHashMap<>() : null;
    }

    /** The labels kept since {@link #recordForDriver}: label and GUI scale, and how each fit. */
    public static Map<String, String> notesForDriver() {
        return notes == null ? Map.of() : new LinkedHashMap<>(notes);
    }

    /** Keeps {@code fit} for the audit when the label (in {@code own}, with an icon or not) did not fit as it is. */
    public static void noteForDriver(String label, Fonts.Face own, boolean icon, int scale, Fit fit) {
        if (notes != null && (fit.cut() || fit.face() != own || fit.icon() != icon)) {
            notes.put(label + " @" + scale, fit.face() + (fit.icon() ? "+icon" : "") + (fit.cut() ? " cut: " + fit.text()
                : ""));
        }
    }

    /**
     * The sizes a label set in {@code face} may take, its own first: a button's words (UI) down to the smallest face,
     * a chip's (SMALL) one step. With the game's own font ({@code bundled} false) only its own.
     */
    public static List<Fonts.Face> steps(Fonts.Face face, boolean bundled) {
        if (!bundled) {
            return List.of(face);
        }
        return switch (face) {
            case UI, STRONG -> List.of(face, Fonts.Face.SMALL, Fonts.Face.MINI);
            case SMALL -> List.of(face, Fonts.Face.MINI);
            default -> List.of(face);
        };
    }

    /**
     * How {@code label} (in {@code face}; {@code iconRoom} the icon and its gap before the words, 0 without an icon)
     * fits {@code inner} GUI pixels.
     */
    public static Fit fit(String label, Fonts.Face face, int iconRoom, int inner, boolean bundled, Measure measure) {
        if (!fitting) {
            return new Fit(face, iconRoom > 0, label, false, iconRoom + measure.width(face, label));
        }
        List<Fonts.Face> faces = steps(face, bundled);
        if (iconRoom > 0) {
            for (Fonts.Face f : faces) {
                int w = iconRoom + (label.isEmpty() ? 0 : measure.width(f, label));
                if (w <= inner) {
                    return new Fit(f, true, label, false, w);
                }
            }
        }
        for (Fonts.Face f : faces) {
            int w = measure.width(f, label);
            if (w <= inner) {
                return new Fit(f, false, label, false, w);
            }
        }
        Fonts.Face smallest = faces.get(faces.size() - 1);
        String text = cut(label, smallest, inner, measure);
        return new Fit(smallest, false, text, true, measure.width(smallest, text));
    }

    /**
     * A row of buttons sharing {@code total} GUI pixels: equal shares while each one's {@code want} (its whole label and
     * icon) fits its share, else shares by what each wants, so the widest label gets the most room before any gets
     * smaller (the voice bar's "Microphone" beside "Deafen" and "Leave"). The last takes what rounding leaves.
     */
    public static int[] row(int total, int[] want) {
        int n = want.length;
        int[] widths = new int[n];
        if (n == 0) {
            return widths;
        }
        int equal = total / n;
        boolean fits = true;
        long sum = 0;
        for (int w : want) {
            fits &= w <= equal;
            sum += Math.max(1, w);
        }
        // As before LABEL-FIT (the dev scenarios' "before"): equal shares whatever they hold.
        fits |= !fitting;
        int used = 0;
        for (int i = 0; i < n - 1; i++) {
            widths[i] = fits ? equal : (int) (total * Math.max(1, want[i]) / sum);
            used += widths[i];
        }
        widths[n - 1] = total - used;
        return widths;
    }

    /** The smallest of {@code faces} by size (a row's buttons all take the size its narrowest one needed). */
    public static Fonts.Face smallest(Fonts.Face... faces) {
        Fonts.Face least = faces[0];
        for (Fonts.Face face : faces) {
            if (face.size < least.size) {
                least = face;
            }
        }
        return least;
    }

    /** {@code label} cut to {@code inner} with "…" (at least the ellipsis; nothing when not even that fits). */
    static String cut(String label, Fonts.Face face, int inner, Measure measure) {
        int end = label.length();
        while (end > 0) {
            String text = label.substring(0, end).stripTrailing() + ELLIPSIS;
            if (measure.width(face, text) <= inner) {
                return text;
            }
            end--;
            // Never between the two halves of a surrogate pair.
            if (end > 0 && Character.isHighSurrogate(label.charAt(end - 1))) {
                end--;
            }
        }
        return measure.width(face, ELLIPSIS) <= inner ? ELLIPSIS : "";
    }
}
