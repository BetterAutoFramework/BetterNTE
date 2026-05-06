import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import simpleImportSort from 'eslint-plugin-simple-import-sort'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    plugins: {
      'simple-import-sort': simpleImportSort,
    },
    rules: {
      'simple-import-sort/imports': 'error',
      'simple-import-sort/exports': 'error',
      '@typescript-eslint/no-explicit-any': 'error',
      'no-console': ['error', { allow: ['warn', 'error', 'info'] }],
      // Allow common sync state sync patterns until refactors; React Hooks v7 Compiler rules are noisy here.
      'react-hooks/set-state-in-effect': 'off',
      'react-hooks/preserve-manual-memoization': 'off',
    },
    languageOptions: {
      globals: globals.browser,
    },
  },
])
