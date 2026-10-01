import { useEffect, useState } from 'react'
import { AlertTriangle, Check, CheckCircle2, Copy, Info, Trash2 } from 'lucide-react'
import clsx from 'clsx'
import { formatRelative } from '../lib/format'
import { useStore } from '../store'
import type { AppNotification, NotificationLevel } from '../types'

const LEVEL_STYLE: Record<NotificationLevel, { icon: typeof Info; tone: string }> = {
  info: { icon: Info, tone: 'text-accent' },
  success: { icon: CheckCircle2, tone: 'text-ok' },
  danger: { icon: AlertTriangle, tone: 'text-bad' },
}

/** Çana basınca açılan bildirim geçmişi. */
export function NotificationPanel() {
  const open = useStore((s) => s.panelOpen)
  const setOpen = useStore((s) => s.setPanelOpen)
  const items = useStore((s) => s.notifications)
  const downloads = useStore((s) => s.downloads)
  const markRead = useStore((s) => s.markRead)
  const markAllRead = useStore((s) => s.markAllRead)
  const remove = useStore((s) => s.removeNotification)
  const clearAll = useStore((s) => s.clearNotifications)
  const openNotification = useStore((s) => s.openNotification)
  const unread = items.filter((n) => !n.read).length

  // Esc ve dışarı tıklama paneli kapatır; çanın kendisi kendi düğmesiyle açıp kapatır.
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    const onDown = (e: MouseEvent) => {
      const el = e.target as HTMLElement
      if (!el.closest('[data-notification-panel]') && !el.closest('[data-notification-bell]')) setOpen(false)
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('mousedown', onDown)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('mousedown', onDown)
    }
  }, [open, setOpen])

  if (!open) return null

  return (
    <section
      data-notification-panel
      aria-label="Bildirimler"
      className="fixed right-4 top-[92px] z-30 flex max-h-[70vh] w-[min(400px,calc(100vw-2rem))] flex-col overflow-hidden rounded-2xl border border-line bg-panel shadow-[0_12px_40px_rgba(0,0,0,0.35)]"
    >
      <header className="flex items-center gap-2 border-b border-line px-4 py-3">
        <h2 className="mr-auto text-sm font-semibold">Bildirimler</h2>
        <button
          type="button"
          onClick={markAllRead}
          disabled={unread === 0}
          className="rounded-lg px-2 py-1 text-xs text-accent hover:bg-raised disabled:pointer-events-none disabled:opacity-40"
        >
          Tümünü okundu olarak işaretle
        </button>
        <button
          type="button"
          onClick={clearAll}
          disabled={items.length === 0}
          className="rounded-lg px-2 py-1 text-xs text-mute hover:bg-raised hover:text-ink disabled:pointer-events-none disabled:opacity-40"
        >
          Tümünü temizle
        </button>
      </header>

      {items.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-mute">Henüz bildirim yok</p>
      ) : (
        <ul className="min-h-0 flex-1 divide-y divide-line overflow-y-auto">
          {items.map((n) => (
            <Row
              key={n.id}
              n={n}
              gone={!!n.downloadId && !downloads[n.downloadId]}
              onOpen={() => openNotification(n)}
              onRead={() => markRead(n.id)}
              onRemove={() => remove(n.id)}
            />
          ))}
        </ul>
      )}
    </section>
  )
}

function Row({
  n,
  gone,
  onOpen,
  onRead,
  onRemove,
}: {
  n: AppNotification
  gone: boolean
  onOpen: () => void
  onRead: () => void
  onRemove: () => void
}) {
  const { icon: Icon, tone } = LEVEL_STYLE[n.level]
  const [name, ...rest] = n.body.split('\n')
  const digest = n.kind === 'hash' ? rest.join('\n') : ''
  const body = n.kind === 'hash' ? name : n.body
  const [copied, setCopied] = useState(false)
  const copy = () => {
    void navigator.clipboard
      ?.writeText(digest)
      .then(() => {
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      })
      .catch(() => {})
  }

  return (
    <li className={clsx('flex gap-3 px-4 py-3', !n.read && 'bg-white/[0.04]')}>
      <Icon size={18} className={clsx('mt-0.5 shrink-0', tone)} />
      <div className="min-w-0 flex-1">
        <button type="button" onClick={onOpen} className="block w-full text-left">
          <span className="flex items-center gap-2">
            <span className="truncate text-sm font-medium">{n.title}</span>
            {!n.read && <span aria-label="Okunmadı" className="h-2 w-2 shrink-0 rounded-full bg-accent" />}
          </span>
          <span className="mt-0.5 block break-words text-xs text-mute">{body}</span>
        </button>
        {digest && (
          <div className="mt-1.5 flex items-start gap-2">
            <code className="min-w-0 flex-1 select-text break-all rounded-lg bg-black/20 px-2 py-1 font-mono text-[11px] leading-4">
              {digest}
            </code>
            <button
              type="button"
              onClick={copy}
              className="flex shrink-0 items-center gap-1 rounded-lg px-2 py-1 text-xs text-accent hover:bg-raised"
            >
              {copied ? <Check size={12} /> : <Copy size={12} />}
              {copied ? 'Kopyalandı' : 'Kopyala'}
            </button>
          </div>
        )}
        {gone && <p className="mt-1 text-[11px] text-mute">İndirme artık yok</p>}
        <div className="mt-1 flex items-center gap-3 text-[11px] text-mute">
          <time>{formatRelative(n.createdAt)}</time>
          {!n.read && (
            <button type="button" onClick={onRead} className="text-accent hover:underline">
              Okundu olarak işaretle
            </button>
          )}
        </div>
      </div>
      <button
        type="button"
        onClick={onRemove}
        title="Sil"
        aria-label="Bildirimi sil"
        className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-mute hover:bg-raised hover:text-ink"
      >
        <Trash2 size={14} />
      </button>
    </li>
  )
}

