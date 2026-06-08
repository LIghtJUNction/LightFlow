import Link from 'next/link'

export default function HomePage() {
  return (
    <main className="language-page">
      <section className="language-panel">
        <p className="language-kicker">LightFlow Docs</p>
        <h1>Choose your language</h1>
        <p>
          LightFlow is a backend-first AI workflow system built around Rust
          assets, CortexFS execution, and an OpenAPI control surface.
        </p>
        <div className="language-actions">
          <Link href="/zh/">中文文档</Link>
          <Link href="/en/">English Docs</Link>
        </div>
      </section>
    </main>
  )
}
