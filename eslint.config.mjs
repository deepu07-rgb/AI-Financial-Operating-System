import eslint from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "artifacts/**",
      "coverage/**",
      "**/dist/**",
      "node_modules/**",
      "PRD and Guideline/**",
      "**/target/**",
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["apps/desktop/src/**/*.{ts,tsx}"],
    plugins: reactHooks.configs.flat["recommended-latest"].plugins,
    rules: reactHooks.configs.flat["recommended-latest"].rules,
  },
  {
    files: ["**/*.mjs"],
    languageOptions: {
      globals: {
        Buffer: "readonly",
        console: "readonly",
        process: "readonly",
      },
    },
  },
);
