package gg.projects.overlay.stickers;

import com.mojang.blaze3d.platform.NativeImage;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.ui.draw.Draw;
import java.io.IOException;
import java.io.InputStream;
import java.lang.ref.SoftReference;
import java.util.concurrent.CompletableFuture;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The stickers' pictures (CHAT2 §8.3): the set's atlas from the jar, baked at the size each place draws them
 * (the message 48 GUI pixels, the picker's cell, the toast's 16) for the current GUI scale, one texture per size.
 * Nothing is loaded until a sticker is first drawn; decoding and baking run on a background thread, only the
 * upload on the render thread, and until a size is ready its place shows a quiet tile. A size not drawn for a
 * minute (the panel closed, the toast gone) is released: no memory held while nothing shows. Render thread
 * only, apart from the bake.
 */
public final class StickerArt {
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    private static final String ATLAS = "/assets/projects_overlay/stickers/pl1.png";
    /** A size is released this long after it was last drawn. */
    private static final long KEEP_MS = 60_000;
    /** Sizes kept at once (message, picker, toast, and one during a scale change). */
    private static final int SLOTS = 4;

    /** One size's texture, or its bake on the way. */
    private static final class Slot {
        final int px;
        CompletableFuture<Baked> baking;
        Identifier texture;
        int width;
        int height;
        long lastDrawn;

        Slot(int px) {
            this.px = px;
        }
    }

    /** A finished bake: the sheet's layout and its picture, ready to upload. */
    private record Baked(StickerBaker.Sheet sheet, NativeImage image) {
    }

    private static final Slot[] SLOT = new Slot[SLOTS];
    private static SoftReference<int[]> source = new SoftReference<>(null);
    private static int sourceW;
    private static boolean failed;
    private static int serial;

    private StickerArt() {
    }

    static long now() {
        return System.nanoTime() / 1_000_000L;
    }

    /** Screen pixels a sticker of {@code guiSize} GUI pixels is baked at: one texel a pixel, at most the tile. */
    public static int pixels(int guiSize) {
        int tile = Math.max(1, StickerSet.get().tile());
        return Math.max(1, Math.min(tile, guiSize * Draw.scale()));
    }

    /** Starts baking the size {@code guiSize} if needed (before it is first drawn: the picker's button hovered). */
    public static void prepare(int guiSize) {
        if (!failed && !StickerSet.get().isEmpty()) {
            Slot slot = slot(pixels(guiSize));
            if (slot != null) {
                slot.lastDrawn = now();
                ready(slot);
            }
        }
    }

    /**
     * Draws {@code sticker} at {@code guiSize} GUI pixels, its top-left corner at {@code x, y} (with the current
     * transform and opacity). Returns false while its size is not ready yet; the caller draws a quiet tile.
     */
    public static boolean draw(GuiGraphicsExtractor g, StickerSet.Sticker sticker, int guiSize, float x, float y) {
        if (sticker == null || failed) {
            return false;
        }
        Slot slot = slot(pixels(guiSize));
        if (slot == null) {
            return false;
        }
        slot.lastDrawn = now();
        if (!ready(slot)) {
            return false;
        }
        int u = sticker.column() * (slot.px + 2) + 1;
        int v = sticker.row() * (slot.px + 2) + 1;
        Draw.texture(g, slot.texture, x, y, guiSize, guiSize, u, v, slot.px, slot.px, slot.width, slot.height,
            0xFFFFFFFF);
        return true;
    }

    /** Releases the sizes nobody drew for a while. Called once a tick: no allocation. */
    public static void tick(long now) {
        for (int i = 0; i < SLOTS; i++) {
            Slot slot = SLOT[i];
            if (slot != null && slot.baking == null && now - slot.lastDrawn > KEEP_MS) {
                free(slot);
                SLOT[i] = null;
            }
        }
    }

    /** Releases every size (the world closed). */
    public static void releaseAll() {
        for (int i = 0; i < SLOTS; i++) {
            Slot slot = SLOT[i];
            if (slot != null && slot.baking == null) {
                free(slot);
                SLOT[i] = null;
            }
        }
    }

    /** The slot of {@code px}, made when there is room (the oldest unused one gives way). */
    private static Slot slot(int px) {
        int empty = -1;
        int oldest = -1;
        for (int i = 0; i < SLOTS; i++) {
            Slot slot = SLOT[i];
            if (slot == null) {
                empty = empty < 0 ? i : empty;
            } else if (slot.px == px) {
                return slot;
            } else if (slot.baking == null && (oldest < 0 || slot.lastDrawn < SLOT[oldest].lastDrawn)) {
                oldest = i;
            }
        }
        int at = empty >= 0 ? empty : oldest;
        if (at < 0) {
            return null;
        }
        if (SLOT[at] != null) {
            free(SLOT[at]);
        }
        Slot made = new Slot(px);
        SLOT[at] = made;
        return made;
    }

    /** Whether the slot's texture is up; starts its bake or uploads a finished one. */
    private static boolean ready(Slot slot) {
        if (slot.texture != null) {
            return true;
        }
        if (slot.baking == null) {
            int px = slot.px;
            slot.baking = CompletableFuture.supplyAsync(() -> bake(px), StickerArt::runOnDaemon)
                .exceptionally(e -> {
                    LOG.warn("Could not bake the overlay's stickers: {}", e.toString());
                    return null;
                });
            return false;
        }
        if (!slot.baking.isDone()) {
            return false;
        }
        Baked baked = slot.baking.getNow(null);
        slot.baking = null;
        if (baked == null) {
            failed = true;
            return false;
        }
        Identifier id = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID, "stickers/" + baked.sheet().px()
            + "_" + (++serial));
        int px = baked.sheet().px();
        DynamicTexture texture = Compat.smoothTexture(() -> "Projects Overlay stickers " + px, baked.image());
        Minecraft.getInstance().getTextureManager().register(id, texture);
        slot.texture = id;
        slot.width = baked.sheet().width();
        slot.height = baked.sheet().height();
        return true;
    }

    private static void free(Slot slot) {
        if (slot.texture != null) {
            Minecraft.getInstance().getTextureManager().release(slot.texture);
            slot.texture = null;
        }
    }

    /** Decodes the atlas (once while memory allows) and bakes one size into a picture ready to upload. */
    private static Baked bake(int px) {
        long started = System.nanoTime();
        StickerSet set = StickerSet.get();
        int[] atlas = source.get();
        if (atlas == null) {
            atlas = decode();
            source = new SoftReference<>(atlas);
        }
        StickerBaker.Sheet sheet = StickerBaker.bake(atlas, sourceW, set.tile(), set.columns(), set.rows(), px);
        NativeImage image = new NativeImage(sheet.width(), sheet.height(), false);
        int[] argb = sheet.argb();
        for (int y = 0; y < sheet.height(); y++) {
            int row = y * sheet.width();
            for (int x = 0; x < sheet.width(); x++) {
                image.setPixel(x, y, argb[row + x]);
            }
        }
        LOG.debug("Baked the overlay's stickers at {} px in {} ms ({}x{})", px,
            (System.nanoTime() - started) / 1_000_000, sheet.width(), sheet.height());
        return new Baked(new StickerBaker.Sheet(sheet.px(), sheet.columns(), sheet.rows(), sheet.width(),
            sheet.height(), new int[0]), image);
    }

    private static int[] decode() {
        try (InputStream in = StickerArt.class.getResourceAsStream(ATLAS)) {
            if (in == null) {
                throw new IllegalStateException("missing " + ATLAS);
            }
            try (NativeImage image = NativeImage.read(in)) {
                int w = image.getWidth();
                int h = image.getHeight();
                int[] out = new int[w * h];
                for (int y = 0; y < h; y++) {
                    for (int x = 0; x < w; x++) {
                        out[y * w + x] = image.getPixel(x, y);
                    }
                }
                sourceW = w;
                return out;
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void runOnDaemon(Runnable r) {
        Thread t = new Thread(r, "Projects Overlay stickers");
        t.setDaemon(true);
        t.setPriority(Thread.NORM_PRIORITY - 1);
        t.start();
    }
}
