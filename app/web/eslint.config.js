import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "node_modules"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.ts"],
    languageOptions: {
      globals: { ...globals.browser },
    },
    rules: {
      // 骨架阶段：live2d/stage 对第三方类型按 any 处理，P0 实装时收紧。
      "@typescript-eslint/no-explicit-any": "off",
      // stub 参数允许 _ 前缀占位。
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    files: ["**/*.js"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
);
