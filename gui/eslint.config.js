import js from '@eslint/js'
import globals from 'globals'
import tseslint from 'typescript-eslint'
import reactHooks from 'eslint-plugin-react-hooks'
// The line's rule: a component names a colour from the dowel vocabulary and
// never writes one down, so the theme can swap it and the accent can move.
import dowel from 'dowel-ui/eslint'

export default tseslint.config(
  // `src-tauri` is the Rust half; `src/ipc/generated` is ts-rs's output
  // (`SEFY_BLESS=1 cargo test -p sefy-gui --test bindings`), not written here.
  { ignores: ['dist', 'src-tauri', 'src/ipc/generated'] },
  js.configs.recommended,
  tseslint.configs.recommended,
  reactHooks.configs.flat['recommended-latest'],
  ...dowel.configs.recommended,
  // The registry's copies are dowel's: what they draw is answered in dowel.
  {
    files: ['src/components/ui/**'],
    rules: { 'dowel/no-arbitrary-scale': 'off' },
  },
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
  },
  // Build tooling runs under Node, not in the webview.
  {
    files: ['*.config.{js,ts}'],
    languageOptions: { globals: globals.node },
  },
)
