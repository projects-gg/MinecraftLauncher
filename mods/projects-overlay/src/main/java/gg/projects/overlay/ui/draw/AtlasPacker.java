package gg.projects.overlay.ui.draw;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Packs baked images into one texture: shelves, tallest first, into the smallest power-of-two width that
 * keeps the texture roughly square. One texture means one bind for every shape the overlay draws. Pure.
 */
public final class AtlasPacker {
    /** Where an image landed, in texels. */
    public record Region(int x, int y, int width, int height) {
    }

    /** A packed texture. */
    public record Atlas(int width, int height, int[] argb, Map<String, Region> regions) {
    }

    private static final int PAD = 1;

    private final Map<String, ShapeBaker.Image> images = new LinkedHashMap<>();

    public AtlasPacker add(String key, ShapeBaker.Image image) {
        images.put(key, image);
        return this;
    }

    public Atlas pack() {
        List<Map.Entry<String, ShapeBaker.Image>> order = new ArrayList<>(images.entrySet());
        order.sort(Comparator.comparingInt((Map.Entry<String, ShapeBaker.Image> e) -> e.getValue().height())
            .reversed());
        long area = 0;
        int widest = 1;
        for (ShapeBaker.Image image : images.values()) {
            area += (long) (image.width() + PAD) * (image.height() + PAD);
            widest = Math.max(widest, image.width() + PAD);
        }
        int width = 64;
        while (width < widest || (long) width * width < area) {
            width *= 2;
        }
        Map<String, Region> regions = new LinkedHashMap<>();
        int x = 0;
        int y = 0;
        int shelf = 0;
        for (Map.Entry<String, ShapeBaker.Image> e : order) {
            ShapeBaker.Image image = e.getValue();
            if (x + image.width() > width) {
                x = 0;
                y += shelf + PAD;
                shelf = 0;
            }
            regions.put(e.getKey(), new Region(x, y, image.width(), image.height()));
            x += image.width() + PAD;
            shelf = Math.max(shelf, image.height());
        }
        int height = 1;
        while (height < y + shelf) {
            height *= 2;
        }
        int[] argb = new int[width * height];
        for (Map.Entry<String, ShapeBaker.Image> e : order) {
            Region r = regions.get(e.getKey());
            ShapeBaker.Image image = e.getValue();
            for (int row = 0; row < image.height(); row++) {
                System.arraycopy(image.argb(), row * image.width(), argb, (r.y() + row) * width + r.x(),
                    image.width());
            }
        }
        return new Atlas(width, height, argb, regions);
    }
}
