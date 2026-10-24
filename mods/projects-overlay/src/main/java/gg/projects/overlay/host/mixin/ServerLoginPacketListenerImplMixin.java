package gg.projects.overlay.host.mixin;

import com.mojang.authlib.GameProfile;
import gg.projects.overlay.host.HostGate;
import io.netty.channel.Channel;
import net.minecraft.core.UUIDUtil;
import net.minecraft.network.Connection;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.login.ServerboundHelloPacket;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.network.ServerLoginPacketListenerImpl;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The login of a remote connection to the world (PROTOCOL.md §7.3). Guests of a shared world log in only as their
 * launcher vouched: the header's name, never the host's, bound to their friend, Mojang's check for a Microsoft account
 * and the offline profile for a username account; the room and removals are checked once more as the login ends.
 * Any other remote connection (vanilla LAN) keeps vanilla's way, except that the host's own name never logs in from
 * outside (vanilla would hand it the host's profile without a check). The host's own game is a memory connection and
 * is never looked at.
 */
@Mixin(ServerLoginPacketListenerImpl.class)
abstract class ServerLoginPacketListenerImplMixin {
    @Shadow
    @Final
    private Connection connection;

    @Shadow
    @Final
    private MinecraftServer server;

    @Shadow
    private String requestedUsername;

    @Shadow
    private void startClientVerification(GameProfile profile) {
        throw new AssertionError();
    }

    @Shadow
    public abstract void disconnect(Component reason);

    /** After the game's own checks of the hello, before it looks at the host's name. */
    @Inject(method = "handleHello", cancellable = true, require = 1, at = @At(value = "INVOKE",
        target = "Lnet/minecraft/server/MinecraftServer;getSingleplayerProfile()Lcom/mojang/authlib/GameProfile;"))
    private void projects$hello(ServerboundHelloPacket packet, CallbackInfo ci) {
        if (connection.isMemoryConnection()) {
            return;
        }
        String[] text = new String[1];
        switch (HostGate.hello(server, channel(), requestedUsername, text)) {
            case VANILLA -> {
            }
            case OFFLINE -> {
                startClientVerification(UUIDUtil.createOfflineProfile(requestedUsername));
                ci.cancel();
            }
            case REFUSE -> {
                disconnect(Component.literal(text[0]));
                ci.cancel();
            }
        }
    }

    /** The profile the login ended with (Mojang's for a Microsoft account): the last check, with the room counted. */
    @Inject(method = "verifyLoginAndFinishConnectionSetup", at = @At("HEAD"), cancellable = true, require = 1)
    private void projects$admit(GameProfile profile, CallbackInfo ci) {
        if (connection.isMemoryConnection()) {
            return;
        }
        String text = HostGate.verify(server, channel(), profile);
        if (text != null) {
            disconnect(Component.literal(text));
            ci.cancel();
        }
    }

    /** Mojang did not vouch for a guest's Microsoft account: the launcher hears it. */
    @Inject(method = "disconnect", at = @At("HEAD"), require = 1)
    private void projects$disconnected(Component reason, CallbackInfo ci) {
        if (!connection.isMemoryConnection()) {
            HostGate.loginDisconnect(channel(), reason);
        }
    }

    private Channel channel() {
        return ((ConnectionAccessor) connection).projects$channel();
    }
}
