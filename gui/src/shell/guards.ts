/*
 * What a desktop window takes away from the webview it is drawn in. Both
 * windows - the main one and quick search - start with these.
 */

/**
 * The webview's own right-click menu does not belong in a desktop app:
 * "Back", "Reload", "Inspect" are a browser's answers, and "Reload" over the
 * Unlock screen throws away what was being typed. A text field keeps its menu
 * - it is the only way to paste with the mouse.
 */
function noBrowserMenu() {
  document.addEventListener('contextmenu', (event) => {
    const target = event.target
    if (!(target instanceof Element)) return
    if (target.closest('input, textarea')) return
    event.preventDefault()
  })
}

/**
 * Nor do its keys. Reload throws away what was being typed and starts the
 * page over while the vault may still be open behind it; print sends the
 * screen to a spooler, which keeps a copy on disk. Neither is anything this
 * window does.
 */
function noBrowserKeys() {
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
}

export function guard() {
  noBrowserMenu()
  noBrowserKeys()
}
