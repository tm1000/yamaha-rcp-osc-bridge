import typescriptEslint from "@typescript-eslint/eslint-plugin";
import typescriptParser from "@typescript-eslint/parser";
import prettierPlugin from "eslint-plugin-prettier";
import reactPlugin from "eslint-plugin-react";
import reactHooksPlugin from "eslint-plugin-react-hooks";

export default [
  {
    ignores: ["dist", "build", "node_modules", "src-tauri/**/*"],
    rules: {
      "prettier/prettier": "error",
    },
    plugins: {
      prettier: prettierPlugin,
    },
  },
  {
    files: ["**/*.mjs"],
  },
  {
    files: ["**/*.ts?(x)"],
    languageOptions: {
      parser: typescriptParser,
    },
    plugins: {
      "@typescript-eslint": typescriptEslint,
    },
  },
  {
    files: ["**/*.tsx"],
    ...reactHooksPlugin.configs.flat.recommended,
  },
  {
    files: ["**/*.tsx"],
    ...reactPlugin.configs.flat.recommended,
    settings: {
      react: {
        version: "detect",
      },
    },
  },
  {
    files: ["**/*.tsx"],
    ...reactPlugin.configs.flat["jsx-runtime"],
  },
];
