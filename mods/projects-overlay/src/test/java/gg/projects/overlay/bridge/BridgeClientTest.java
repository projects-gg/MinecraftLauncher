package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/** Runs the client against a fake launcher on a loopback port. */
class BridgeClientTest {
    private static final String TOKEN = "t0ken-for-tests_".repeat(3);

    private final BlockingQueue<BridgeClient.Status> statuses = new LinkedBlockingQueue<>();
    private final BlockingQueue<JsonObject> frames = new LinkedBlockingQueue<>();
    private ServerSocket server;
    private BridgeClient client;

    @BeforeEach
    void listen() throws IOException {
        server = new ServerSocket(0, 4, InetAddress.getByAddress(new byte[] {127, 0, 0, 1}));
        server.setSoTimeout(5_000);
        client = new BridgeClient(new BridgeConfig(server.getLocalPort(), TOKEN), "test", "26.1.2", () -> "tr_tr",
            new BridgeClient.Listener() {
                @Override
                public void onStatus(BridgeClient.Status status, String reason) {
                    statuses.add(status);
                }

                @Override
                public void onFrame(JsonObject frame) {
                    frames.add(frame);
                }
            });
    }

    @AfterEach
    void close() throws IOException {
        client.stop();
        server.close();
    }

    /** Accepts the client, checks its hello and returns the connection. */
    private Peer accept() throws IOException {
        Socket s = server.accept();
        s.setSoTimeout(5_000);
        Peer peer = new Peer(s);
        JsonObject hello = Protocol.parse(peer.in.readLine());
        assertNotNull(hello);
        assertEquals("hello", Protocol.type(hello));
        assertEquals(TOKEN, Protocol.str(hello, "token", ""));
        assertEquals(Protocol.VERSION, Protocol.num(hello, "proto", 0));
        return peer;
    }

    private BridgeClient.Status nextStatus() throws InterruptedException {
        return statuses.poll(5, TimeUnit.SECONDS);
    }

    @Test
    void handshakeThenFramesBothWays() throws Exception {
        client.start();
        try (Peer peer = accept()) {
            peer.send("{\"t\":\"welcome\",\"proto\":1}");
            assertEquals(BridgeClient.Status.ONLINE, nextStatus());
            assertEquals("welcome", Protocol.type(frames.poll(5, TimeUnit.SECONDS)));
            peer.send("{\"t\":\"toast\",\"title\":\"Ayşe\"}");
            assertEquals("Ayşe", Protocol.str(frames.poll(5, TimeUnit.SECONDS), "title", ""));
            assertTrue(client.send(Protocol.answer("c1", "coming")));
            JsonObject answer = Protocol.parse(peer.in.readLine());
            assertEquals("answer", Protocol.type(answer));
            assertEquals("coming", Protocol.str(answer, "answer", ""));
        }
    }

    @Test
    void reconnectsAfterTheLauncherDropsTheConnection() throws Exception {
        client.start();
        try (Peer first = accept()) {
            first.send("{\"t\":\"welcome\",\"proto\":1}");
            assertEquals(BridgeClient.Status.ONLINE, nextStatus());
        }
        assertEquals(BridgeClient.Status.OFFLINE, nextStatus());
        long dropped = System.nanoTime();
        try (Peer second = accept()) {
            long waitedMs = (System.nanoTime() - dropped) / 1_000_000;
            assertTrue(waitedMs < 2_500, "reconnect took " + waitedMs + " ms");
            second.send("{\"t\":\"welcome\",\"proto\":1}");
            assertEquals(BridgeClient.Status.ONLINE, nextStatus());
        }
    }

    @Test
    void wrongTokenEndsWithoutRetrying() throws Exception {
        client.start();
        try (Peer peer = accept()) {
            peer.send("{\"t\":\"bye\",\"reason\":\"auth\"}");
            assertEquals(BridgeClient.Status.REJECTED, nextStatus());
        }
        server.setSoTimeout(2_500);
        assertTrue(acceptTimesOut(), "client retried after a rejected token");
        assertEquals(BridgeClient.Status.REJECTED, client.status());
    }

    @Test
    void protocolMismatchIsReported() throws Exception {
        client.start();
        try (Peer peer = accept()) {
            peer.send("{\"t\":\"welcome\",\"proto\":99}");
            assertEquals(BridgeClient.Status.OUTDATED, nextStatus());
        }
    }

    @Test
    void oversizedFrameDropsTheConnection() throws Exception {
        client.start();
        try (Peer peer = accept()) {
            peer.send("{\"t\":\"welcome\",\"proto\":1}");
            assertEquals(BridgeClient.Status.ONLINE, nextStatus());
            try {
                peer.send("{\"t\":\"msg\",\"text\":\"" + "x".repeat(Protocol.MAX_INCOMING_FRAME_BYTES) + "\"}");
            } catch (IOException e) {
                // The client may already have closed the connection; that is the point.
            }
            assertEquals(BridgeClient.Status.OFFLINE, nextStatus());
        }
        frames.removeIf(f -> Protocol.type(f).equals("welcome"));
        assertNull(frames.poll(200, TimeUnit.MILLISECONDS));
    }

    @Test
    void sendFailsWhileOffline() {
        assertTrue(!client.send(Protocol.panel(true)));
    }

    private boolean acceptTimesOut() {
        try (Socket ignored = server.accept()) {
            return false;
        } catch (IOException e) {
            return true;
        }
    }

    private static final class Peer implements AutoCloseable {
        final Socket socket;
        final BufferedReader in;
        final OutputStream out;

        Peer(Socket socket) throws IOException {
            this.socket = socket;
            this.in = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.UTF_8));
            this.out = socket.getOutputStream();
        }

        void send(String line) throws IOException {
            out.write((line + "\n").getBytes(StandardCharsets.UTF_8));
            out.flush();
        }

        @Override
        public void close() throws IOException {
            socket.close();
        }
    }
}
