// Yeni sürümü derler, imzalar ve YALNIZCA kurulum/güncelleme dosyalarını GitHub sürüm deposuna yükler.
// Kullanım: node scripts/release.mjs ["Sürüm notu"]
// Sürüm numarası src-tauri/tauri.conf.json içindeki "version" alanından okunur; yayından önce artırın.
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const REPO = 'Fatal-IV/download-manager-releases'
const KEY_PATH = join(homedir(), '.tauri', 'downloadmanager.key')
const notes = process.argv[2] ?? ''

const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
const version = conf.version
const tag = `v${version}`
// npm bir .cmd dosyası olduğundan kabuk gerekir; gh gerçek bir exe'dir ve boşluklu argümanlar bozulmadan geçmelidir.
const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { stdio: 'inherit', shell: cmd === 'npm' && process.platform === 'win32', ...opts })

const existing = execFileSync('gh', ['release', 'list', '--repo', REPO, '--json', 'tagName']).toString()
if (JSON.parse(existing).some((r) => r.tagName === tag)) {
  console.error(`${tag} sürümü zaten yayında. tauri.conf.json içindeki sürümü artırın.`)
  process.exit(1)
}

run('npm', ['run', 'tauri', 'build'], {
  env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: KEY_PATH, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' },
})

const bundleDir = 'src-tauri/target/release/bundle/nsis'
const built = `${conf.productName}_${version}_x64-setup.exe`
// GitHub eklerde boşluk ve Türkçe karakterleri değiştirdiği için ASCII bir ad kullanılır.
const asset = `IndirmeYoneticisi_${version}_x64-setup.exe`
const out = join('src-tauri', 'target', 'release', 'release-upload')
mkdirSync(out, { recursive: true })
copyFileSync(join(bundleDir, built), join(out, asset))
copyFileSync(join(bundleDir, `${built}.sig`), join(out, `${asset}.sig`))
// README'deki "son sürümü indir" bağlantısı için sürümsüz, sabit adlı kopya.
const stable = 'IndirmeYoneticisi_x64-setup.exe'
copyFileSync(join(out, asset), join(out, stable))

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature: readFileSync(join(out, `${asset}.sig`), 'utf8').trim(),
      url: `https://github.com/${REPO}/releases/download/${tag}/${asset}`,
    },
  },
}
writeFileSync(join(out, 'latest.json'), JSON.stringify(latest, null, 2))

run('gh', [
  'release', 'create', tag,
  join(out, asset), join(out, stable), join(out, 'latest.json'),
  '--repo', REPO, '--title', `İndirme Yöneticisi ${version}`, '--notes', notes || `Sürüm ${version}`,
])
console.log(`\n${tag} yayınlandı: https://github.com/${REPO}/releases/tag/${tag}`)
