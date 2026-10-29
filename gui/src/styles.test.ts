import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { describe, expect, it } from 'vitest'

/*
 * Rules the page's libraries write into a `<style>` while it runs, copied
 * into the bundle's stylesheet: under the window's content security policy a
 * `<style>` made at run time is dropped without a word (see styles.css). A
 * copy drifts when the library changes its rule, so it is held to the
 * library's own text.
 */

const require = createRequire(import.meta.url)
const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8').replace(/\s+/g, '')

describe('the bundle stylesheet', () => {
  it('carries the rule Base UI hides the native scrollbar with', () => {
    // Not an export of the package: found beside one that is.
    const root = dirname(dirname(require.resolve('@base-ui/react/scroll-area')))
    const source = readFileSync(join(root, 'utils', 'styles.js'), 'utf8')
    const rule = /children:\s*`([^`]*DISABLE_SCROLLBAR_CLASS_NAME[^`]*)`/.exec(source)?.[1]
    expect(rule, 'Base UI no longer writes the rule this test reads').toBeDefined()
    const written = rule!.replace(/\$\{DISABLE_SCROLLBAR_CLASS_NAME\}/g, 'base-ui-disable-scrollbar')
    for (const piece of written.split('}').filter(Boolean)) {
      expect(css).toContain(`${piece.replace(/\s+/g, '')};}`)
    }
  })
})
