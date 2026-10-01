import { ChevronDown, File, FolderOpen, Hash, Pause, Play, ShieldAlert, ShieldCheck, Trash2 } from 'lucide-react'
import { useState } from 'react'
import clsx from 'clsx'
import { formatBytes, formatEta, formatSpeed } from '../lib/format'
import { useStore } from '../store'
import type { Download, DownloadStatus, Segment } from '../types'

const STATUS_LABEL: Record<DownloadStatus, string> = {
  queued: 'Sırada',
  downloading: 'İndiriliyor',
  paused: 'Duraklatıldı',
  completed: 'Tamamlandı',
  failed: 'Hata',
}

const STATUS_COLOR: Record<DownloadStatus, string> = {
  queued: 'bg-mute',
  downloading: 'bg-accent',
  paused: 'bg-warn',
  completed: 'bg-ok',
  failed: 'bg-bad',
}

function statusLabel(d: Download): string {
  if (d.status === 'paused' && d.autoPaused === 'network') return 'Ağ bekleniyor'
  if (d.status === 'paused' && d.autoPaused === 'schedule') return 'Zamanlayıcı bekliyor'
  return STATUS_LABEL[d.status]
}

export function DownloadRow({ d }: { d: Download }) {
  const pause = useStore((s) => s.pause)
  const resume = useStore((s) => s.resume)
  const remove = useStore((s) => s.remove)
  const open = useStore((s) => s.open)
  const reveal = useStore((s) => s.reveal)
  const hash = useStore((s) => s.hash)
  const scan = useStore((s) => s.scan)
  const focused = useStore((s) => s.focusId === d.id)

  const [open_, setOpenDetails] = useState(false)
  const pct = d.totalBytes ? Math.min(100, (d.downloadedBytes / d.totalBytes) * 100) : 0
  const canPause = d.status === 'downloading'
  const canResume = d.status === 'paused' || d.status === 'failed'

  return (
    <div
      id={`dl-${d.id}`}
      className={clsx(
        'group rounded-xl border bg-panel p-4 transition-colors hover:border-accent/40',
        focused ? 'border-accent ring-2 ring-accent/60' : 'border-line',
      )}
    >
      <div className="flex items-center gap-3">
        <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-raised text-mute">
          <File size={18} />
        </div>

        <div className="min-w-0 flex-1">
          <div
            className={clsx('truncate text-sm font-medium', d.status === 'completed' && 'cursor-pointer hover:text-accent')}
            title={d.filename}
            onDoubleClick={() => d.status === 'completed' && open(d.id)}
          >
            {d.filename}
          </div>
          <div className="mt-0.5 flex items-center gap-2 text-xs text-mute">
            <span className={clsx('h-1.5 w-1.5 rounded-full', STATUS_COLOR[d.status])} />
            <span>{statusLabel(d)}</span>
            <span>·</span>
            <span className="tabular-nums">
              {formatBytes(d.downloadedBytes)} / {formatBytes(d.totalBytes)}
            </span>
            {d.status === 'downloading' && (
              <>
                <span>·</span>
                <span className="tabular-nums">{formatSpeed(d.speed)}</span>
                <span>·</span>
                <span className="tabular-nums">
                  {formatEta(d.totalBytes - d.downloadedBytes, d.speed)}
                </span>
              </>
            )}
          </div>
        </div>

        <div className="flex items-center gap-1 opacity-70 transition-opacity group-hover:opacity-100">
          {d.status === 'completed' && (
            <>
              <IconButton label="SHA-256 hesapla" onClick={() => hash(d.id)}>
                <Hash size={16} />
              </IconButton>
              <IconButton label="Windows Defender ile tara" onClick={() => scan(d.id)}>
                <ShieldCheck size={16} />
              </IconButton>
            </>
          )}
          {d.status === 'completed' && (
            <IconButton label="Klasörde göster" onClick={() => reveal(d.id)}>
              <FolderOpen size={16} />
            </IconButton>
          )}
          {canPause && (
            <IconButton label="Duraklat" onClick={() => pause(d.id)}>
              <Pause size={16} />
            </IconButton>
          )}
          {canResume && (
            <IconButton label="Devam et" onClick={() => resume(d.id)}>
              <Play size={16} />
            </IconButton>
          )}
          <IconButton label="Kaldır" onClick={() => remove(d.id)}>
            <Trash2 size={16} />
          </IconButton>
        </div>
      </div>

      {d.status === 'downloading' && <SpeedGraph id={d.id} />}

      {d.status === 'completed' && <Integrity d={d} />}

      {d.error && <p className="mt-2 truncate text-xs text-bad" title={d.error}>{d.error}</p>}

      {d.segments.length > 1 && (
        <>
          {d.status !== 'completed' && <SegmentBars segments={d.segments} active={d.status === 'downloading'} />}
          <button
            onClick={() => setOpenDetails(!open_)}
            className="mt-2 flex items-center gap-1 text-[11px] text-mute transition-colors hover:text-ink"
          >
            <ChevronDown size={12} className={clsx('transition-transform', open_ && 'rotate-180')} />
            Bağlantı ayrıntıları
          </button>
          {open_ && <SegmentDetails segments={d.segments} />}
        </>
      )}

      <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-raised">
        <div
          className={clsx('h-full rounded-full transition-[width] duration-200', STATUS_COLOR[d.status])}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  )
}

/** Bitmiş dosyanın SHA-256 özeti, doğrulama ve virüs taraması sonucu. */
function Integrity({ d }: { d: Download }) {
  if (!d.sha256 && !d.scan) return null
  const copy = () => void navigator.clipboard?.writeText(d.sha256 ?? '').catch(() => {})
  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px]">
      {d.sha256 && (
        <button type="button" onClick={copy} title="Kopyalamak için tıkla" className="flex min-w-0 items-center gap-1.5 text-mute transition-colors hover:text-ink">
          <Hash size={12} className="shrink-0" />
          <span className="truncate font-mono">SHA-256 {d.sha256.slice(0, 16)}…{d.sha256.slice(-8)}</span>
        </button>
      )}
      {d.verified === true && <span className="flex items-center gap-1 text-ok"><ShieldCheck size={12} /> Özet doğrulandı</span>}
      {d.verified === false && <span className="flex items-center gap-1 text-bad"><ShieldAlert size={12} /> Özet uyuşmuyor, dosya bozuk ya da değişmiş olabilir</span>}
      {d.scan === 'scanning' && <span className="text-mute">Taranıyor…</span>}
      {d.scan === 'clean' && <span className="flex items-center gap-1 text-ok"><ShieldCheck size={12} /> Virüs taraması temiz</span>}
      {d.scan === 'threat' && <span className="flex items-center gap-1 text-bad"><ShieldAlert size={12} /> Tehdit bulundu, dosyayı açma</span>}
    </div>
  )
}

/** Son ~60 saniyenin hız grafiği. */
function SpeedGraph({ id }: { id: string }) {
  const history = useStore((s) => s.speedHistory[id])
  if (!history || history.length < 2) return null
  const W = 300
  const H = 36
  const max = Math.max(...history, 1)
  const step = W / (history.length - 1)
  const pts = history.map((v, i) => `${(i * step).toFixed(1)},${(H - (v / max) * (H - 4) - 2).toFixed(1)}`)
  return (
    <div className="mt-3 flex items-end gap-3">
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="h-9 min-w-0 flex-1 text-accent" role="img" aria-label="Hız grafiği">
        <polygon points={`0,${H} ${pts.join(' ')} ${W},${H}`} fill="currentColor" opacity="0.15" />
        <polyline points={pts.join(' ')} fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
      </svg>
      <span className="shrink-0 text-[11px] tabular-nums text-mute">En yüksek {formatSpeed(max)}</span>
    </div>
  )
}

function IconButton({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      title={label}
      aria-label={label}
      onClick={onClick}
      className="grid h-8 w-8 place-items-center rounded-lg text-mute transition-colors hover:bg-raised hover:text-ink"
    >
      {children}
    </button>
  )
}

/** IDM'deki gibi her parçanın kendi ilerlemesi; genişlik parçanın boyutuyla orantılıdır. */
function SegmentBars({ segments, active }: { segments: Segment[]; active: boolean }) {
  return (
    <div className="mt-3">
      <div className="mb-1 text-[11px] text-mute">{segments.length} parça</div>
      <div className="flex gap-1">
        {segments.map((g, i) => {
          const p = g.size ? (g.downloaded / g.size) * 100 : 0
          return (
            <div
              key={i}
              className="h-1.5 overflow-hidden rounded-full bg-raised"
              style={{ flex: g.size }}
              title={`Parça ${i + 1}: ${formatBytes(g.downloaded)} / ${formatBytes(g.size)} (%${Math.floor(p)})`}
            >
              <div
                className={clsx('h-full rounded-full transition-[width] duration-200', p >= 100 ? 'bg-ok' : active ? 'bg-accent' : 'bg-warn')}
                style={{ width: `${p}%` }}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}

function hostOf(remote: string | null): string {
  if (!remote) return ''
  const i = remote.lastIndexOf(':')
  return i > 0 ? remote.slice(0, i) : remote
}

/** Parçaların gerçekten ayrı Range istekleriyle indiğinin kanıtı. */
function SegmentDetails({ segments }: { segments: Segment[] }) {
  const answered = segments.filter((g) => g.httpStatus > 0)
  const partial = answered.filter((g) => g.httpStatus === 206).length
  const hosts = new Set(answered.map((g) => hostOf(g.remote)).filter(Boolean))
  const sources = [...new Set(answered.map((g) => g.source).filter(Boolean))]
  return (
    <div className="mt-2 rounded-lg border border-line bg-raised/50 p-3 text-[11px]">
      <p className="mb-2 text-mute">
        <span className="text-ink">{segments.length}</span> parça ·{' '}
        <span className="text-ink">{partial}</span> tanesi sunucudan HTTP 206 (kısmi içerik) aldı ·{' '}
        <span className="text-ink">{hosts.size}</span> sunucu adresi
        {hosts.size ? ` (${[...hosts].join(', ')})` : ''}
        {sources.length > 1 && (
          <>
            {' '}· <span className="text-ink">{sources.length}</span> kaynak adres (yansı)
          </>
        )}
      </p>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[520px] tabular-nums">
          <thead className="text-left text-mute">
            <tr>
              <th className="pb-1 pr-3 font-normal">#</th>
              <th className="pb-1 pr-3 font-normal">İstenen aralık</th>
              <th className="pb-1 pr-3 font-normal">Yanıt</th>
              <th className="pb-1 pr-3 font-normal">Sunucunun onayı</th>
              <th className="pb-1 pr-3 font-normal">Sunucu adresi</th>
              {sources.length > 1 && <th className="pb-1 pr-3 font-normal">Kaynak</th>}
              <th className="pb-1 text-right font-normal">İnen</th>
            </tr>
          </thead>
          <tbody>
            {segments.map((g, i) => (
              <tr key={i} className="border-t border-line/60">
                <td className="py-1 pr-3">{i + 1}</td>
                <td className="py-1 pr-3">{g.range || '—'}</td>
                <td className={clsx('py-1 pr-3', g.httpStatus === 206 ? 'text-ok' : g.httpStatus ? 'text-bad' : 'text-mute')}>
                  {g.httpStatus || '—'}
                </td>
                <td className="py-1 pr-3">{g.contentRange ?? '—'}</td>
                <td className="py-1 pr-3">{g.remote ?? '—'}</td>
                {sources.length > 1 && (
                  <td className="max-w-[160px] truncate py-1 pr-3" title={g.source}>
                    {g.source ? new URL(g.source).host : '—'}
                  </td>
                )}
                <td className="py-1 text-right">
                  {formatBytes(g.downloaded)} / {formatBytes(g.size)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
