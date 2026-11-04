import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import path from "node:path";

/**
 * The renderer is built into `dist/client` and embedded into the Tauri binary.
 * `tauri dev` starts Vite on a fixed port so `devUrl` in tauri.conf.json works.
 */
export default defineConfig({
  root: path.resolve(__dirname, "src/client"),
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  build: {
    outDir: path.resolve(__dirname, "dist/client"),
    emptyOutDir: true,
    target: "chrome110",
    rollupOptions: {
      input: [
        path.resolve(__dirname, "src/client/index.html"),
        path.resolve(__dirname, "src/client/widget.html"),
      ],
    },
  },
  server: {
    port: 5173,
    strictPort: true,
  },
});
