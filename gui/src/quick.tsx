import React from 'react'
import ReactDOM from 'react-dom/client'
import { initTheme } from 'dowel-ui'
import { Quick } from '@/screens/Quick'
import { guard } from '@/shell/guards'
import '@/styles.css'

// The quick search window: made hidden at unlock, shown by the shortcut from
// the process (src-tauri/src/quick.rs), never by the page itself.
initTheme()
guard()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Quick />
  </React.StrictMode>,
)
