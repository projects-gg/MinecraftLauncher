package gg.projects.overlay.host.mixin;

import com.llamalad7.mixinextras.injector.WrapWithCondition;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import gg.projects.overlay.host.HostGate;
import java.net.InetAddress;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.client.server.LanServerPinger;
import net.minecraft.server.MinecraftServer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * 26.3 (as 26.2): our publish opens the world on loopback only, with no LAN announcement and no announcement socket
 * (host1.md §4.3). When the game's own multiplayer control turns sharing off ({@code unpublishServer}) our session
 * ends first, so guests read why in their own language.
 */
@Mixin(IntegratedServer.class)
abstract class IntegratedServerMixin {
    private static final String PUBLISH = "publishServer(Lnet/minecraft/server/MinecraftServer$MultiplayerScope;I)Z";

    @ModifyArg(method = PUBLISH, index = 0, require = 1, at = @At(value = "INVOKE",
        target = "Lnet/minecraft/server/network/ServerConnectionListener;startTcpServerListener(Ljava/net/InetAddress;I)V"))
    private InetAddress projects$loopback(InetAddress address) {
        return HostGate.publishing() ? InetAddress.getLoopbackAddress() : address;
    }

    @WrapOperation(method = PUBLISH, require = 1,
        at = @At(value = "NEW", target = "net/minecraft/client/server/LanServerPinger"))
    private LanServerPinger projects$noPinger(String motd, String address, Operation<LanServerPinger> original) {
        return HostGate.publishing() ? null : original.call(motd, address);
    }

    @WrapWithCondition(method = PUBLISH, require = 1,
        at = @At(value = "INVOKE", target = "Lnet/minecraft/client/server/LanServerPinger;start()V"))
    private boolean projects$noAnnouncement(LanServerPinger pinger) {
        return !HostGate.publishing();
    }

    @Inject(method = "unpublishServer", at = @At("HEAD"), require = 1)
    private void projects$vanillaOff(CallbackInfoReturnable<Boolean> cir) {
        if (!HostGate.unpublishing()) {
            HostGate.vanillaUnpublishing((MinecraftServer) (Object) this);
        }
    }

    /**
     * The player leaves the world (or the game closes): {@code halt} first takes every guest out of the player list
     * without a word, then stops the server. Sharing ends here, before that, so guests read why in their own language.
     */
    @Inject(method = "halt", at = @At("HEAD"), require = 1)
    private void projects$endBeforeHalt(boolean wait, CallbackInfo ci) {
        HostGate.serverStopping((MinecraftServer) (Object) this);
    }
}
