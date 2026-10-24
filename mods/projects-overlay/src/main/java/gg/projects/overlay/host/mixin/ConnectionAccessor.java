package gg.projects.overlay.host.mixin;

import io.netty.channel.Channel;
import net.minecraft.network.Connection;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The network channel under a game connection: the gate bound the guest's identity to it. */
@Mixin(Connection.class)
public interface ConnectionAccessor {
    @Accessor("channel")
    Channel projects$channel();
}
