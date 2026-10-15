import { defineConfig } from "vite";

export default defineConfig(() => ({
  clearScreen: false,
  server: { port: 1442, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
}));
