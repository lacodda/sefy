import type { ReactNode } from 'react'
import { ProductMark } from '@/components/ui/product-mark'
import { TitleBar } from '@/components/ui/window-frame'
import { text } from '@/text'

/**
 * The window's title bar, and the application's: one bar rather than a
 * system one over an application one.
 *
 * It is dowel's TitleBar, built the way kilna builds it - the mark, the name
 * and the version at the left where the system put the icon, everything that
 * is not a control a handle to drag the window by, the product's own controls
 * before the window's buttons - on the page's own ground rather than a raised
 * strip.
 *
 * The bar names the product and nothing else. The open vault's name is on the
 * screen below it, which a lock takes away; a title bar is also what the
 * taskbar and the window switcher show.
 */
export function Titlebar({ actions }: { actions?: ReactNode }) {
  return (
    <TitleBar
      className="bg-bg"
      labels={text.window}
      actions={actions}
    >
      <div className="flex shrink-0 items-center gap-2 pl-3">
        <ProductMark product="sefy" size={20} />
        <b className="text-base font-semibold">{text.app.name}</b>
        <small className="font-mono text-2xs text-faint">{__APP_VERSION__}</small>
      </div>
    </TitleBar>
  )
}
