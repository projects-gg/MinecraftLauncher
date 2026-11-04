package gg.projects.overlay.ui;

import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.NativeImage;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.util.Base64;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.resources.Identifier;

/**
 * People's pictures on their faces (OVL-AVATARS; the owner: "kullanıcının ilk harfi yerine avatarı gözükse daha iyi
 * olur fpsi etkilemeyecekse"). The launcher fetches and prepares each picture once, by its own rule (a bundled
 * picture, the Minecraft head, the Discord photo): 64 px square on the face colour with the player head's corners.
 * Here it is decoded off the render thread, halved twice there (32 and 16 px, so a small face never samples more
 * than two texels a pixel: the whole one would shimmer into jaggies), uploaded once and drawn as one textured quad
 * where the letter was; the letter shows while a picture is on its way and for anyone without one. The mod never
 * goes to the network. Nothing runs while no face shows ({@link AvatarBook} keeps the rest). Render thread only, apart
 * from the decoding.
 */
public final class Avatars {
    /** A picture's pixels, square (its largest level). */
    public static final int SIZE = 64;
    /** The player, for their own face (their row in a voice room); no user id has an {@code @}. */
    public static final String ME = "@me";
    /** The levels kept: 64, 32 and 16 px. */
    private static final int LEVELS = 3;
    /** A picture's base64 at most (a 64 px PNG is a few kilobytes). */
    private static final int MAX_PNG_CHARS = 64 * 1024;
    /** A picture that has just come fades in over the letter this long (opacity only). */
    private static final long FADE_MS = 150;
    /** The asks go out together, one frame every half second at most (the launcher hears 120 a minute). */
    private static final long WANT_GAP_MS = 500;
    /**
     * Pictures uploaded in one frame at most (each is three small textures): a panel full of new faces fades them in
     * over a few frames instead of one long one.
     */
    private static final int UPLOADS_PER_FRAME = 4;
    /** About one frame: a new upload budget each time this has passed. */
    private static final long UPLOAD_WINDOW_MS = 8;
    /** The dev scenario's comparison turns the pictures off and on (the letters everywhere while off). */
    public static boolean shown = true;

    /** A person's picture on the GPU, one texture per level (64, 32, 16 px). */
    private record Uploaded(Identifier[] levels) {
    }

    /** A picture decoded off the render thread, its levels ready to upload. */
    private record Decoded(NativeImage[] levels) {
        void close() {
            for (NativeImage level : levels) {
                level.close();
            }
        }
    }

    private static final AvatarBook<Uploaded, CompletableFuture<Decoded>> BOOK = new AvatarBook<>();
    private static final AvatarBook.Sink<Uploaded, CompletableFuture<Decoded>> SINK = new AvatarBook.Sink<>() {
        @Override
        public void release(Uploaded texture) {
            TextureManager textures = Minecraft.getInstance().getTextureManager();
            for (Identifier level : texture.levels()) {
                textures.release(level);
            }
        }

        @Override
        public void abandon(CompletableFuture<Decoded> pending) {
            // Nobody uploads it now: its pixels are freed once decoded.
            pending.thenAccept(decoded -> {
                if (decoded != null) {
                    decoded.close();
                }
            });
        }
    };
    /** One background thread at most, gone after half a minute with nothing to decode. */
    private static final ExecutorService DECODE = new ThreadPoolExecutor(0, 1, 30, TimeUnit.SECONDS,
        new LinkedBlockingQueue<>(), r -> {
            Thread t = new Thread(r, "Projects Overlay pictures");
            t.setDaemon(true);
            t.setPriority(Thread.NORM_PRIORITY - 1);
            return t;
        });
    private static int serial;
    private static float fade = 1f;
    private static int size = SIZE;
    private static long nextWantAt = Long.MIN_VALUE;
    private static long uploadsUntil = Long.MIN_VALUE;
    private static int uploadsLeft;

    private Avatars() {
    }

    /**
     * {@code id}'s picture ready to draw on a face {@code pixels} screen pixels wide, or null for the letter; the
     * launcher is asked the first time it shows. Its texture is {@link #size()} pixels square, its opacity over the
     * letter {@link #fade()}.
     */
    public static Identifier picture(String id, int pixels) {
        if (!shown || id == null || id.isEmpty()) {
            return null;
        }
        long now = now();
        AvatarBook.Entry<Uploaded, CompletableFuture<Decoded>> e = BOOK.drawn(id, now, SINK);
        if (e.texture == null && !upload(e, now)) {
            return null;
        }
        long since = now - e.readyAt;
        fade = since >= FADE_MS ? 1f : Math.max(0f, since / (float) FADE_MS);
        // The level with at most two texels a screen pixel.
        int level = pixels > SIZE / 2 ? 0 : pixels > SIZE / 4 ? 1 : 2;
        size = SIZE >> level;
        return e.texture.levels()[level];
    }

    /** The pixels of the texture {@link #picture} gave last. */
    public static int size() {
        return size;
    }

    /** The opacity of the picture {@link #picture} gave last: below 1 while it fades in over the letter. */
    public static float fade() {
        return fade;
    }

    /** Starts the decoding, or uploads a finished one; whether the picture is up now. */
    private static boolean upload(AvatarBook.Entry<Uploaded, CompletableFuture<Decoded>> e, long now) {
        if (e.png == null) {
            return false;
        }
        if (e.pending == null) {
            String png = e.png;
            e.pending = CompletableFuture.supplyAsync(() -> decode(png), DECODE).exceptionally(t -> null);
            return false;
        }
        if (!e.pending.isDone()) {
            return false;
        }
        if (now >= uploadsUntil) {
            uploadsUntil = now + UPLOAD_WINDOW_MS;
            uploadsLeft = UPLOADS_PER_FRAME;
        }
        if (uploadsLeft <= 0) {
            // The next frame's turn.
            return false;
        }
        uploadsLeft--;
        Decoded decoded = e.pending.getNow(null);
        e.pending = null;
        if (decoded == null) {
            // No picture after all: the letter.
            e.png = null;
            return false;
        }
        TextureManager textures = Minecraft.getInstance().getTextureManager();
        int n = ++serial;
        Identifier[] levels = new Identifier[LEVELS];
        for (int i = 0; i < LEVELS; i++) {
            levels[i] = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID, "avatars/" + n + "_" + (SIZE >> i));
            textures.register(levels[i], Compat.smoothTexture(() -> "Projects Overlay picture", decoded.levels()[i]));
        }
        e.texture = new Uploaded(levels);
        e.readyAt = now;
        return true;
    }

    /** The launcher's {@code avatar}: someone's picture, or none, for their face. */
    public static void onFrame(JsonObject frame) {
        String id = Protocol.str(frame, "id", "");
        String version = Protocol.str(frame, "v", "");
        if (id.isEmpty() || id.length() > 64 || version.isEmpty() || version.length() > 64) {
            return;
        }
        String png = Protocol.str(frame, "png", null);
        if (png != null && (png.isEmpty() || png.length() > MAX_PNG_CHARS)) {
            png = null;
        }
        BOOK.answered(id, version, Protocol.num(frame, "n", 0L), png, now(), SINK);
    }

    /**
     * Once a tick: the asks go out together (one frame every {@link #WANT_GAP_MS} at most); the pictures nobody drew for
     * a while give their textures back.
     */
    public static void tick(BridgeClient bridge) {
        if (BOOK.isEmpty()) {
            return;
        }
        long now = now();
        if (now >= nextWantAt && bridge.status() == BridgeClient.Status.ONLINE) {
            List<String> wants = BOOK.takeWants(Protocol.MAX_AVATAR_WANT, now);
            if (!wants.isEmpty()) {
                if (bridge.send(Protocol.avatarWant(wants))) {
                    nextWantAt = now + WANT_GAP_MS;
                } else {
                    BOOK.unask(wants);
                }
            }
        }
        BOOK.sweep(now, SINK);
    }

    /** The launcher went away: every face asks again once it is back (at once). */
    public static void offline() {
        BOOK.offline();
        nextWantAt = Long.MIN_VALUE;
    }

    /** The world closed: the textures go back, the pictures stay for the next time. */
    public static void releaseAll() {
        BOOK.releaseTextures(SINK);
    }

    /** People kept now (the dev scenario's results). */
    public static int kept() {
        return BOOK.size();
    }

    /**
     * A picture and its two halvings from its base64 PNG, or null for anything else. Off the render thread; the native
     * images made on the way are freed on any failure.
     */
    private static Decoded decode(String png) {
        byte[] bytes = Base64.getDecoder().decode(png);
        NativeImage[] levels = new NativeImage[LEVELS];
        boolean made = false;
        try {
            NativeImage whole = NativeImage.read(new ByteArrayInputStream(bytes));
            levels[0] = whole;
            if (whole.getWidth() != SIZE || whole.getHeight() != SIZE) {
                return null;
            }
            for (int i = 1; i < LEVELS; i++) {
                NativeImage from = levels[i - 1];
                int side = SIZE >> i;
                NativeImage half = new NativeImage(side, side, false);
                levels[i] = half;
                for (int y = 0; y < side; y++) {
                    for (int x = 0; x < side; x++) {
                        half.setPixel(x, y, AvatarBook.halve(from.getPixel(2 * x, 2 * y),
                            from.getPixel(2 * x + 1, 2 * y), from.getPixel(2 * x, 2 * y + 1),
                            from.getPixel(2 * x + 1, 2 * y + 1)));
                    }
                }
            }
            made = true;
            return new Decoded(levels);
        } catch (IOException e) {
            return null;
        } finally {
            if (!made) {
                for (NativeImage level : levels) {
                    if (level != null) {
                        level.close();
                    }
                }
            }
        }
    }

    private static long now() {
        return System.nanoTime() / 1_000_000L;
    }
}
