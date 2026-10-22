import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

/*
 * What the window is allowed to be. Each of these is one line of
 * configuration that a convenience would undo, and each undoing is quiet:
 * the window still opens, and the master password is what pays.
 */

const read = (path: string) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'))
const conf = read('../src-tauri/tauri.conf.json')
const capability = read('../src-tauri/capabilities/default.json')
const window = conf.app.windows[0]

describe('the window', () => {
  it('never offers to remember what is typed into it', () => {
    // WebView2 fills forms from what it saw typed before, and keeps what it
    // saw in a database under the user's profile. The password field says
    // `autocomplete="off"` too; this is the engine's own switch.
    expect(window.generalAutofillEnabled).toBe(false)
  })

  it('draws its own frame, and shows itself once painted', () => {
    expect(window.decorations).toBe(false)
    expect(window.visible).toBe(false)
    // A file dropped on it is how a vault can be opened without the dialog.
    expect(window.dragDropEnabled).toBe(true)
  })

  it('loads nothing from anywhere but itself', () => {
    const csp: string = conf.app.security.csp
    expect(csp).toContain("default-src 'self'")
    // The bridge is the only other place it talks to; no address on the
    // network appears anywhere in the policy.
    const others = csp.match(/https?:\/\/[^\s;]+/g) ?? []
    expect(others.every((origin) => origin === 'http://ipc.localhost')).toBe(true)
    expect(conf.app.withGlobalTauri ?? false).toBe(false)
  })

  it('gives the page no way to the disk, the shell or the network', () => {
    const permissions: string[] = capability.permissions
    for (const permission of permissions) {
      expect(permission.startsWith('core:'), `${permission} reaches past the window`).toBe(true)
    }
  })
})
