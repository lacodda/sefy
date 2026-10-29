import React from 'react'
import ReactDOM from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { initTheme } from 'dowel-ui'
import App from '@/App'
import { guard } from '@/shell/guards'
import '@/styles.css'

// The theme follows the system; a choice made elsewhere in the line on this
// machine is applied before the first paint rather than a moment after it.
initTheme()

// The window is created hidden (tauri.conf.json) and shown from here, once
// the document has painted its splash in the right colours. Shown before it
// had, the webview flashes white for as long as the bundle takes to arrive.
// Outside Tauri (a test, a browser tab) there is no window to show.
if ('__TAURI_INTERNALS__' in window) {
  getCurrentWindow()
    .show()
    .catch(() => undefined)
}

// No browser menu, no reload, no print: see `shell/guards.ts`.
guard()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
