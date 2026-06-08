import { getPageMap } from 'nextra/page-map'
import { Layout, Navbar } from 'nextra-theme-docs'
import 'nextra-theme-docs/style.css'

const languages = [
  { locale: 'zh', name: '中文' },
  { locale: 'en', name: 'English' }
]

const navbar = (
  <Navbar
    logo={<strong>LightFlow</strong>}
    projectLink="https://github.com/LIghtJUNction/LightFlow"
  />
)

export async function generateStaticParams() {
  return languages.map(({ locale }) => ({ locale }))
}

export async function generateMetadata(props) {
  const { locale } = await props.params
  const isZh = locale === 'zh'
  return {
    title: {
      default: isZh ? 'LightFlow 文档' : 'LightFlow Docs',
      template: '%s - LightFlow'
    },
    description: isZh
      ? 'LightFlow 的设计、架构、资产模型、运行流程和部署文档。'
      : 'Design, architecture, asset model, run lifecycle, and deployment documentation for LightFlow.'
  }
}

export default async function LocaleLayout({ children, params }) {
  const { locale } = await params
  const isZh = locale === 'zh'

  return (
    <Layout
      navbar={navbar}
      pageMap={await getPageMap(`/${locale}`)}
      i18n={languages}
      docsRepositoryBase="https://github.com/LIghtJUNction/LightFlow/tree/main/docs-site"
      sidebar={{ defaultMenuCollapseLevel: 1 }}
      editLink={isZh ? '在 GitHub 上编辑此页' : 'Edit this page on GitHub'}
      feedback={{ content: isZh ? '反馈这页内容' : 'Give feedback on this page' }}
      footer={<span>MIT OR Apache-2.0 Licensed.</span>}
      toc={{ title: isZh ? '本页内容' : 'On this page' }}
      themeSwitch={
        isZh
          ? { light: '浅色', dark: '深色', system: '系统' }
          : { light: 'Light', dark: 'Dark', system: 'System' }
      }
    >
      {children}
    </Layout>
  )
}
