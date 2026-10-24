package gg.projects.overlay.host.net;

import gg.projects.overlay.host.GuestIdentity;
import gg.projects.overlay.host.HostGate;
import gg.projects.overlay.host.HostSession;
import gg.projects.overlay.host.PlgHeader;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandlerContext;
import io.netty.handler.codec.ByteToMessageDecoder;
import java.util.List;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;

/**
 * First in the pipeline of every connection to the shared world's port (PROTOCOL.md §7.3): within five seconds it
 * reads the host launcher's PLG1 header, checks its MAC in constant time and its fields, binds the identity to the
 * channel and takes itself out, handing the game's bytes after the header on to the game. Anything else, or nothing,
 * closes the connection without a word. While no session is open (or it closed) every connection is closed.
 */
public final class GateHandler extends ByteToMessageDecoder {
    /** How long a connection may take with its header. */
    public static final long HEADER_WAIT_SECONDS = 5;

    private ScheduledFuture<?> deadline;
    private boolean passed;

    @Override
    public void handlerAdded(ChannelHandlerContext ctx) {
        deadline = ctx.executor().schedule(() -> {
            if (!passed) {
                ctx.close();
            }
        }, HEADER_WAIT_SECONDS, TimeUnit.SECONDS);
    }

    @Override
    protected void handlerRemoved0(ChannelHandlerContext ctx) {
        if (deadline != null) {
            deadline.cancel(false);
        }
    }

    @Override
    protected void decode(ChannelHandlerContext ctx, ByteBuf in, List<Object> out) {
        if (passed) {
            return;
        }
        if (in.readableBytes() < PlgHeader.PREFIX) {
            return;
        }
        byte[] prefix = new byte[PlgHeader.PREFIX];
        in.getBytes(in.readerIndex(), prefix);
        int total = PlgHeader.length(prefix);
        if (total < 0) {
            reject(ctx, in);
            return;
        }
        if (in.readableBytes() < total) {
            return;
        }
        byte[] header = new byte[total];
        in.readBytes(header);
        HostSession session = HostGate.session();
        byte[] key = HostGate.gateKey();
        GuestIdentity identity = session == null || key == null ? null : PlgHeader.verify(key, header);
        HostSession.Guest guest = identity == null ? null : session.admitHeader(identity, ctx.channel());
        if (guest == null) {
            reject(ctx, in);
            return;
        }
        passed = true;
        ctx.channel().attr(HostGate.IDENTITY).set(identity);
        ctx.channel().closeFuture().addListener(f -> HostGate.connectionClosed(session, identity));
        // What came after the header is the game's: the decoder hands it on as it leaves.
        ctx.pipeline().remove(this);
    }

    private static void reject(ChannelHandlerContext ctx, ByteBuf in) {
        in.skipBytes(in.readableBytes());
        ctx.close();
    }

    @Override
    public void exceptionCaught(ChannelHandlerContext ctx, Throwable cause) {
        ctx.close();
    }
}
