import { configDefaults, defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    testTimeout: 15000,
    // Saved design previews are separate projects with their own mock behavior.
    exclude: [...configDefaults.exclude, '.agents/references/**'],
  },
});
