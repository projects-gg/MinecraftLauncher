package gg.projects.overlay.bridge;

import com.google.gson.JsonObject;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.Supplier;

/**
 * Connection to the launcher's bridge on 127.0.0.1. One daemon thread blocks on the socket (no timers, no
 * polling, so it costs nothing while idle) and reconnects with back-off when the launcher goes away. Writes
 * go through a second daemon thread so the render thread never blocks on the socket.
 */
public final class BridgeClient {
    public enum Status {
        /** First attempt, or a retry the player asked for. */
        CONNECTING,
        ONLINE,
        /** Launcher unreachable; retrying in the background. */
        OFFLINE,
        /** The launcher said goodbye (closed or overlay turned off); retries only when asked. */
        ENDED,
        /** Launcher and mod speak different protocol versions. */
        OUTDATED,
        /** The launcher did not accept this session's token. */
        REJECTED
    }

    public interface Listener {
        /** Called on the bridge thread whenever the status changes. {@code reason} may be null. */
        void onStatus(Status status, String reason);

        /** Called on the bridge thread for every launcher frame, starting with {@code welcome}. */
        void onFrame(JsonObject frame);
    }

    private enum Outcome { LOST, FAILED, ENDED, STOPPED }

    private static final int CONNECT_TIMEOUT_MS = 2_000;
    private static final int HANDSHAKE_TIMEOUT_MS = 5_000;
    /** Always IPv4 loopback, the address the launcher binds, whatever the JVM's IPv6 preference. */
    private static final InetAddress LOOPBACK = loopback();

    private final BridgeConfig config;
    private final String modVersion;
    private final String minecraftVersion;
    private final Supplier<String> language;
    /** The panel runs in this game (else the emoji picker alone, CHAT-EMOJI): what the hello says. */
    private final boolean panel;
    private final Listener listener;
    private final Thread thread;
    private final ExecutorService writer;

    private final Object lock = new Object();
    private Socket socket;
    private OutputStream out;
    private boolean wakeRequested;
    private boolean manualRetry;

    private volatile boolean stopped;
    private volatile Status status = Status.CONNECTING;

    public BridgeClient(BridgeConfig config, String modVersion, String minecraftVersion, Supplier<String> language,
        Listener listener) {
        this(config, modVersion, minecraftVersion, language, true, listener);
    }

    public BridgeClient(BridgeConfig config, String modVersion, String minecraftVersion, Supplier<String> language,
        boolean panel, Listener listener) {
        this.config = config;
        this.modVersion = modVersion;
        this.minecraftVersion = minecraftVersion;
        this.language = language;
        this.panel = panel;
        this.listener = listener;
        this.thread = daemon(this::run, "Projects Overlay bridge");
        this.writer = Executors.newSingleThreadExecutor(r -> daemon(r, "Projects Overlay bridge writer"));
    }

    public void start() {
        thread.start();
    }

    public Status status() {
        return status;
    }

    /** Queues a frame for the launcher. Returns false when there is no live connection. */
    public boolean send(JsonObject frame) {
        if (status != Status.ONLINE) {
            return false;
        }
        byte[] bytes = (frame.toString() + "\n").getBytes(StandardCharsets.UTF_8);
        if (bytes.length > Protocol.MAX_OUTGOING_FRAME_BYTES + 1) {
            return false;
        }
        try {
            writer.execute(() -> writeCurrent(bytes));
            return true;
        } catch (RejectedExecutionException e) {
            return false;
        }
    }

    /** Skips the back-off wait (for example when the player opens the panel or presses "try again"). */
    public void retryNow() {
        synchronized (lock) {
            wakeRequested = true;
            manualRetry = true;
            lock.notifyAll();
        }
    }

    public void stop() {
        stopped = true;
        Socket s;
        synchronized (lock) {
            s = socket;
            wakeRequested = true;
            lock.notifyAll();
        }
        closeQuietly(s);
        writer.shutdownNow();
    }

    private void run() {
        int failures = 0;
        while (!stopped) {
            Outcome outcome = session();
            if (stopped || outcome == Outcome.STOPPED) {
                return;
            }
            long waitMs;
            switch (outcome) {
                case LOST -> {
                    failures = 0;
                    setStatus(Status.OFFLINE, "lost");
                    waitMs = Backoff.delayMs(0, jitter());
                }
                case FAILED -> {
                    if (status != Status.OUTDATED) {
                        setStatus(Status.OFFLINE, "unreachable");
                    }
                    waitMs = status == Status.OUTDATED ? Long.MAX_VALUE : Backoff.delayMs(failures++, jitter());
                }
                default -> waitMs = Long.MAX_VALUE;
            }
            if (awaitRetry(waitMs)) {
                failures = 0;
                setStatus(Status.CONNECTING, null);
            }
        }
    }

    /** Runs one connection from connect to close. */
    private Outcome session() {
        Socket s = new Socket();
        boolean online = false;
        try {
            s.setTcpNoDelay(true);
            s.connect(new InetSocketAddress(LOOPBACK, config.port()), CONNECT_TIMEOUT_MS);
            OutputStream o = s.getOutputStream();
            synchronized (lock) {
                if (stopped) {
                    return Outcome.STOPPED;
                }
                socket = s;
                out = o;
            }
            JsonObject hello = Protocol.hello(config.token(), modVersion, minecraftVersion, language.get(), panel);
            o.write((hello + "\n").getBytes(StandardCharsets.UTF_8));
            o.flush();

            s.setSoTimeout(HANDSHAKE_TIMEOUT_MS);
            LineReader reader = new LineReader(s.getInputStream(), Protocol.MAX_INCOMING_FRAME_BYTES);
            JsonObject first = Protocol.parse(reader.readLine());
            if (first == null) {
                return Outcome.FAILED;
            }
            String type = Protocol.type(first);
            if (type.equals("bye")) {
                return goodbye(first);
            }
            if (!type.equals("welcome")) {
                return Outcome.FAILED;
            }
            if (Protocol.num(first, "proto", -1) != Protocol.VERSION) {
                setStatus(Status.OUTDATED, "proto");
                return Outcome.FAILED;
            }
            // From here on the thread blocks in read() without a timeout: nothing runs until a frame arrives.
            s.setSoTimeout(0);
            online = true;
            setStatus(Status.ONLINE, null);
            listener.onFrame(first);
            for (String line; !stopped && (line = reader.readLine()) != null; ) {
                JsonObject frame = Protocol.parse(line);
                if (frame == null) {
                    continue;
                }
                if (Protocol.type(frame).equals("bye")) {
                    return goodbye(frame);
                }
                listener.onFrame(frame);
            }
            return stopped ? Outcome.STOPPED : Outcome.LOST;
        } catch (IOException | RuntimeException e) {
            if (stopped) {
                return Outcome.STOPPED;
            }
            return online ? Outcome.LOST : Outcome.FAILED;
        } finally {
            synchronized (lock) {
                if (socket == s) {
                    socket = null;
                    out = null;
                }
            }
            closeQuietly(s);
        }
    }

    private Outcome goodbye(JsonObject bye) {
        String reason = Protocol.str(bye, "reason", "launcher-exit");
        switch (reason) {
            case "auth" -> setStatus(Status.REJECTED, reason);
            case "proto" -> setStatus(Status.OUTDATED, reason);
            default -> setStatus(Status.ENDED, reason);
        }
        return Outcome.ENDED;
    }

    /** Waits for the delay or a manual retry. Returns true when the player asked for the retry. */
    private boolean awaitRetry(long delayMs) {
        long deadline = delayMs == Long.MAX_VALUE ? Long.MAX_VALUE : System.nanoTime() + delayMs * 1_000_000L;
        synchronized (lock) {
            while (!stopped && !wakeRequested) {
                long leftMs = 0;
                if (deadline != Long.MAX_VALUE) {
                    leftMs = (deadline - System.nanoTime()) / 1_000_000L;
                    if (leftMs <= 0) {
                        break;
                    }
                }
                try {
                    lock.wait(leftMs);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    stopped = true;
                    return false;
                }
            }
            boolean manual = manualRetry;
            wakeRequested = false;
            manualRetry = false;
            return manual;
        }
    }

    private void writeCurrent(byte[] bytes) {
        OutputStream o;
        Socket s;
        synchronized (lock) {
            o = out;
            s = socket;
        }
        if (o == null) {
            return;
        }
        try {
            o.write(bytes);
            o.flush();
        } catch (IOException e) {
            closeQuietly(s);
        }
    }

    private void setStatus(Status next, String reason) {
        if (status == next) {
            return;
        }
        status = next;
        listener.onStatus(next, reason);
    }

    private static InetAddress loopback() {
        try {
            return InetAddress.getByAddress(new byte[] {127, 0, 0, 1});
        } catch (java.net.UnknownHostException e) {
            throw new IllegalStateException(e);
        }
    }

    private static double jitter() {
        return ThreadLocalRandom.current().nextDouble(-1.0, 1.0);
    }

    private static Thread daemon(Runnable r, String name) {
        Thread t = new Thread(r, name);
        t.setDaemon(true);
        return t;
    }

    private static void closeQuietly(Socket s) {
        if (s == null) {
            return;
        }
        try {
            s.close();
        } catch (IOException ignored) {
            // Closing is best effort.
        }
    }
}
