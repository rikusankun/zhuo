import { Agent } from "node:http";
import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const backendAgent = new Agent({ keepAlive: true });

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), 'ZHUO_');
  const port = process.env.ZHUO_PORT || env.ZHUO_PORT || '1127';
  return {
  plugins: [react(), tailwindcss()],
  server: {
    host: "127.0.0.1",
    port: 3000,
    strictPort: true,
    hmr: {
      port: 3000,
    },
    watch: {
      ignored: ["**/*.db", "**/*.db-wal", "**/*.db-shm", "**/target/**"],
    },
    proxy: {
      // 后端 zhuo-server（axum，SSE 流式对话 + 记忆库）
      "/api": {
        target: `http://127.0.0.1:${port}`,
        changeOrigin: true,
        agent: backendAgent,
      },
    },
  },
  };
});
