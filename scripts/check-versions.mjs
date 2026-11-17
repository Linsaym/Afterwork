import { readFileSync } from 'node:fs'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const json = path => JSON.parse(read(path))
// Read only the package tables and string fields used by these Cargo files.
const field = (table, key) => table?.match(new RegExp(`^[ \\t]*${key}[ \\t]*=[ \\t]*"([^"\\r\\n]+)"`, 'm'))?.[1]
const manifest = read('src-tauri/Cargo.toml').split(/(?=^\[)/m).find(table => table.startsWith('[package]'))
const crate = field(manifest, 'name')
const lock = read('src-tauri/Cargo.lock').split(/^\[\[package\]\][ \t]*\r?$/m).slice(1).find(table =>
  crate && field(table, 'name') === crate && !field(table, 'source')
)
const npmLock = json('package-lock.json')
const expected = json('package.json').version
const versions = {
  'package-lock.json (version)': npmLock.version,
  'package-lock.json (packages[""].version)': npmLock.packages?.['']?.version,
  'src-tauri/Cargo.toml ([package])': field(manifest, 'version'),
  'src-tauri/Cargo.lock (local package)': field(lock, 'version'),
  'src-tauri/tauri.conf.json': json('src-tauri/tauri.conf.json').version,
}

if (typeof expected !== 'string' || !expected) {
  console.error('package.json: missing version')
  process.exitCode = 1
} else {
  for (const [file, version] of Object.entries(versions)) {
    if (version === expected) continue
    console.error(`${file}: found ${JSON.stringify(version) ?? 'no version'}, expected ${expected} (package.json)`)
    process.exitCode = 1
  }
  if (!process.exitCode) console.log(`Afterwork versions match: ${expected}`)
}
