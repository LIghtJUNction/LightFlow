import { Head } from 'nextra/components'
import './root.css'

export const metadata = {
  title: 'LightFlow Docs',
  description: 'LightFlow documentation in Chinese and English.'
}

export default function RootLayout({ children }) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <Head />
      <body>
        <script
          dangerouslySetInnerHTML={{
            __html:
              "document.documentElement.lang=location.pathname.split('/').includes('en')?'en':'zh-CN'"
          }}
        />
        {children}
      </body>
    </html>
  )
}
