import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/platform/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            { name: "@tauri-apps/api", message: "Use src/platform" },
            { name: "@tauri-apps/api/core", message: "Use src/platform" },
            { name: "electron", message: "Use src/platform" },
          ],
          patterns: [
            { group: ["@tauri-apps/api/*"], message: "Use src/platform" },
            { group: ["@tauri-apps/plugin-*"], message: "Use src/platform" },
          ],
        },
      ],
    },
  },
);
