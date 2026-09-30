import { useEffect, useMemo, useRef, useState } from 'react'
import { Gauge, Loader2 } from 'lucide-react'
import clsx from 'clsx'
import { api } from '../lib/api'
import { formatBytes, formatSpeed } from '../lib/format'
import { useStore } from '../store'
import type { SpeedUpdate } from '../types'

const LEVELS = [1, 4, 8, 16]
const MIN_TEST_BYTES = 32 * 1024 * 1024

type Rows = Record<number, SpeedUpdate | undefined>

/** En yüksek hızın %90'ına ulaşan en küçük bağlantı sayısı: daha fazlası sunucuyu boşuna yorar. */
function recommend(rows: Rows): { best: number; max: number; flat: boolean } | null {
  const done = LEVELS.map((n) => rows[n]).filter((r): r is SpeedUpdate => r?.state === 'done')
  if (done.length === 0) return null
  const max = Math.max(...done.map((r) => r.speed))
  const best = done.find((r) => r.speed >= max * 0.9)!.connections
  const one = rows[1]
  const flat = one?.state === 'done' && done.length > 1 && one.speed >= max * 0.9
  return { best, max, flat }
}

export function SpeedTest() {
  const downloads = useStore((s) => s.downloads)
  const current = useStore((s) => s.settings.connections)
  const save = useStore((s) => s.saveSettings)

  // Test için en yakın adayı öner: en son eklenen, yeterince büyük indirme.
  const candidate = useMemo(
    () =>
      Object.values(downloads)
        .filter((d) => d.totalBytes >= MIN_TEST_BYTES)
        .sort((a, b) => b.addedAt - a.addedAt)[0],
    [downloads],
  )
  const [url, setUrl] = useState('')
  const [id, setId] = useState<string | undefined>()
  const [running, setRunning] = useState(false)
  const [rows, setRows] = useState<Rows>({})
  const [error, setError] = useState('')
  const [finished, setFinished] = useState(false)
  const touched = useRef(false)

  useEffect(() => {
    if (!touched.current && candidate) {
      setUrl(candidate.url)
      setId(candidate.id)
    }
  }, [candidate])

  useEffect(() => api.subscribeSpeedTest((u) => setRows((r) => ({ ...r, [u.connections]: u }))), [])

  async function start() {
    setRunning(true)
    setError('')
    setFinished(false)
    setRows({})
    try {
      await api.speedTest(url.trim(), id)
      setFinished(true)
    } catch (e) {
      setError(String(e))
    } finally {
      setRunning(false)
    }
  }

  const rec = finished ? recommend(rows) : null
  const maxSpeed = Math.max(1, ...LEVELS.map((n) => rows[n]?.speed ?? 0))
  const limited = LEVELS.map((n) => rows[n]).find((r) => r?.state === 'limited')

  return (
    <section>
      <div className="mb-2 px-1">
        <div className="text-sm font-medium">Hız testi</div>
        <p className="mt-0.5 text-xs text-mute">
          Aynı dosyayı 1, 4, 8 ve 16 bağlantıyla birkaç saniye indirip hızı ölçer; hiçbir şey diske
          yazılmaz. Böylece bağlantı sayısının gerçekten işe yarayıp yaramadığını görürsün. Yaklaşık 40
          saniye sürer.
        </p>
      </div>

      <div className="flex gap-2">
        <input
          value={url}
          onChange={(e) => {
            touched.current = true
            setId(undefined)
            setUrl(e.target.value)
          }}
          disabled={running}
          placeholder="Test edilecek dosya adresi (en az 32 MB)"
          spellCheck={false}
          className="h-10 min-w-0 flex-1 select-text rounded-xl border border-white/10 bg-black/20 px-3 text-xs outline-none transition-colors placeholder:text-mute/70 focus:border-accent disabled:opacity-60"
        />
        {running ? (
          <button
            type="button"
            onClick={() => void api.cancelSpeedTest()}
            className="h-10 shrink-0 rounded-xl bg-white/10 px-4 text-xs font-medium transition-colors hover:bg-white/15"
          >
            Durdur
          </button>
        ) : (
          <button
            type="button"
            onClick={start}
            disabled={!/^https?:\/\//.test(url.trim())}
            className="flex h-10 shrink-0 items-center gap-1.5 rounded-xl bg-accent px-4 text-xs font-semibold text-white shadow-md shadow-accent/30 transition-all enabled:hover:brightness-110 enabled:active:scale-95 disabled:opacity-40"
          >
            <Gauge size={14} /> Testi başlat
          </button>
        )}
      </div>

      {error && <p className="mt-2 px-1 text-xs text-bad">{error}</p>}

      {(running || Object.keys(rows).length > 0) && (
        <div className="mt-3 space-y-1.5 rounded-2xl bg-black/20 p-3">
          {LEVELS.map((n) => {
            const r = rows[n]
            const state = r?.state
            return (
              <div key={n} className="flex items-center gap-3 text-xs">
                <span className="w-16 shrink-0 tabular-nums text-mute">{n} bağlantı</span>
                <div className="h-2 flex-1 overflow-hidden rounded-full bg-raised">
                  <div
                    className={clsx(
                      'h-full rounded-full transition-[width] duration-300',
                      state === 'done' ? 'bg-ok' : state === 'running' ? 'bg-accent' : 'bg-bad',
                    )}
                    style={{ width: `${state === 'limited' || state === 'failed' ? 0 : ((r?.speed ?? 0) / maxSpeed) * 100}%` }}
                  />
                </div>
                <span className="w-24 shrink-0 text-right tabular-nums">
                  {!r ? (
                    <span className="text-mute">{running ? 'sırada' : '—'}</span>
                  ) : state === 'limited' ? (
                    <span className="text-bad">reddedildi</span>
                  ) : state === 'failed' ? (
                    <span className="text-bad" title={r.detail ?? ''}>hata</span>
                  ) : (
                    <span className="inline-flex items-center gap-1">
                      {state === 'running' && <Loader2 size={11} className="animate-spin text-mute" />}
                      {formatSpeed(r.speed)}
                    </span>
                  )}
                </span>
              </div>
            )
          })}
        </div>
      )}

      {rec && (
        <div className="mt-3 rounded-2xl border border-white/10 bg-white/[0.04] p-3 text-xs">
          <p className="text-mute">
            {rec.flat ? (
              <>
                <span className="text-ink">Tek bağlantı bile aynı hızı veriyor</span> ({formatBytes(rec.max)}/sn). Bu
                sunucuda ya da hattında sınır bağlantı sayısı değil; daha fazla bağlantı hız katmaz.
              </>
            ) : (
              <>
                En verimli seçenek <span className="text-ink">{rec.best} bağlantı</span> (
                {formatBytes(rec.max)}/sn'ye kadar). Daha fazlası belirgin bir kazanç getirmiyor.
              </>
            )}
            {limited && <> Sunucu {limited.connections} bağlantıyı reddetti.</>}
          </p>
          {rec.best !== current && (
            <button
              type="button"
              onClick={() => save({ connections: rec.best })}
              className="mt-2 rounded-lg bg-accent px-3 py-1.5 font-semibold text-white transition-all hover:brightness-110 active:scale-95"
            >
              {rec.best} bağlantıyı varsayılan yap
            </button>
          )}
          {rec.best === current && <p className="mt-2 text-ok">Varsayılan zaten {current} bağlantı.</p>}
        </div>
      )}
    </section>
  )
}
