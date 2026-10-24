package gg.projects.overlay.link;

import io.netty.buffer.ByteBuf;
import java.util.function.Consumer;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

/**
 * One {@code projects:link} message: a single UTF-8 string, VarInt length first, holding JSON
 * ({@link LinkMessages}). The same channel both ways, as ProBotLinker's {@code Messenger} uses it.
 */
public record LinkPayload(String json) implements CustomPacketPayload {
    public static final Type<LinkPayload> TYPE = new Type<>(Identifier.fromNamespaceAndPath("projects", "link"));
    /** ProBotLinker's limit is 1 KB of UTF-8; a longer string is not read at all. */
    private static final StreamCodec<ByteBuf, LinkPayload> CODEC =
        ByteBufCodecs.stringUtf8(LinkMessages.MAX_BYTES).map(LinkPayload::new, LinkPayload::json);

    @Override
    public Type<LinkPayload> type() {
        return TYPE;
    }

    /** The message never goes to a log (it may carry the permit or the question's token). */
    @Override
    public String toString() {
        return "LinkPayload[" + json.length() + " chars]";
    }

    /**
     * Registers the channel both ways and hears the server's messages on the game's thread. Registering the
     * receiver tells every server the game listens on {@code projects:link}; the mod still answers only Projects.
     */
    public static void register(Consumer<String> fromServer) {
        PayloadTypeRegistry.clientboundPlay().register(TYPE, CODEC);
        PayloadTypeRegistry.serverboundPlay().register(TYPE, CODEC);
        ClientPlayNetworking.registerGlobalReceiver(TYPE,
            (payload, context) -> context.client().execute(() -> fromServer.accept(payload.json())));
    }

    /** Sends a message to the server the game is on. */
    public static void send(String json) {
        ClientPlayNetworking.send(new LinkPayload(json));
    }
}
