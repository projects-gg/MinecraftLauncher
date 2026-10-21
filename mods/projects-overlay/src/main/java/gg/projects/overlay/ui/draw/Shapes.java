package gg.projects.overlay.ui.draw;

import com.mojang.blaze3d.platform.NativeImage;
import gg.projects.overlay.OverlayClient;
import java.io.IOException;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The baked shapes for the current GUI scale, as one texture. Baking runs on a background thread (it is
 * pure pixel math), the upload on the render thread; until a scale's atlas is ready the previous one keeps
 * drawing (or, at start, plain fills do). Render thread only, except the bake itself.
 */
public final class Shapes {
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");

    private static Map<Icon, String> paths;
    private static boolean pathsFailed;
    private static AtlasPacker.Atlas atlas;
    private static Identifier texture;
    private static int scale = -1;
    private static CompletableFuture<AtlasPacker.Atlas> baking;
    private static int bakingScale = -1;

    private Shapes() {
    }

    /** Starts baking for the window's scale if needed and uploads a finished bake. Cheap when up to date. */
    public static void prepare() {
        Minecraft mc = Minecraft.getInstance();
        int k = Math.max(1, mc.getWindow().getGuiScale());
        if (baking != null && baking.isDone()) {
            CompletableFuture<AtlasPacker.Atlas> done = baking;
            int doneScale = bakingScale;
            baking = null;
            bakingScale = -1;
            AtlasPacker.Atlas baked = done.getNow(null);
            if (baked != null) {
                upload(mc, baked, doneScale);
            }
        }
        if (k != scale && baking == null) {
            startBake(k);
        }
    }

    /** Whether shapes for the current scale can be drawn (else callers draw plain fills). */
    public static boolean ready() {
        return atlas != null;
    }

    public static int scale() {
        return scale;
    }

    public static Identifier texture() {
        return texture;
    }

    public static int textureWidth() {
        return atlas.width();
    }

    public static int textureHeight() {
        return atlas.height();
    }

    public static AtlasPacker.Region region(String key) {
        return atlas == null ? null : atlas.regions().get(key);
    }

    private static void startBake(int k) {
        if (paths == null && !pathsFailed) {
            try {
                paths = Icon.loadPaths();
            } catch (IOException | RuntimeException e) {
                pathsFailed = true;
                LOG.warn("Overlay icons unavailable: {}", e.toString());
            }
        }
        Map<Icon, String> icons = paths;
        bakingScale = k;
        baking = CompletableFuture.supplyAsync(() -> {
            long started = System.nanoTime();
            AtlasPacker.Atlas baked = ShapeSet.bake(k, icons);
            LOG.debug("Baked overlay shapes for GUI scale {} in {} ms ({}x{})", k,
                (System.nanoTime() - started) / 1_000_000, baked.width(), baked.height());
            return baked;
        }, Shapes::runOnDaemon).exceptionally(e -> {
            LOG.warn("Could not bake overlay shapes: {}", e.toString());
            return null;
        });
    }

    private static void runOnDaemon(Runnable r) {
        Thread t = new Thread(r, "Projects Overlay shapes");
        t.setDaemon(true);
        t.setPriority(Thread.NORM_PRIORITY - 1);
        t.start();
    }

    private static void upload(Minecraft mc, AtlasPacker.Atlas baked, int k) {
        NativeImage image = new NativeImage(baked.width(), baked.height(), false);
        int[] px = baked.argb();
        for (int y = 0; y < baked.height(); y++) {
            int row = y * baked.width();
            for (int x = 0; x < baked.width(); x++) {
                image.setPixel(x, y, px[row + x]);
            }
        }
        Identifier id = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID, "shapes/" + k);
        DynamicTexture tex = new DynamicTexture(() -> "Projects Overlay shapes x" + k, image);
        mc.getTextureManager().register(id, tex);
        Identifier previous = texture;
        texture = id;
        atlas = baked;
        scale = k;
        if (previous != null && !previous.equals(id)) {
            mc.getTextureManager().release(previous);
        }
    }
}
