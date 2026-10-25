/// <reference types="vitest/config" />
import { availableParallelism } from "node:os";
import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { phosphorWeights } from "./build/phosphor-weights";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [phosphorWeights(), react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // WebView2 (Windows) is evergreen Chromium; WebKit on macOS/Linux needs a safer target.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome120" : "safari16",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
  test: {
    // Vitest's pool gives a worker 60 s to start (fixed, no setting); one worker per thread (15 on
    // a 16-thread PC) starting at once beside cargo builds missed it and two files never ran.
    maxWorkers: Math.max(1, Math.min(4, availableParallelism() - 1)),
  },
});
