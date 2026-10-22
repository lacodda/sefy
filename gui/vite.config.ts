import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

/**
 * The version the window prints, read from the workspace manifest.
 *
 * The crates, the CLI and the window are one release with one number, and it
 * is written in one place: `[workspace.package]` in the root Cargo.toml. A
 * version in this package.json would be a second copy for the release to
 * forget.
 */
export function workspaceVersion(): string {
  const manifest = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8')
  const section = manifest.split('[workspace.package]')[1] ?? ''
  const version = /^version\s*=\s*"([^"]+)"/m.exec(section)?.[1]
  if (version === undefined) throw new Error('no version under [workspace.package] in Cargo.toml')
  return version
}

const version = workspaceVersion()

// Tauri serves the frontend from a fixed port and expects a static build in dist/.
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // The splash in index.html prints the version before the bundle - and
    // `define` below - has loaded, so the number is written into the page
    // itself at build time.
    {
      name: 'sefy:splash-version',
      transformIndexHtml: (html) => html.replace('__APP_VERSION__', version),
    },
  ],
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  clearScreen: false,
  server: {
    port: 1427,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
