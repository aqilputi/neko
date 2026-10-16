import { defineConfig } from 'vitest/config';

export default defineConfig({
  oxc: {
    jsx: { runtime: 'automatic', importSource: 'preact' },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
    coverage: {
      provider: 'v8',
      include: ['src/client/**/*.ts', 'src/state/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/index.ts', 'src/client/types.ts'],
      reporter: ['text', 'html', 'json-summary'],
      thresholds: { lines: 80 },
    },
  },
});
