// Vite builds the UI into dist/, which the app serves (crates/api embeds it).
//
// While working on the UI, run the app as a web server on a scratch folder and Vite
// next to it; Vite reloads the page as you save and passes /api on to the app:
//   cargo run -p tcgbinder -- --web --data-dir /tmp/tcgbinder-dev
//   npm run dev          (then open http://localhost:5173)
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  // The app serves the files from its own root, on every platform.
  base: "./",
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
  server: {
    port: 5173,
    strictPort: true,
    proxy: { "/api": { target: process.env.TCGBINDER_API || "http://127.0.0.1:8080", changeOrigin: true } },
  },
});
