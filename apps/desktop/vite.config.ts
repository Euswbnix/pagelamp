/// <reference types="vitest/config" />
import { existsSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";

// Tauri expects a fixed dev port (see src-tauri/tauri.conf.json → build.devUrl).
const DEV_PORT = 1420;

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");

  // Fail the build early when a distribution asks for a brand that does not exist.
  const brand = env.VITE_BRAND || "default";
  if (!existsSync(fileURLToPath(new URL(`./src/brand/brands/${brand}.ts`, import.meta.url)))) {
    throw new Error(`VITE_BRAND="${brand}" but src/brand/brands/${brand}.ts does not exist`);
  }

  return {
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
    },
    // Keep Rust errors visible in the terminal when running under `tauri dev`.
    clearScreen: false,
    server: {
      port: DEV_PORT,
      strictPort: true,
      watch: { ignored: ["**/src-tauri/**"] },
    },
    envPrefix: ["VITE_", "TAURI_ENV_"],
    build: {
      target: "es2022",
      sourcemap: !!process.env.TAURI_ENV_DEBUG,
      // A desktop app loads its bundle from disk, so one ~500 kB chunk is fine.
      chunkSizeWarningLimit: 1500,
    },
    test: {
      environment: "jsdom",
      globals: true,
      setupFiles: ["./src/test/setup.ts"],
      // Tests always run against the in-memory mock API.
      env: { VITE_API: "mock", VITE_TEST_ASYNC_TIMEOUT: process.env.CI ? "3000" : "1000" },
      css: false,
      include: ["src/**/*.test.{ts,tsx}"],
      // Windows CI runners take over 5 s for the larger page tests (a whole route in jsdom,
      // queried by accessible name); locally the defaults still catch a hang quickly.
      testTimeout: process.env.CI ? 15_000 : 5_000,
    },
  };
});
