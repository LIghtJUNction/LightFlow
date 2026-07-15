import { generateStaticParamsFor, importPage } from 'nextra/pages'
import { useMDXComponents as getMDXComponents } from '../../../mdx-components'

export const generateStaticParams = generateStaticParamsFor('mdxPath', 'locale')

export async function generateMetadata(props) {
  const params = await props.params
  const { metadata } = await importPage(params.mdxPath, params.locale)
  return metadata
}

const Wrapper = getMDXComponents().wrapper

export default async function Page(props) {
  const params = await props.params
  const result = await importPage(params.mdxPath, params.locale)
  const { default: MDXContent, ...pageProps } = result

  return (
    <Wrapper {...pageProps}>
      <MDXContent {...props} params={params} />
    </Wrapper>
  )
}
