import { defineConfig } from "vite";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  root: "src",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "chrome105",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  // Vitest in-source testing
  define: {
    "import.meta.vitest": "undefined",
  },
  test: {
    root: ".",
    includeSource: ["src/**/*.ts"],
    environment: "node",
  },
}));
