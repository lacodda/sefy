import { readdirSync, readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

/*
 * What the window is allowed to be. Each of these is one line of
 * configuration that a convenience would undo, and each undoing is quiet:
 * the window still opens, and the master password is what pays.
 */

const read = (path: string) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'))
const conf = read('../src-tauri/tauri.conf.json')
const capabilities: { identifier: string; windows: string[]; permissions: string[] }[] = readdirSync(
  new URL('../src-tauri/capabilities/', import.meta.url),
)
  .filter((name) => name.endsWith('.json'))
  .map((name) => read(`../src-tauri/capabilities/${name}`))
const windows: Record<string, unknown>[] = conf.app.windows
const window = windows.find((each) => each.label === 'main')!
const quick = windows.find((each) => each.label === 'quick')!

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

  it('gives no page a way to the disk, the shell, the clipboard or the network', () => {
    expect(capabilities.length).toBeGreaterThanOrEqual(2)
    for (const capability of capabilities) {
      for (const permission of capability.permissions) {
        expect(permission.startsWith('core:'), `${capability.identifier}: ${permission} reaches past the window`).toBe(
          true,
        )
      }
    }
  })
})

describe('the quick search window', () => {
  it('is not made at launch: it exists only while a vault is open', () => {
    // Made at unlock and destroyed at lock (src-tauri/src/quick.rs), so no
    // title found in it outlives the lock in a hidden page.
    expect(quick.create).toBe(false)
    expect(quick.visible).toBe(false)
  })

  it('never offers to remember what is typed into it either', () => {
    expect(quick.generalAutofillEnabled).toBe(false)
  })

  it('shares the webview the main window runs in', () => {
    // WebView2 runs every window of an app in one browser process, and
    // refuses a second window whose arguments differ from the first's.
    expect(quick.additionalBrowserArgs).toBe(window.additionalBrowserArgs)
  })

  it('floats over everything, and stays out of the taskbar and the window switcher', () => {
    expect(quick.alwaysOnTop).toBe(true)
    expect(quick.skipTaskbar).toBe(true)
    expect(quick.decorations).toBe(false)
  })

  it('may hear that it was brought up, and nothing more', () => {
    const own = capabilities.filter((capability) => capability.windows.includes('quick'))
    expect(own.flatMap((capability) => capability.permissions)).toEqual(['core:event:default'])
  })
})
