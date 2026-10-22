import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { describe, expect, it } from 'vitest'
import { marks } from 'dowel-ui/marks'
import { text } from '@/text'

/*
 * The static half of the splash: `index.html`, painted before the bundle.
 *
 * It writes the theme out by hand, because the theme is in the bundle being
 * waited for - and anything written by hand drifts. dowel ships every colour
 * of its theme on sefy's accent, resolved by a browser (`dowel-ui/palettes`),
 * so the page is held to that rather than to a number someone measured once.
 */

const require = createRequire(import.meta.url)
const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8')
const palette = require('dowel-ui/palettes/sefy.json').color as Record<
  'dark' | 'light',
  Record<string, { $value: string }>
>
const conf = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'))

/** The `background:` and `color:` of the first rule whose selector is `selector`. */
function rule(selector: string): string {
  const at = html.indexOf(`${selector} {`)
  expect(at, `no rule for ${selector}`).toBeGreaterThan(-1)
  return html.slice(at, html.indexOf('}', at))
}

const value = (name: string, block: string) => new RegExp(`${name}:\\s*(#[0-9a-f]{6})`, 'i').exec(block)?.[1]

describe('the static splash', () => {
  it('is drawn on the theme’s own ground, in both schemes', () => {
    for (const scheme of ['dark', 'light'] as const) {
      const block = rule(`html.${scheme}`)
      expect(value('background', block)?.toLowerCase()).toBe(palette[scheme].bg!.$value)
      expect(value('color', block)?.toLowerCase()).toBe(palette[scheme].text!.$value)
    }
    // The system's preference, before a class is set: dark by default, light
    // under the media query.
    expect(value('background', rule('html'))?.toLowerCase()).toBe(palette.dark.bg!.$value)
  })

  it('sweeps in the accent of each scheme', () => {
    for (const scheme of ['dark', 'light'] as const) {
      const block = rule(`html.${scheme} #splash .bar::after`)
      expect(value('background', block)?.toLowerCase()).toBe(palette[scheme].accent!.$value)
    }
  })

  it('opens the window on the same ground, so nothing white is ever seen', () => {
    expect(conf.app.windows[0].backgroundColor.toLowerCase()).toBe(palette.dark.bg!.$value)
    // Hidden until the page has painted; `main.tsx` shows it.
    expect(conf.app.windows[0].visible).toBe(false)
  })

  it('draws the mark dowel ships, at the level React draws it', () => {
    // The geometry and the colours of sefy's M-level master, attribute by
    // attribute: the static mark and ProductMark are one picture.
    for (const attribute of marks.sefy.M.match(/[a-z-]+="[^"]*"/g) ?? []) {
      expect(html, attribute).toContain(attribute)
    }
    expect(html).toMatch(/>\s*se\s*</)
  })

  it('says the product’s name and promise, and nothing about any vault', () => {
    const splash = html.slice(html.indexOf('<div id="splash"'), html.indexOf('<div id="root"'))
    expect(splash).toContain(`<div class="name">${text.app.name}</div>`)
    expect(splash).toContain(`<div class="tagline">${text.app.tagline}</div>`)
    expect(splash).toContain('v__APP_VERSION__')
    // Exactly the five parts, so that a line added here - a file name, a
    // count - fails rather than ships.
    const parts = [...splash.matchAll(/class="([a-z]+)"/g)].map((match) => match[1])
    expect(parts).toEqual(['mark', 'name', 'tagline', 'version', 'bar'])
  })
})
