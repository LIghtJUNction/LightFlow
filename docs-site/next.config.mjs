import nextra from 'nextra'

const repositoryName = process.env.GITHUB_REPOSITORY?.split('/')[1] ?? 'LightFlow'
const basePath = process.env.GITHUB_ACTIONS === 'true' ? `/${repositoryName}` : ''

const withNextra = nextra({
  defaultShowCopyCode: true,
  unstable_shouldAddLocaleToLinks: true
})

export default withNextra({
  output: 'export',
  trailingSlash: true,
  basePath,
  assetPrefix: basePath ? `${basePath}/` : undefined,
  images: {
    unoptimized: true
  },
  i18n: {
    locales: ['zh', 'en'],
    defaultLocale: 'zh'
  }
})
