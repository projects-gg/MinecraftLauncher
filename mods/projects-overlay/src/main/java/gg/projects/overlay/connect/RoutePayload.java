package gg.projects.overlay.connect;

import io.netty.buffer.ByteBuf;
import java.util.function.Consumer;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

/**
 * One {@code projects:route} message from the lobby (AUTOJOIN, {@link Route}, {@link RouteMessages}). Lobby to game
 * only; listened to only while a route waits, since listening is what tells the lobby this player will switch.
 */
public record RoutePayload(String json) implements CustomPacketPayload {
    public static final Type<RoutePayload> TYPE = new Type<>(Identifier.fromNamespaceAndPath("projects", "route"));
    private static final StreamCodec<ByteBuf, RoutePayload> CODEC = StreamCodec.of(RoutePayload::write, RoutePayload::read);

    @Override
    public Type<RoutePayload> type() {
        return TYPE;
    }

    /** The message never goes to a log whole. */
    @Override
    public String toString() {
        return "RoutePayload[" + json.length() + " chars]";
    }

    private static RoutePayload read(ByteBuf buf) {
        // Everything is read whatever it holds: bytes left over would end the connection.
        byte[] all = new byte[buf.readableBytes()];
        buf.readBytes(all);
        return new RoutePayload(RouteMessages.text(all));
    }

    private static void write(ByteBuf buf, RoutePayload payload) {
        buf.writeBytes(RouteMessages.bytes(payload.json()));
    }

    /** Registers the message's type: at the start, in every mode (receiving it needs {@link #listen}). */
    public static void registerType() {
        PayloadTypeRegistry.clientboundPlay().register(TYPE, CODEC);
    }

    /**
     * Starts listening, on the game's thread. From then on every server the game joins hears that it takes
     * {@code projects:route}: only while a route waits.
     */
    public static void listen(Consumer<String> fromServer) {
        ClientPlayNetworking.registerGlobalReceiver(TYPE,
            (payload, context) -> context.client().execute(() -> fromServer.accept(payload.json())));
    }

    /** Stops listening once the route is done. */
    public static void stopListening() {
        ClientPlayNetworking.unregisterGlobalReceiver(TYPE.id());
    }
}
