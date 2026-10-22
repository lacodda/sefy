import React from 'react'
import ReactDOM from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { initTheme } from 'dowel-ui'
import App from '@/App'
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

/*
 * The webview's own right-click menu does not belong in a desktop app: "Back",
 * "Reload", "Inspect" are a browser's answers, and "Reload" over the Unlock
 * screen throws away what was being typed. A text field keeps its menu - it is
 * the only way to paste with the mouse.
 */
document.addEventListener('contextmenu', (event) => {
  const target = event.target
  if (!(target instanceof Element)) return
  if (target.closest('input, textarea')) return
  event.preventDefault()
})

/*
 * Nor do its keys. Reload throws away what was being typed and starts the
 * page over while the vault may still be open behind it; print sends the
 * screen to a spooler, which keeps a copy on disk. Neither is anything this
 * window does.
 */
document.addEventListener(
  'keydown',
  (event) => {
    // By the key's place, not its letter: on a Russian layout Ctrl+R is
    // Ctrl+к, and the browser still reloads.
    const command = event.ctrlKey || event.metaKey
    const reload = event.code === 'F5' || (command && event.code === 'KeyR')
    const print = command && event.code === 'KeyP'
    if (reload || print) event.preventDefault()
  },
  { capture: true },
)

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
