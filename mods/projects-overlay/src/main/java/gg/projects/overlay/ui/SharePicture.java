package gg.projects.overlay.ui;

import com.mojang.blaze3d.platform.NativeImage;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.voice.VoiceState;
import java.nio.ByteBuffer;
import java.nio.IntBuffer;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import net.minecraft.client.Minecraft;
import net.minecraft.resources.Identifier;
import org.lwjgl.stb.STBImage;
import org.lwjgl.system.MemoryStack;
import org.lwjgl.system.MemoryUtil;

/**
 * The one stream picture a live preview card shows (SHARE-LIVE): the launcher's small JPEG (320 px wide, 24 KB at most)
 * decoded with the game's own STB off the render thread, uploaded on it, and let go as soon as the card closes, so no
 * picture is held while none shows. A newer picture of the same person replaces it once it is decoded (the last one
 * stays meanwhile). Render thread only, apart from the decoder.
 */
final class SharePicture {
    /** A JPEG larger than this is not one the room took (24 KB): not decoded. */
    static final int MAX_JPG_BYTES = 36_000;
    private static final ExecutorService DECODER = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "Projects Overlay previews");
        t.setDaemon(true);
        t.setPriority(Thread.MIN_PRIORITY);
        return t;
    });
    private static int serial;

    private int decodingAt = -1;
    private CompletableFuture<NativeImage> pending;
    private int shownAt = -1;
    private Identifier shown;
    private int width;
    private int height;

    /** The picture for {@code thumb} (the last one while a newer decodes), or null: none yet, none at all. */
    Identifier texture(VoiceState.Thumb thumb) {
        if (thumb != null && thumb.image() && thumb.at() != shownAt && thumb.at() != decodingAt) {
            cancel();
            decodingAt = thumb.at();
            byte[] jpg = thumb.jpg();
            pending = CompletableFuture.supplyAsync(() -> decode(jpg), DECODER);
        }
        if (pending != null && pending.isDone()) {
            NativeImage image = pending.getNow(null);
            int at = decodingAt;
            pending = null;
            decodingAt = -1;
            if (image != null) {
                release();
                Identifier id = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID, "share-preview/" + (++serial));
                width = image.getWidth();
                height = image.getHeight();
                Minecraft.getInstance().getTextureManager()
                    .register(id, Compat.smoothTexture(() -> "Projects Overlay stream preview", image));
                shown = id;
                shownAt = at;
            }
        }
        return shown;
    }

    int width() {
        return width;
    }

    int height() {
        return height;
    }

    /** The card closed: nothing is kept. */
    void release() {
        if (shown != null) {
            Minecraft.getInstance().getTextureManager().release(shown);
            shown = null;
            shownAt = -1;
        }
    }

    /** Lets go of everything, a decode under way too. */
    void close() {
        cancel();
        release();
    }

    private void cancel() {
        if (pending != null) {
            // A picture that is still decoding is freed when it comes.
            pending.thenAccept(image -> {
                if (image != null) {
                    image.close();
                }
            });
            pending = null;
        }
        decodingAt = -1;
    }

    /** The JPEG as an image (straight RGBA from STB), or null when it is no picture. */
    static NativeImage decode(byte[] jpg) {
        if (jpg == null || jpg.length == 0 || jpg.length > MAX_JPG_BYTES) {
            return null;
        }
        ByteBuffer data = MemoryUtil.memAlloc(jpg.length);
        try (MemoryStack stack = MemoryStack.stackPush()) {
            data.put(jpg).flip();
            IntBuffer w = stack.mallocInt(1);
            IntBuffer h = stack.mallocInt(1);
            IntBuffer channels = stack.mallocInt(1);
            ByteBuffer pixels = STBImage.stbi_load_from_memory(data, w, h, channels, 4);
            if (pixels == null) {
                return null;
            }
            try {
                int width = w.get(0);
                int height = h.get(0);
                if (width <= 0 || height <= 0 || width > 640 || height > 640) {
                    return null;
                }
                NativeImage image = new NativeImage(width, height, false);
                for (int y = 0; y < height; y++) {
                    for (int x = 0; x < width; x++) {
                        int i = (y * width + x) * 4;
                        int r = pixels.get(i) & 0xFF;
                        int g = pixels.get(i + 1) & 0xFF;
                        int b = pixels.get(i + 2) & 0xFF;
                        image.setPixel(x, y, 0xFF000000 | (r << 16) | (g << 8) | b);
                    }
                }
                return image;
            } finally {
                STBImage.stbi_image_free(pixels);
            }
        } finally {
            MemoryUtil.memFree(data);
        }
    }
}
