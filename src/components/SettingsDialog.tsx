import { useEffect, useState, type ReactNode } from 'react'
import { FolderOpen, Puzzle, RefreshCw, RotateCcw, X } from 'lucide-react'
import clsx from 'clsx'
import { getVersion } from '@tauri-apps/api/app'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import { api } from '../lib/api'
import { SETTINGS_TABS } from '../lib/settingsTabs'
import { checkForUpdatesNow, useUpdate } from '../lib/updater'
import { useStore } from '../store'
import { SpeedTest } from './SpeedTest'

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

/** Başlıklı kart; içindeki satırlar ince çizgiyle ayrılır. */
function Card({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section>
      {title && <h3 className="mb-2 px-1 text-xs font-semibold uppercase tracking-wider text-mute">{title}</h3>}
      <div className="divide-y divide-line/70 rounded-2xl border border-line bg-panel px-4">{children}</div>
    </section>
  )
}

function Row({ title, hint, children, stacked }: { title: string; hint?: string; children?: ReactNode; stacked?: boolean }) {
  return (
    <div className={clsx('py-3.5', stacked ? 'space-y-3' : 'flex items-center gap-4')}>
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium">{title}</div>
        {hint && <p className="mt-0.5 text-xs leading-relaxed text-mute">{hint}</p>}
      </div>
      {children}
    </div>
  )
}

function SwitchRow({
  checked,
  onChange,
  title,
  hint,
  disabled,
}: {
  checked: boolean
  onChange: () => void
  title: string
  hint: string
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={onChange}
      className="flex w-full items-center gap-4 py-3.5 text-left disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{title}</span>
        <span className="mt-0.5 block text-xs leading-relaxed text-mute">{hint}</span>
      </span>
      <span className={clsx('relative h-6 w-11 shrink-0 rounded-full transition-colors', checked ? 'bg-accent' : 'bg-white/15')}>
        <span className={clsx('absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all', checked ? 'left-[22px]' : 'left-0.5')} />
      </span>
    </button>
  )
}

function Segmented({
  value,
  options,
  onChange,
  cols,
}: {
  value: number
  options: { value: number; label: string }[]
  onChange: (v: number) => void
  cols: string
}) {
  return (
    <div className={clsx('grid gap-1.5 rounded-2xl bg-black/20 p-1.5', cols)}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => onChange(o.value)}
          aria-pressed={value === o.value}
          className={clsx(
            'h-9 rounded-xl text-sm font-medium tabular-nums transition-all active:scale-95',
            value === o.value ? 'bg-accent text-white shadow-md shadow-accent/30' : 'text-mute hover:bg-white/10 hover:text-ink',
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

export function SettingsDialog() {
  const settings = useStore((s) => s.settings)
  const save = useStore((s) => s.saveSettings)
  const tab = useStore((s) => s.settingsTab)
  const closeSettings = useStore((s) => s.closeSettings)
  const setTab = useStore((s) => s.setSettingsTab)
  const meta = SETTINGS_TABS.find((t) => t.id === tab) ?? SETTINGS_TABS[0]

  const inTauri = '__TAURI_INTERNALS__' in window

  // Windows ile başlat: işletim sisteminin kaydı okunur; düz tarayıcıda (simülasyon) gizlenir.
  const [autostart, setAutostart] = useState<boolean | null>(null)
  useEffect(() => {
    if (!inTauri) return
    isEnabled().then(setAutostart).catch(() => {})
  }, [inTauri])
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

  const [extPath, setExtPath] = useState<string | null>(null)
  useEffect(() => {
    api.extensionPath().then(setExtPath).catch(() => {})
  }, [])

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

  // Esc ayarlardan çıkar; açık bir iletişim kutusu varsa o önce kapanır.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && document.querySelectorAll('.dialog-overlay').length <= 1) closeSettings()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [closeSettings])

  const Icon = meta.icon

  return (
    <div
      className="dialog-overlay fixed inset-0 z-30 grid place-items-center bg-black/55 px-4 py-6 backdrop-blur-md"
      onMouseDown={(e) => e.target === e.currentTarget && closeSettings()}
    >
      <div className="dialog-card flex max-h-[calc(100vh-48px)] w-[640px] max-w-full flex-col overflow-hidden rounded-[28px] border border-white/15 bg-bg/90 shadow-[0_24px_80px_rgba(0,0,0,0.55),inset_0_1px_0_rgba(255,255,255,0.18)] backdrop-blur-3xl backdrop-saturate-150">
        <header className="flex shrink-0 items-center gap-3 border-b border-line/70 px-6 py-4">
          <div className="grid h-10 w-10 place-items-center rounded-2xl bg-accent text-white shadow-lg shadow-accent/30">
            <Icon size={19} />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="text-[17px] font-semibold leading-tight tracking-tight">{meta.title}</h2>
            <p className="text-xs text-mute">{meta.hint} Değişiklikler anında kaydedilir.</p>
          </div>
          <button
            type="button"
            onClick={closeSettings}
            aria-label="Kapat"
            className="grid h-9 w-9 place-items-center rounded-xl text-mute transition-colors hover:bg-white/10 hover:text-ink"
          >
            <X size={18} />
          </button>
        </header>

        <nav
          aria-label="Ayar kategorileri"
          className="mx-6 mt-4 flex shrink-0 items-center gap-1.5 rounded-[20px] border border-white/20 bg-white/[0.07] p-1.5 shadow-[inset_0_1px_0_rgba(255,255,255,0.14)]"
        >
          {SETTINGS_TABS.map((t) => {
            const TabIcon = t.icon
            const active = t.id === tab
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => setTab(t.id)}
                aria-pressed={active}
                title={t.label}
                className={clsx(
                  'orbit-hover relative flex h-10 min-w-0 flex-1 items-center justify-center gap-2 rounded-[14px] px-2 text-sm font-medium shadow-md transition-transform hover:scale-[1.04] active:scale-95',
                  active ? 'orbit-light bg-accent text-white' : 'bg-white/10 text-ink hover:bg-white/15',
                )}
              >
                <TabIcon size={17} className="shrink-0" />
                <span className={clsx('truncate', !active && 'hidden min-[560px]:inline')}>{t.label}</span>
              </button>
            )
          })}
        </nav>

        <div className="overflow-y-auto px-6 pb-6 pt-5">
      <div key={tab} className="page-in space-y-6">
        {tab === 'general' && (
          <>
            <Card title="Dosyalar">
              <Row
                stacked
                title="İndirme klasörü"
                hint="Yeni indirmeler buraya kaydedilir. Zaten süren ya da biten indirmeler bulundukları klasörde kalır."
              >
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
              </Row>
              <SwitchRow
                checked={settings.sortByCategory}
                onChange={() => save({ sortByCategory: !settings.sortByCategory })}
                title="Türüne göre klasörle"
                hint="Yeni indirmeler dosya türüne göre alt klasöre kaydedilir (Video, Müzik, Belgeler, Arşivler…)."
              />
              <SwitchRow
                checked={settings.scanOnComplete}
                onChange={() => save({ scanOnComplete: !settings.scanOnComplete })}
                title="Bitince virüs taraması"
                hint="İndirme tamamlanınca dosya Windows Defender ile taranır. Tehdit bulunursa dosya silinmez, yalnızca uyarılırsın."
              />
            </Card>

            {autostart !== null && (
              <Card title="Başlangıç">
                <SwitchRow
                  checked={autostart}
                  onChange={toggleAutostart}
                  title="Windows ile başlat"
                  hint="Bilgisayar açıldığında ve oturum başladığında uygulama kendiliğinden açılır; böylece tarayıcı eklentisi hep çalışır durumda olur."
                />
                <SwitchRow
                  checked={settings.startMinimized}
                  onChange={() => save({ startMinimized: !settings.startMinimized })}
                  disabled={!autostart}
                  title="Tepside küçültülmüş başlat"
                  hint="Windows ile açıldığında pencere görünmez, uygulama sistem tepsisinde başlar. Yalnızca “Windows ile başlat” açıkken geçerlidir."
                />
              </Card>
            )}
          </>
        )}

        {tab === 'transfer' && (
          <>
            <Card title="Bağlantılar">
              <Row
                stacked
                title="Varsayılan bağlantı sayısı"
                hint="Her indirme dosyayı bu kadar parçaya bölüp aynı anda indirir. Sunucu çok bağlantıyı reddediyorsa (429 hatası) sayıyı düşürebilirsin; uygulama bunu kendisi de yapar. Tarayıcı eklentisinden gelen indirmeler de bu sayıyı kullanır."
              >
                <Segmented
                  cols="grid-cols-6"
                  value={settings.connections}
                  options={CONNECTION_OPTIONS.map((n) => ({ value: n, label: String(n) }))}
                  onChange={(n) => save({ connections: n })}
                />
              </Row>
              <Row
                stacked
                title="Aynı anda indirme sayısı"
                hint={'Bu sayıdan fazla indirme eklenirse "Sırada" durumunda bekler ve biri bitince kendiliğinden başlar.'}
              >
                <Segmented
                  cols="grid-cols-6"
                  value={settings.maxConcurrent}
                  options={CONCURRENT_OPTIONS.map((n) => ({ value: n, label: String(n) }))}
                  onChange={(n) => save({ maxConcurrent: n })}
                />
              </Row>
              <SwitchRow
                checked={settings.spreadIps}
                onChange={() => save({ spreadIps: !settings.spreadIps })}
                title="CDN sunucularına dağıt"
                hint="Büyük siteler aynı adı birden çok sunucuya (IP) çözer. Açıkken bağlantılar bu sunuculara paylaştırılır. Sunucu tek adreste duruyorsa bir fark yaratmaz."
              />
            </Card>

            <Card title="Hız ve zaman">
              <Row stacked title="Hız sınırı" hint="Tüm indirmelerin toplamı bu hızı aşmaz; internetin başka işlere de kalır.">
                <Segmented
                  cols="grid-cols-4"
                  value={settings.speedLimitKbps}
                  options={SPEED_LIMITS.map((o) => ({ value: o.kbps, label: o.label }))}
                  onChange={(n) => save({ speedLimitKbps: n })}
                />
              </Row>
              <SwitchRow
                checked={settings.scheduleEnabled}
                onChange={() => save({ scheduleEnabled: !settings.scheduleEnabled })}
                title="Zamanlayıcı"
                hint="Açıkken indirmeler yalnızca seçtiğin saat aralığında çalışır; dışında bekler, aralık başlayınca kendiliğinden devam eder."
              />
              {settings.scheduleEnabled && (
                <div className="flex items-center gap-3 py-3.5 text-sm">
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
            </Card>

            <Card title="Ölçüm">
              <div className="py-3.5">
                <SpeedTest />
              </div>
            </Card>
          </>
        )}

        {tab === 'notifications' && (
          <>
            <Card title="Windows">
              <SwitchRow
                checked={settings.notifyOnComplete}
                onChange={() => save({ notifyOnComplete: !settings.notifyOnComplete })}
                title="Bitince bildirim göster"
                hint="Bir indirme tamamlandığında Windows bildirimi gelir."
              />
            </Card>
            <Card title="Uygulama içi">
              <div className="py-3.5 text-xs leading-relaxed text-mute">
                Hangi olaylarda toast gösterilip geçmişe kaydedileceğini seç. Tehdit bulundu ve özet uyuşmuyor uyarıları her zaman gösterilir.
              </div>
              {(
                [
                  ['downloadComplete', 'İndirme tamamlandı', 'Bir indirme bittiğinde bildirim gelir.'],
                  ['downloadFailed', 'İndirme hatası', 'Bir indirme başarısız olduğunda bildirim gelir.'],
                  ['hash', 'SHA-256 özeti', 'Özet hesaplandığında bildirim gelir.'],
                  ['verify', 'Özet doğrulama', 'Verdiğin beklenen özetle eşleşince bildirim gelir.'],
                  ['scan', 'Virüs taraması', 'Tarama temiz çıktığında ya da yapılamadığında bildirim gelir.'],
                ] as const
              ).map(([key, title, hint]) => (
                <SwitchRow
                  key={key}
                  checked={settings.notifyKinds[key]}
                  onChange={() => save({ notifyKinds: { ...settings.notifyKinds, [key]: !settings.notifyKinds[key] } })}
                  title={title}
                  hint={hint}
                />
              ))}
            </Card>
          </>
        )}

        {tab === 'browser' && (
          <>
            <Card title="Tarayıcı eklentisi">
              {extPath ? (
                <Row
                  stacked
                  title="Eklenti klasörü"
                  hint="Eklenti kurulumla birlikte bilgisayarına kopyalandı ve uygulama güncellendikçe kendiliğinden güncellenir. Chrome / Edge'e bir kez eklemek için chrome://extensions adresini aç, sağ üstten “Geliştirici modu”nu aç, “Paketlenmemiş öğe yükle”ye bas ve aşağıdaki klasörü seç."
                >
                  <p className="break-all rounded-lg bg-black/15 px-2.5 py-1.5 text-xs text-mute select-text" title={extPath}>
                    {extPath}
                  </p>
                  <button
                    type="button"
                    onClick={() => void api.openExtensionFolder()}
                    className="flex h-10 w-full items-center justify-center gap-2 rounded-2xl bg-black/20 text-sm font-medium transition-colors hover:bg-white/10"
                  >
                    <Puzzle size={15} />
                    Eklenti klasörünü aç
                  </button>
                </Row>
              ) : (
                <Row title="Eklenti klasörü" hint="Eklenti klasörü bulunamadı." />
              )}
            </Card>
            <Card title="Yansı adresleri">
              <Row
                title="Yansı (mirror) adresleri"
                hint={'Her indirmenin kendi penceresinde, "Yansı adresleri" bölümünden eklenir.'}
              />
            </Card>
          </>
        )}

        {tab === 'about' && (
          <Card title="Güncellemeler">
            <Row
              stacked
              title={appVersion ? `Kurulu sürüm: v${appVersion}` : 'İndirme Yöneticisi'}
              hint={inTauri ? updateNote : 'Sürüm bilgisi yalnızca kurulu uygulamada görünür.'}
            >
              {inTauri && (
                <button
                  type="button"
                  disabled={updateBusy}
                  onClick={() => void checkForUpdatesNow()}
                  className="flex h-10 w-full items-center justify-center gap-2 rounded-2xl bg-black/20 text-sm font-medium transition-colors hover:bg-white/10 disabled:opacity-60"
                >
                  <RefreshCw size={15} className={clsx(updateBusy && 'animate-spin')} />
                  Güncellemeleri denetle
                </button>
              )}
            </Row>
          </Card>
        )}
      </div>
        </div>
      </div>
    </div>
  )
}
