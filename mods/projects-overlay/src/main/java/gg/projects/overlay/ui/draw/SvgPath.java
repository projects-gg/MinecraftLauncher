package gg.projects.overlay.ui.draw;

import java.util.ArrayList;
import java.util.List;

/**
 * Turns SVG path data (the Phosphor icons' outlines) into closed polygons: straight lines as they are,
 * Bézier curves and elliptical arcs cut into short segments. Supports M, L, H, V, C, S, Q, T, A and Z in
 * both absolute and relative form. Pure.
 */
public final class SvgPath {
    /** Closed outlines; each is x0, y0, x1, y1, … in the path's own units. */
    private final List<float[]> contours;

    private SvgPath(List<float[]> contours) {
        this.contours = contours;
    }

    public List<float[]> contours() {
        return contours;
    }

    /**
     * Parses {@code d}. {@code tolerance} is the largest gap allowed between a curve and its segments, in
     * the path's units (a fraction of an output pixel keeps curves smooth).
     */
    public static SvgPath parse(String d, float tolerance) {
        Parser p = new Parser(d, Math.max(0.01f, tolerance));
        p.run();
        return new SvgPath(p.contours);
    }

    private static final class Parser {
        private final String s;
        private final float tolerance;
        private final List<float[]> contours = new ArrayList<>();
        private int i;
        private float[] points = new float[64];
        private int count;
        private float x;
        private float y;
        private float startX;
        private float startY;
        private float lastCtrlX;
        private float lastCtrlY;
        private char lastCommand;

        Parser(String s, float tolerance) {
            this.s = s;
            this.tolerance = tolerance;
        }

        void run() {
            char command = 0;
            while (true) {
                skipSeparators();
                if (i >= s.length()) {
                    break;
                }
                char c = s.charAt(i);
                if (Character.isLetter(c)) {
                    command = c;
                    i++;
                } else if (command == 0) {
                    throw new IllegalArgumentException("path starts without a command");
                }
                command = apply(command);
            }
            closeContour();
        }

        /** Runs one command and returns the command that repeated numbers continue with. */
        private char apply(char command) {
            boolean rel = Character.isLowerCase(command);
            char up = Character.toUpperCase(command);
            switch (up) {
                case 'M' -> {
                    closeContour();
                    float nx = number() + (rel ? x : 0f);
                    float ny = number() + (rel ? y : 0f);
                    x = nx;
                    y = ny;
                    startX = nx;
                    startY = ny;
                    add(x, y);
                    lastCommand = 'M';
                    return rel ? 'l' : 'L';
                }
                case 'L' -> lineTo(number() + (rel ? x : 0f), number() + (rel ? y : 0f));
                case 'H' -> lineTo(number() + (rel ? x : 0f), y);
                case 'V' -> lineTo(x, number() + (rel ? y : 0f));
                case 'C' -> {
                    float x1 = number() + (rel ? x : 0f);
                    float y1 = number() + (rel ? y : 0f);
                    float x2 = number() + (rel ? x : 0f);
                    float y2 = number() + (rel ? y : 0f);
                    float ex = number() + (rel ? x : 0f);
                    float ey = number() + (rel ? y : 0f);
                    cubic(x1, y1, x2, y2, ex, ey);
                }
                case 'S' -> {
                    float x1 = x;
                    float y1 = y;
                    if (lastCommand == 'C' || lastCommand == 'S') {
                        x1 = 2 * x - lastCtrlX;
                        y1 = 2 * y - lastCtrlY;
                    }
                    float x2 = number() + (rel ? x : 0f);
                    float y2 = number() + (rel ? y : 0f);
                    float ex = number() + (rel ? x : 0f);
                    float ey = number() + (rel ? y : 0f);
                    cubic(x1, y1, x2, y2, ex, ey);
                    lastCommand = 'S';
                }
                case 'Q' -> {
                    float x1 = number() + (rel ? x : 0f);
                    float y1 = number() + (rel ? y : 0f);
                    float ex = number() + (rel ? x : 0f);
                    float ey = number() + (rel ? y : 0f);
                    quad(x1, y1, ex, ey);
                }
                case 'T' -> {
                    float x1 = x;
                    float y1 = y;
                    if (lastCommand == 'Q' || lastCommand == 'T') {
                        x1 = 2 * x - lastCtrlX;
                        y1 = 2 * y - lastCtrlY;
                    }
                    float ex = number() + (rel ? x : 0f);
                    float ey = number() + (rel ? y : 0f);
                    quad(x1, y1, ex, ey);
                    lastCommand = 'T';
                }
                case 'A' -> {
                    float rx = Math.abs(number());
                    float ry = Math.abs(number());
                    float rotation = number();
                    boolean large = flag();
                    boolean sweep = flag();
                    float ex = number() + (rel ? x : 0f);
                    float ey = number() + (rel ? y : 0f);
                    arc(rx, ry, rotation, large, sweep, ex, ey);
                }
                case 'Z' -> {
                    x = startX;
                    y = startY;
                    closeContour();
                    lastCommand = 'Z';
                    return 0;
                }
                default -> throw new IllegalArgumentException("unknown path command " + command);
            }
            return command;
        }

        private void lineTo(float nx, float ny) {
            ensureStarted();
            x = nx;
            y = ny;
            add(x, y);
            lastCommand = 'L';
        }

        private void cubic(float x1, float y1, float x2, float y2, float ex, float ey) {
            ensureStarted();
            float len = dist(x, y, x1, y1) + dist(x1, y1, x2, y2) + dist(x2, y2, ex, ey);
            int steps = segments(len);
            float sx = x;
            float sy = y;
            for (int k = 1; k <= steps; k++) {
                float t = (float) k / steps;
                float u = 1 - t;
                float px = u * u * u * sx + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t * ex;
                float py = u * u * u * sy + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t * ey;
                add(px, py);
            }
            x = ex;
            y = ey;
            lastCtrlX = x2;
            lastCtrlY = y2;
            lastCommand = 'C';
        }

        private void quad(float x1, float y1, float ex, float ey) {
            ensureStarted();
            float len = dist(x, y, x1, y1) + dist(x1, y1, ex, ey);
            int steps = segments(len);
            float sx = x;
            float sy = y;
            for (int k = 1; k <= steps; k++) {
                float t = (float) k / steps;
                float u = 1 - t;
                add(u * u * sx + 2 * u * t * x1 + t * t * ex, u * u * sy + 2 * u * t * y1 + t * t * ey);
            }
            x = ex;
            y = ey;
            lastCtrlX = x1;
            lastCtrlY = y1;
            lastCommand = 'Q';
        }

        /** SVG's endpoint arc, converted to its centre form (SVG 1.1, appendix F.6.5) and sampled. */
        private void arc(float rx, float ry, float rotationDeg, boolean large, boolean sweep, float ex, float ey) {
            if (rx == 0f || ry == 0f || (ex == x && ey == y)) {
                lineTo(ex, ey);
                return;
            }
            ensureStarted();
            double phi = Math.toRadians(rotationDeg);
            double cos = Math.cos(phi);
            double sin = Math.sin(phi);
            double dx = (x - ex) / 2.0;
            double dy = (y - ey) / 2.0;
            double x1p = cos * dx + sin * dy;
            double y1p = -sin * dx + cos * dy;
            double rxs = rx;
            double rys = ry;
            double lambda = (x1p * x1p) / (rxs * rxs) + (y1p * y1p) / (rys * rys);
            if (lambda > 1) {
                double scale = Math.sqrt(lambda);
                rxs *= scale;
                rys *= scale;
            }
            double num = rxs * rxs * rys * rys - rxs * rxs * y1p * y1p - rys * rys * x1p * x1p;
            double den = rxs * rxs * y1p * y1p + rys * rys * x1p * x1p;
            double coef = den == 0 ? 0 : Math.sqrt(Math.max(0, num / den));
            if (large == sweep) {
                coef = -coef;
            }
            double cxp = coef * rxs * y1p / rys;
            double cyp = -coef * rys * x1p / rxs;
            double cx = cos * cxp - sin * cyp + (x + ex) / 2.0;
            double cy = sin * cxp + cos * cyp + (y + ey) / 2.0;
            double theta1 = angle(1, 0, (x1p - cxp) / rxs, (y1p - cyp) / rys);
            double delta = angle((x1p - cxp) / rxs, (y1p - cyp) / rys, (-x1p - cxp) / rxs, (-y1p - cyp) / rys);
            if (!sweep && delta > 0) {
                delta -= 2 * Math.PI;
            } else if (sweep && delta < 0) {
                delta += 2 * Math.PI;
            }
            int steps = segments((float) (Math.abs(delta) * Math.max(rxs, rys)));
            for (int k = 1; k <= steps; k++) {
                double t = theta1 + delta * k / steps;
                double px = cx + rxs * Math.cos(t) * cos - rys * Math.sin(t) * sin;
                double py = cy + rxs * Math.cos(t) * sin + rys * Math.sin(t) * cos;
                add((float) px, (float) py);
            }
            x = ex;
            y = ey;
            lastCommand = 'A';
        }

        private static double angle(double ux, double uy, double vx, double vy) {
            double dot = ux * vx + uy * vy;
            double len = Math.sqrt((ux * ux + uy * uy) * (vx * vx + vy * vy));
            double a = Math.acos(Math.max(-1, Math.min(1, dot / len)));
            return (ux * vy - uy * vx) < 0 ? -a : a;
        }

        private int segments(float length) {
            // A chord of length L on a radius-r curve strays about L^2 / (8r); keep it under the tolerance.
            int n = (int) Math.ceil(Math.sqrt(length / (2f * tolerance)));
            return Math.max(2, Math.min(64, n));
        }

        private static float dist(float ax, float ay, float bx, float by) {
            float dx = bx - ax;
            float dy = by - ay;
            return (float) Math.sqrt(dx * dx + dy * dy);
        }

        private void add(float px, float py) {
            if (count + 2 > points.length) {
                float[] grown = new float[points.length * 2];
                System.arraycopy(points, 0, grown, 0, count);
                points = grown;
            }
            points[count++] = px;
            points[count++] = py;
        }

        private void closeContour() {
            if (count >= 6) {
                float[] c = new float[count];
                System.arraycopy(points, 0, c, 0, count);
                contours.add(c);
            }
            count = 0;
        }

        /** A drawing command right after Z starts a new outline at the current point. */
        private void ensureStarted() {
            if (count == 0) {
                startX = x;
                startY = y;
                add(x, y);
            }
        }

        private void skipSeparators() {
            while (i < s.length()) {
                char c = s.charAt(i);
                if (c == ' ' || c == ',' || c == '\n' || c == '\r' || c == '\t') {
                    i++;
                } else {
                    break;
                }
            }
        }

        private boolean flag() {
            skipSeparators();
            if (i >= s.length()) {
                throw new IllegalArgumentException("arc flag missing");
            }
            char c = s.charAt(i++);
            if (c != '0' && c != '1') {
                throw new IllegalArgumentException("bad arc flag " + c);
            }
            return c == '1';
        }

        private float number() {
            skipSeparators();
            int start = i;
            if (i < s.length() && (s.charAt(i) == '-' || s.charAt(i) == '+')) {
                i++;
            }
            boolean dot = false;
            boolean digits = false;
            while (i < s.length()) {
                char c = s.charAt(i);
                if (c >= '0' && c <= '9') {
                    digits = true;
                    i++;
                } else if (c == '.' && !dot) {
                    dot = true;
                    i++;
                } else {
                    break;
                }
            }
            if (i < s.length() && (s.charAt(i) == 'e' || s.charAt(i) == 'E')) {
                i++;
                if (i < s.length() && (s.charAt(i) == '-' || s.charAt(i) == '+')) {
                    i++;
                }
                while (i < s.length() && Character.isDigit(s.charAt(i))) {
                    i++;
                }
            }
            if (!digits) {
                throw new IllegalArgumentException("number expected at " + start);
            }
            return Float.parseFloat(s.substring(start, i));
        }
    }
}
