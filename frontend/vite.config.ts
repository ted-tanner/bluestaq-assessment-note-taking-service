import adapter from "@sveltejs/adapter-auto";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

export default defineConfig({
  // Discover the lazy editor dependencies before the first note opens, avoiding
  // a development optimizer reload that can interrupt the module download.
  optimizeDeps: {
    include: [
      "@tiptap/core",
      "@tiptap/extension-list",
      "@tiptap/extension-table",
      "@tiptap/markdown",
      "@tiptap/starter-kit",
    ],
  },
  server: {
    // Browser tests must not reset in-memory drafts when another task edits source files.
    hmr: process.env.E2E_DISABLE_HMR === "true" ? false : undefined,
    proxy: {
      "/api": {
        target: process.env.API_PROXY_TARGET || "http://127.0.0.1:8080",
        changeOrigin: false,
      },
    },
  },
  plugins: [
    sveltekit({
      compilerOptions: {
        // Force runes mode for the project, except for libraries. Can be removed in svelte 6.
        runes: ({ filename }) =>
          filename.split(/[/\\]/).includes("node_modules") ? undefined : true,
      },

      // adapter-auto only supports some environments, see https://svelte.dev/docs/kit/adapter-auto for a list.
      // If your environment is not supported, or you settled on a specific environment, switch out the adapter.
      // See https://svelte.dev/docs/kit/adapters for more information about adapters.
      adapter: adapter(),
      csp: {
        mode: "auto",
        directives: {
          "default-src": ["self"],
          "script-src": ["self"],
          "style-src": ["self", "unsafe-inline"],
          "img-src": ["self", "data:"],
          "font-src": ["self"],
          "connect-src": ["self"],
          "object-src": ["none"],
          "base-uri": ["self"],
          "frame-ancestors": ["none"],
          "form-action": ["self"],
        },
      },
    }),
  ],
});
