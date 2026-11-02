package gg.projects.overlay.packs.mixin;

import com.google.common.hash.HashCode;
import gg.projects.overlay.packs.SamePack;
import java.net.URL;
import java.util.UUID;
import net.minecraft.client.resources.server.ServerPackManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A server pack pushed again unchanged within one connection is kept instead of reloaded ({@link SamePack},
 * FASTPACK-LEAVE). Optional: should the method be gone in a later version, the game reloads as without the mod.
 */
@Mixin(ServerPackManager.class)
abstract class ServerPackManagerMixin {
    @Inject(method = "pushPack", at = @At("HEAD"), cancellable = true, require = 0)
    private void projects$samePack(UUID id, URL url, HashCode hash, CallbackInfo info) {
        if (SamePack.enabled() && SamePack.answer((ServerPackManager) (Object) this, id, hash)) {
            info.cancel();
        }
    }
}
