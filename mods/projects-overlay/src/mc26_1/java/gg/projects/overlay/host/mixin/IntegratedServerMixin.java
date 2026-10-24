package gg.projects.overlay.host.mixin;

import com.llamalad7.mixinextras.injector.WrapWithCondition;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import gg.projects.overlay.host.HostGate;
import java.net.InetAddress;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.client.server.LanServerPinger;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.GameType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * 26.1.2: our publish opens the world on loopback only, with no LAN announcement and no announcement socket (host1.md
 * §4.3). This version publishes again on top of a published world, so while our listener stands nobody else
 * publishes next to it.
 */
@Mixin(IntegratedServer.class)
abstract class IntegratedServerMixin {
    @Inject(method = "publishServer", at = @At("HEAD"), cancellable = true, require = 1)
    private void projects$onlyOnce(GameType mode, boolean commands, int port, CallbackInfoReturnable<Boolean> cir) {
        if (!HostGate.publishing() && HostGate.publishedBy((MinecraftServer) (Object) this)) {
            cir.setReturnValue(false);
        }
    }

    @ModifyArg(method = "publishServer", index = 0, require = 1, at = @At(value = "INVOKE",
        target = "Lnet/minecraft/server/network/ServerConnectionListener;startTcpServerListener(Ljava/net/InetAddress;I)V"))
    private InetAddress projects$loopback(InetAddress address) {
        return HostGate.publishing() ? InetAddress.getLoopbackAddress() : address;
    }

    @WrapOperation(method = "publishServer", require = 1,
        at = @At(value = "NEW", target = "net/minecraft/client/server/LanServerPinger"))
    private LanServerPinger projects$noPinger(String motd, String address, Operation<LanServerPinger> original) {
        return HostGate.publishing() ? null : original.call(motd, address);
    }

    @WrapWithCondition(method = "publishServer", require = 1,
        at = @At(value = "INVOKE", target = "Lnet/minecraft/client/server/LanServerPinger;start()V"))
    private boolean projects$noAnnouncement(LanServerPinger pinger) {
        return !HostGate.publishing();
    }
}
