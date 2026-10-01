import { useEffect, useState } from 'react'
import { AlertTriangle, CheckCircle2, Info, X } from 'lucide-react'
import clsx from 'clsx'
import { useStore } from '../store'
import type { AppNotification, NotificationLevel } from '../types'

const TOAST_MS = 6000

const LEVEL_STYLE: Record<NotificationLevel, { icon: typeof Info; tone: string }> = {
  info: { icon: Info, tone: 'text-accent' },
  success: { icon: CheckCircle2, tone: 'text-ok' },
  danger: { icon: AlertTriangle, tone: 'text-bad' },
}

/** Sağ altta geçici bildirimler; tehlike seviyesi kapatılana kadar kalır. */
export function Toasts() {
  const toasts = useStore((s) => s.toasts)
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-40 flex w-[min(360px,calc(100vw-2rem))] flex-col gap-2">
      {toasts.map((n) => (
        <Toast key={n.id} n={n} />
      ))}
    </div>
  )
}

function Toast({ n }: { n: AppNotification }) {
  const dismiss = useStore((s) => s.dismissToast)
  const open = useStore((s) => s.openNotification)
  const gone = useStore((s) => !!n.downloadId && !s.downloads[n.downloadId])
  const [hover, setHover] = useState(false)
  const { icon: Icon, tone } = LEVEL_STYLE[n.level]

  // Üzerine gelince süre durur; ayrılınca yeniden 6 sn başlar. Tehlike seviyesi kendiliğinden kapanmaz.
  useEffect(() => {
    if (n.level === 'danger' || hover) return
    const t = setTimeout(() => dismiss(n.id), TOAST_MS)
    return () => clearTimeout(t)
  }, [n.id, n.level, hover, dismiss])

  const firstLine = n.body.split('\n')[0]
  return (
    <div
      role={n.level === 'danger' ? 'alert' : 'status'}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className={clsx(
        'toast-in pointer-events-auto flex items-start gap-3 rounded-2xl border bg-panel p-3 shadow-[0_12px_40px_rgba(0,0,0,0.35)]',
        n.level === 'danger' ? 'border-bad/60' : 'border-line',
      )}
    >
      <Icon size={18} className={clsx('mt-0.5 shrink-0', tone)} />
      <button type="button" onClick={() => open(n)} className="min-w-0 flex-1 text-left">
        <span className="block truncate text-sm font-medium">{n.title}</span>
        <span className="mt-0.5 block break-words text-xs text-mute [overflow-wrap:anywhere] line-clamp-3">{firstLine}</span>
        {gone && <span className="mt-1 block text-[11px] text-mute">İndirme artık yok</span>}
      </button>
      <button
        type="button"
        onClick={() => dismiss(n.id)}
        aria-label="Kapat"
        className="grid h-6 w-6 shrink-0 place-items-center rounded-lg text-mute hover:bg-raised hover:text-ink"
      >
        <X size={14} />
      </button>
    </div>
  )
}
