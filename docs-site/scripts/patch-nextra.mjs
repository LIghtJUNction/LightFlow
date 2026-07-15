import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const schemaPath = join(
  process.cwd(),
  'node_modules',
  'nextra-theme-docs',
  'dist',
  'schemas.js'
)

const before = 'children: reactNode,'
const after = 'children: reactNode.optional(),'

let source = readFileSync(schemaPath, 'utf8')

if (source.includes(after)) {
  process.exit(0)
}

if (!source.includes(before)) {
  throw new Error('Could not find Nextra Layout children schema to patch')
}

source = source.replace(before, after)
writeFileSync(schemaPath, source)
