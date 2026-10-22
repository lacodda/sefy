import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'
import { workspaceVersion } from './vite.config.ts'

// Moments on screen are drawn in local time, and a test should not read one
// thing on one machine and another elsewhere. Set before any worker starts.
process.env.TZ = 'UTC'

// Two runs over one tree: a `.test.ts` is logic and runs in Node; a
// `.test.tsx` renders, and gets jsdom with a mocked bridge (`src/test/`).
export default defineConfig({
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  define: {
    __APP_VERSION__: JSON.stringify(workspaceVersion()),
  },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'logic',
          include: ['src/**/*.test.ts'],
          environment: 'node',
        },
      },
      {
        extends: true,
        test: {
          name: 'dom',
          include: ['src/**/*.test.tsx'],
          environment: 'jsdom',
          setupFiles: ['src/test/setup.ts'],
        },
      },
    ],
  },
})
