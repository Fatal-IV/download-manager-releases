import { useEffect, useState } from 'react'
import { FolderOpen, RefreshCw, RotateCcw, Settings as SettingsIcon, X } from 'lucide-react'
import clsx from 'clsx'
import { useStore } from '../store'
import { SpeedTest } from './SpeedTest'
import { getVersion } from '@tauri-apps/api/app'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import { api } from '../lib/api'
import { checkForUpdatesNow, useUpdate } from '../lib/updater'

const CONNECTION_OPTIONS = [1, 2, 4, 8, 16, 32]
const CONCURRENT_OPTIONS = [1, 2, 3, 4, 5, 8]
const SPEED_LIMITS = [
  { kbps: 0, label: 'Sınırsız' },
  { kbps: 256, label: '256 KB/sn' },
  { kbps: 512, label: '512 KB/sn' },
  { kbps: 1024, label: '1 MB/sn' },
  { kbps: 2048, label: '2 MB/sn' },
  { kbps: 5120, label: '5 MB/sn' },
  { kbps: 10240, label: '10 MB/sn' },
  { kbps: 25600, label: '25 MB/sn' },
]

const toTime = (min: number) => `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`
const fromTime = (v: string) => {
  const [h, m] = v.split(':').map(Number)
  return Number.isFinite(h) && Number.isFinite(m) ? h * 60 + m : 0
}

function Toggle({ checked, onChange, title, hint }: { checked: boolean; onChange: () => void; title: string; hint: string }) {
  return (
    <button type="button" role="switch" aria-checked={checked} onClick={onChange} className="flex w-full items-start gap-4 rounded-2xl px-1 py-1 text-left">
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{title}</span>
        <span className="mt-0.5 block text-xs text-mute">{hint}</span>
      </span>
      <span className={clsx('relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors', checked ? 'bg-accent' : 'bg-white/15')}>
        <span className={clsx('absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all', checked ? 'left-[22px]' : 'left-0.5')} />
      </span>
    </button>
  )
}

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const settings = useStore((s) => s.settings)
  const save = useStore((s) => s.saveSettings)

  // Windows ile başlat: işletim sisteminin kaydı okunur; düz tarayıcıda (simülasyon) gizlenir.
  const [autostart, setAutostart] = useState<boolean | null>(null)
  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    isEnabled().then(setAutostart).catch(() => {})
  }, [])
  const toggleAutostart = async () => {
    if (autostart === null) return
    const next = !autostart
    setAutostart(next)
    try {
      await (next ? enable() : disable())
    } catch {
      setAutostart(!next)
    }
  }

  const inTauri = '__TAURI_INTERNALS__' in window
  const [appVersion, setAppVersion] = useState('')
  useEffect(() => {
    if (inTauri) getVersion().then(setAppVersion).catch(() => {})
  }, [inTauri])
  const { manual, phase: updatePhase, version: updateVersion } = useUpdate()
  const updateBusy = manual === 'checking' || updatePhase === 'downloading' || updatePhase === 'installing'
  let updateNote = 'Uygulama açıkken her 15 dakikada bir kendiliğinden de denetlenir.'
  if (manual === 'checking') updateNote = 'Denetleniyor…'
  else if (manual === 'latest') updateNote = 'Güncelsin, yeni sürüm yok.'
  else if (manual === 'failed') updateNote = 'Denetlenemedi. İnternet bağlantını kontrol edip yeniden dene.'
  else if (updatePhase === 'downloading') updateNote = `v${updateVersion} indiriliyor…`
  else if (updatePhase === 'ready') updateNote = `v${updateVersion} hazır; sağ alttaki "Yenile" düğmesiyle kurabilirsin.`

  const [defaultDir, setDefaultDir] = useState('')
  useEffect(() => {
    api.defaultDownloadDir().then(setDefaultDir).catch(() => {})
  }, [])
  const currentDir = settings.downloadDir || defaultDir
  const pickFolder = async () => {
    const dir = await api.pickFolder(currentDir)
    if (dir) save({ downloadDir: dir })
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div
      className="dialog-overlay fixed inset-0 z-30 grid place-items-center bg-black/55 px-4 backdrop-blur-md"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="dialog-card max-h-[92vh] w-[560px] max-w-full overflow-y-auto rounded-[28px] border border-white/15 bg-panel/85 shadow-[0_24px_80px_rgba(0,0,0,0.55),inset_0_1px_0_rgba(255,255,255,0.18)] backdrop-blur-3xl backdrop-saturate-150">
        <div className="flex items-center gap-3 px-6 pt-6">
          <div className="grid h-11 w-11 place-items-center rounded-2xl bg-accent text-white shadow-lg shadow-accent/30">
            <SettingsIcon size={20} />
          </div>
          <div className="flex-1">
            <h2 className="text-[17px] font-semibold leading-tight tracking-tight">Ayarlar</h2>
            <p className="text-xs text-mute">Değişiklikler anında kaydedilir.</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Kapat"
            className="grid h-9 w-9 place-items-center rounded-xl text-mute transition-colors hover:bg-white/10 hover:text-ink"
          >
            <X size={18} />
          </button>
        </div>

        <div className="space-y-6 px-6 pb-7 pt-5">
          <section>
            <div className="mb-2 px-1">
              <div className="text-sm font-medium">İndirme klasörü</div>
              <p className="mt-0.5 text-xs text-mute">
                Yeni indirmeler buraya kaydedilir. Zaten süren ya da biten indirmeler bulundukları
                klasörde kalır.
              </p>
            </div>
            <div className="flex items-center gap-1.5 rounded-2xl bg-black/20 p-1.5">
              <button
                type="button"
                onClick={pickFolder}
                title="Klasör seç"
                className="flex h-10 min-w-0 flex-1 items-center gap-2.5 rounded-xl px-3 text-left text-sm transition-colors hover:bg-white/10"
              >
                <FolderOpen size={16} className="shrink-0 text-mute" />
                <span className="truncate" dir="rtl">
                  <bdi>{currentDir || '…'}</bdi>
                </span>
              </button>
              {settings.downloadDir && (
                <button
                  type="button"
                  onClick={() => save({ downloadDir: '' })}
                  title="Varsayılana dön"
                  aria-label="Varsayılan klasöre dön"
                  className="grid h-10 w-10 shrink-0 place-items-center rounded-xl text-mute transition-colors hover:bg-white/10 hover:text-ink"
                >
                  <RotateCcw size={15} />
                </button>
              )}
            </div>
          </section>

          <section>
            <div className="mb-2 px-1">
              <div className="text-sm font-medium">Varsayılan bağlantı sayısı</div>
              <p className="mt-0.5 text-xs text-mute">
                Her indirme dosyayı bu kadar parçaya bölüp aynı anda indirir. Sunucu çok bağlantıyı
                reddediyorsa (429 hatası) sayıyı düşürebilirsin; uygulama bunu kendisi de yapar. Tarayıcı
                eklentisinden gelen indirmeler de bu sayıyı kullanır.
              </p>
            </div>
            <div className="grid grid-cols-6 gap-1.5 rounded-2xl bg-black/20 p-1.5">
              {CONNECTION_OPTIONS.map((n) => (
                <button
                  key={n}
                  type="button"
                  onClick={() => save({ connections: n })}
                  aria-pressed={settings.connections === n}
                  className={clsx(
                    'h-10 rounded-xl text-sm font-medium tabular-nums transition-all active:scale-95',
                    settings.connections === n
                      ? 'bg-accent text-white shadow-md shadow-accent/30'
                      : 'text-mute hover:bg-white/10 hover:text-ink',
                  )}
                >
                  {n}
                </button>
              ))}
            </div>
          </section>

          <section>
            <div className="mb-2 px-1">
              <div className="text-sm font-medium">Aynı anda indirme sayısı</div>
              <p className="mt-0.5 text-xs text-mute">
                Bu sayıdan fazla indirme eklenirse "Sırada" durumunda bekler ve biri bitince kendiliğinden
                başlar.
              </p>
            </div>
            <div className="grid grid-cols-6 gap-1.5 rounded-2xl bg-black/20 p-1.5">
              {CONCURRENT_OPTIONS.map((n) => (
                <button
                  key={n}
                  type="button"
                  onClick={() => save({ maxConcurrent: n })}
                  aria-pressed={settings.maxConcurrent === n}
                  className={clsx(
                    'h-10 rounded-xl text-sm font-medium tabular-nums transition-all active:scale-95',
                    settings.maxConcurrent === n
                      ? 'bg-accent text-white shadow-md shadow-accent/30'
                      : 'text-mute hover:bg-white/10 hover:text-ink',
                  )}
                >
                  {n}
                </button>
              ))}
            </div>
          </section>

          <section>
            <div className="mb-2 px-1">
              <div className="text-sm font-medium">Hız sınırı</div>
              <p className="mt-0.5 text-xs text-mute">
                Tüm indirmelerin toplamı bu hızı aşmaz; internetin başka işlere de kalır.
              </p>
            </div>
            <div className="grid grid-cols-4 gap-1.5 rounded-2xl bg-black/20 p-1.5">
              {SPEED_LIMITS.map((o) => (
                <button
                  key={o.kbps}
                  type="button"
                  onClick={() => save({ speedLimitKbps: o.kbps })}
                  aria-pressed={settings.speedLimitKbps === o.kbps}
                  className={clsx(
                    'h-10 rounded-xl text-xs font-medium transition-all active:scale-95',
                    settings.speedLimitKbps === o.kbps
                      ? 'bg-accent text-white shadow-md shadow-accent/30'
                      : 'text-mute hover:bg-white/10 hover:text-ink',
                  )}
                >
                  {o.label}
                </button>
              ))}
            </div>
          </section>

          <section className="space-y-3">
            <Toggle
              checked={settings.scheduleEnabled}
              onChange={() => save({ scheduleEnabled: !settings.scheduleEnabled })}
              title="Zamanlayıcı"
              hint="Açıkken indirmeler yalnızca seçtiğin saat aralığında çalışır; dışında bekler, aralık başlayınca kendiliğinden devam eder."
            />
            {settings.scheduleEnabled && (
              <div className="flex items-center gap-3 rounded-2xl bg-black/20 p-3 text-sm">
                <input
                  type="time"
                  value={toTime(settings.scheduleStart)}
                  onChange={(e) => save({ scheduleStart: fromTime(e.target.value) })}
                  aria-label="Başlangıç saati"
                  className="h-10 flex-1 rounded-xl bg-black/20 px-3 outline-none"
                />
                <span className="text-mute">–</span>
                <input
                  type="time"
                  value={toTime(settings.scheduleEnd)}
                  onChange={(e) => save({ scheduleEnd: fromTime(e.target.value) })}
                  aria-label="Bitiş saati"
                  className="h-10 flex-1 rounded-xl bg-black/20 px-3 outline-none"
                />
              </div>
            )}
          </section>

          <section className="space-y-3">
            <Toggle
              checked={settings.notifyOnComplete}
              onChange={() => save({ notifyOnComplete: !settings.notifyOnComplete })}
              title="Bitince bildirim göster"
              hint="Bir indirme tamamlandığında Windows bildirimi gelir."
            />
            <Toggle
              checked={settings.scanOnComplete}
              onChange={() => save({ scanOnComplete: !settings.scanOnComplete })}
              title="Bitince virüs taraması"
              hint="İndirme tamamlanınca dosya Windows Defender ile taranır. Tehdit bulunursa dosya silinmez, yalnızca uyarılırsın."
            />
            <Toggle
              checked={settings.sortByCategory}
              onChange={() => save({ sortByCategory: !settings.sortByCategory })}
              title="Türüne göre klasörle"
              hint="Yeni indirmeler dosya türüne göre alt klasöre kaydedilir (Video, Müzik, Belgeler, Arşivler…)."
            />
          </section>

          <section>
            <button
              type="button"
              role="switch"
              aria-checked={settings.spreadIps}
              onClick={() => save({ spreadIps: !settings.spreadIps })}
              className="flex w-full items-start gap-4 rounded-2xl px-1 py-1 text-left"
            >
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">CDN sunucularına dağıt</span>
                <span className="mt-0.5 block text-xs text-mute">
                  Büyük siteler aynı adı birden çok sunucuya (IP) çözer. Açıkken bağlantılar bu
                  sunuculara paylaştırılır. Sunucu tek adreste duruyorsa bir fark yaratmaz.
                </span>
              </span>
              <span
                className={clsx(
                  'relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors',
                  settings.spreadIps ? 'bg-accent' : 'bg-white/15',
                )}
              >
                <span
                  className={clsx(
                    'absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all',
                    settings.spreadIps ? 'left-[22px]' : 'left-0.5',
                  )}
                />
              </span>
            </button>
          </section>

          {autostart !== null && (
            <section>
              <button
                type="button"
                role="switch"
                aria-checked={autostart}
                onClick={toggleAutostart}
                className="flex w-full items-start gap-4 rounded-2xl px-1 py-1 text-left"
              >
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium">Windows ile başlat</span>
                  <span className="mt-0.5 block text-xs text-mute">
                    Bilgisayar açıldığında ve oturum başladığında uygulama kendiliğinden açılır; böylece
                    tarayıcı eklentisi hep çalışır durumda olur.
                  </span>
                </span>
                <span
                  className={clsx(
                    'relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors',
                    autostart ? 'bg-accent' : 'bg-white/15',
                  )}
                >
                  <span
                    className={clsx(
                      'absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all',
                      autostart ? 'left-[22px]' : 'left-0.5',
                    )}
                  />
                </span>
              </button>
            </section>
          )}

          {inTauri && (
            <section>
              <div className="mb-2 px-1">
                <div className="text-sm font-medium">Güncellemeler</div>
                <p className="mt-0.5 text-xs text-mute">
                  {appVersion && `Kurulu sürüm: v${appVersion}. `}
                  {updateNote}
                </p>
              </div>
              <button
                type="button"
                disabled={updateBusy}
                onClick={() => void checkForUpdatesNow()}
                className="flex h-10 w-full items-center justify-center gap-2 rounded-2xl bg-black/20 text-sm font-medium transition-colors hover:bg-white/10 disabled:opacity-60"
              >
                <RefreshCw size={15} className={clsx(updateBusy && 'animate-spin')} />
                Güncellemeleri denetle
              </button>
            </section>
          )}

          <SpeedTest />

          <p className="rounded-xl bg-black/15 px-3 py-2.5 text-xs text-mute">
            Yansı (mirror) adreslerini her indirmenin kendi penceresinde, "Yansı adresleri" bölümünden
            ekleyebilirsin.
          </p>
        </div>
      </div>
    </div>
  )
}
