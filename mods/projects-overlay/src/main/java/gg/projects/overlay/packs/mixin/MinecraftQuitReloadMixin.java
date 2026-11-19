package gg.projects.overlay.packs.mixin;

import java.util.concurrent.CompletableFuture;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * No resource reload once the game is quitting (FT-QUIT). Quitting while on a server with a server resource pack: the
 * game's {@code destroy()} disconnects, dropping the server's pack starts a reload on the worker threads, and right after
 * it {@code close()} frees FreeType ({@code FreeTypeUtil.destroy()}) before it stops those threads. The reload's font step
 * then loads glyphs of this mod's TrueType fonts (the only ones in the pack) from freed memory and the JVM dies in
 * {@code freetype.dll} (a 2.2.5 crash log: {@code FT_Load_Glyph} on {@code Worker-Main}, the game's log "Stopping!",
 * "Reloading ResourceManager", "Stopping worker threads"). A reload after {@link Minecraft#stop()} has nothing to show,
 * so it is answered as done. Optional: should the method be gone in a later version, nothing else changes.
 */
@Mixin(Minecraft.class)
abstract class MinecraftQuitReloadMixin {
    @Shadow
    public abstract boolean isRunning();

    @Inject(
            method = "reloadResourcePacks()Ljava/util/concurrent/CompletableFuture;",
            at = @At("HEAD"),
            cancellable = true,
            require = 0)
    private void projects$noReloadWhileQuitting(CallbackInfoReturnable<CompletableFuture<Void>> info) {
        if (!isRunning()) {
            info.setReturnValue(CompletableFuture.completedFuture(null));
        }
    }
}
