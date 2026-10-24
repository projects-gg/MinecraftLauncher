package gg.projects.overlay.host.net;

import gg.projects.overlay.host.HostGate;
import io.netty.channel.Channel;
import io.netty.channel.ChannelHandler;
import io.netty.channel.ChannelInitializer;

/**
 * Wraps the game's own channel initializer for the shared world's listener only: the game sets up its pipeline as
 * always, then the gate goes in front of everything. The connection is marked as ours so the login never treats a
 * connection without an identity as a plain LAN guest.
 */
public final class GateInitializer extends ChannelInitializer<Channel> {
    private final ChannelHandler game;

    public GateInitializer(ChannelHandler game) {
        this.game = game;
    }

    @Override
    protected void initChannel(Channel channel) {
        channel.attr(HostGate.OURS).set(Boolean.TRUE);
        // The game's initializer runs as it is added (the channel is registered) and then takes itself out.
        channel.pipeline().addLast(game);
        channel.pipeline().addFirst("projects_gate", new GateHandler());
    }
}
