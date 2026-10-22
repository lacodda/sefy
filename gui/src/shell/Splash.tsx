import { useEffect } from 'react'
import { ProductMark } from '@/components/ui/product-mark'
import { Splash as SplashScreen } from '@/components/ui/splash'
import { text } from '@/text'

interface Props {
  /** What the window is doing, when it has something to say: unsealing a
   * vault. Nothing at launch, where the picture is the static one's. */
  status?: string
  tip?: string
}

/**
 * What the window shows while it cannot show anything else.
 *
 * The same picture `index.html` painted before the bundle arrived - the mark,
 * the name, the promise, the version - so that nothing jumps when React takes
 * over; the static one is taken down on the first render, because from this
 * moment the page is the one drawing.
 *
 * It never says which vault. No file name, no path, no count of what is in
 * it: a splash is on screen while nobody is looking at it on purpose, which
 * is exactly when somebody else is, and the window says no more there than
 * the name of the program would.
 */
export function Splash({ status, tip }: Props) {
  useEffect(() => {
    document.getElementById('splash')?.remove()
  }, [])

  return (
    <SplashScreen
      mark={<ProductMark product="sefy" size={56} />}
      name={text.app.name}
      tagline={text.app.tagline}
      version={`v${__APP_VERSION__}`}
      status={status}
      tip={tip}
    />
  )
}
