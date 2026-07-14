import { defineConfig, configDefaults } from "vitest/config";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  publicDir: "assets",
  clearScreen: false,
  // Desktop app: the bundle loads from local disk, so the web-oriented 500 kB
  // chunk warning doesn't apply. Raise it to keep build output clean.
  build: { chunkSizeWarningLimit: 1500 },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    // Never run the frozen backup snapshot or the Rust crate as JS tests.
    exclude: [...configDefaults.exclude, "backups/**", "src-tauri/**"],
  },
});
