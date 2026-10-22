// A static page for the emoji and sticker picker (CHAT2-L), built with the app's own config:
//   TAURI_ENV_PLATFORM=windows pnpm exec vite build --config src/features/emoji/preview/vite.config.ts --outDir <dir>
//   pnpm exec vite preview --config src/features/emoji/preview/vite.config.ts --outDir <dir> --port <port>
// then open http://localhost:<port>/src/features/emoji/preview/index.html (switches in main.tsx).
import { fileURLToPath, URL } from "node:url";
import { defineConfig, mergeConfig } from "vite";
import app from "../../../../vite.config";

export default mergeConfig(
  app,
  defineConfig({
    root: fileURLToPath(new URL("../../../../", import.meta.url)),
    build: {
      rollupOptions: {
        input: fileURLToPath(new URL("./index.html", import.meta.url)),
      },
    },
  }),
);
