package gg.projects.overlay.host.mixin;

import gg.projects.overlay.host.HostGate;
import gg.projects.overlay.host.net.GateInitializer;
import io.netty.channel.ChannelHandler;
import net.minecraft.server.network.ServerConnectionListener;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;

/**
 * The shared world's listener gets the gate (host1.md §4.4): only the listener our own publish opens, never vanilla's
 * LAN. Every connection to it starts with the host launcher's identity header or closes.
 */
@Mixin(ServerConnectionListener.class)
abstract class ServerConnectionListenerMixin {
    @ModifyArg(method = "startTcpServerListener", require = 1, at = @At(value = "INVOKE",
        target = "Lio/netty/bootstrap/ServerBootstrap;childHandler(Lio/netty/channel/ChannelHandler;)"
            + "Lio/netty/bootstrap/ServerBootstrap;"))
    private ChannelHandler projects$gate(ChannelHandler game) {
        return HostGate.publishing() ? new GateInitializer(game) : game;
    }
}
