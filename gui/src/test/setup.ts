import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'
import { clearMocks } from '@tauri-apps/api/mocks'

// jsdom has no `matchMedia`, and the theme asks it which scheme the system
// prefers. Nothing here depends on the answer.
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }) as MediaQueryList
}

afterEach(async () => {
  cleanup()
  // A page that listened for an event lets go of it when it unmounts, and
  // the letting go finishes a turn later; the mocks it lets go through have
  // to still be there when it does.
  await new Promise((resolve) => setTimeout(resolve, 0))
  clearMocks()
})
