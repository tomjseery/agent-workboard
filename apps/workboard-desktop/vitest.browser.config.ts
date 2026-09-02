import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [tailwindcss(), react()],
  optimizeDeps: { include: ["react-dom/client"] },
  test: {
    include: ["src/**/*.browser.test.tsx"],
    browser: {
      enabled: true,
      provider: "playwright",
      instances: [{ browser: "chromium" }],
      api: { host: "127.0.0.1", port: 1421 },
    },
  },
});
