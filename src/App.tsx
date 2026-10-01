import { useEffect, useMemo, useState } from 'react'
import { Inbox, Search, X } from 'lucide-react'
import { AddDialog } from './components/AddDialog'
import { DownloadRow } from './components/DownloadRow'
import { Dock } from './components/Dock'
import { NotificationPanel } from './components/NotificationPanel'
import { Toasts } from './components/Toasts'
import { SettingsDialog } from './components/SettingsDialog'
import { UpdateBanner } from './components/UpdateBanner'
import { WhatsNewDialog } from './components/WhatsNewDialog'
import { startUpdateChecks, takeWhatsNew, type WhatsNew } from './lib/updater'
import { useStore } from './store'
import { CATEGORIES, type Download, type Filter, type SortKey } from './types'

const SORTS: { id: SortKey; label: string }[] = [
  { id: 'newest', label: 'En yeni' },
  { id: 'oldest', label: 'En eski' },
  { id: 'name', label: 'Ada göre' },
  { id: 'size', label: 'Boyuta göre' },
  { id: 'progress', label: 'İlerlemeye göre' },
  { id: 'speed', label: 'Hıza göre' },
]

const progressOf = (d: Download) => (d.totalBytes ? d.downloadedBytes / d.totalBytes : 0)

function compare(sort: SortKey): (a: Download, b: Download) => number {
  switch (sort) {
    case 'oldest':
      return (a, b) => a.addedAt - b.addedAt
    case 'name':
      return (a, b) => a.filename.localeCompare(b.filename, 'tr')
    case 'size':
      return (a, b) => b.totalBytes - a.totalBytes
    case 'progress':
      return (a, b) => progressOf(b) - progressOf(a)
    case 'speed':
      return (a, b) => b.speed - a.speed
    default:
      return (a, b) => b.addedAt - a.addedAt
  }
}

export default function App() {
  const downloads = useStore((s) => s.downloads)
  const filter = useStore((s) => s.filter)
  const query = useStore((s) => s.query)
  const setQuery = useStore((s) => s.setQuery)
  const sort = useStore((s) => s.sort)
  const setSort = useStore((s) => s.setSort)
  const init = useStore((s) => s.init)
  const [adding, setAdding] = useState(false)
  const category = useStore((s) => s.category)
  const setCategory = useStore((s) => s.setCategory)
  const [settingsOpen, setSettingsOpen] = useState(false)

  useEffect(() => init(), [init])

  // Bildirimden gidilen indirme: tür süzgecini kaldır, satırı ortala ve 2 sn vurgula.
  const focusId = useStore((s) => s.focusId)
  const clearFocus = useStore((s) => s.clearFocus)
  useEffect(() => {
    if (!focusId) return
    const scroll = requestAnimationFrame(() =>
      document.getElementById(`dl-${focusId}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' }),
    )
    const done = setTimeout(clearFocus, 2000)
    return () => {
      cancelAnimationFrame(scroll)
      clearTimeout(done)
    }
  }, [focusId, clearFocus])
  useEffect(() => startUpdateChecks(), [])
  const [whatsNew, setWhatsNew] = useState<WhatsNew | null>(null)
  useEffect(() => {
    void takeWhatsNew().then(setWhatsNew)
  }, [])

  const all = useMemo(
    () => Object.values(downloads),
    [downloads],
  )

  const counts = useMemo<Record<Filter, number>>(
    () => ({
      all: all.length,
      active: all.filter((d) => d.status === 'downloading' || d.status === 'paused' || d.status === 'queued').length,
      completed: all.filter((d) => d.status === 'completed').length,
    }),
    [all],
  )

  const needle = query.trim().toLocaleLowerCase('tr')
  const visible = all
    .filter((d) =>
      filter === 'all' ? true : filter === 'completed' ? d.status === 'completed' : d.status !== 'completed',
    )
    .filter((d) => !category || d.category === category)
    .filter((d) => !needle || d.filename.toLocaleLowerCase('tr').includes(needle) || d.url.toLowerCase().includes(needle))
    .sort(compare(sort))
  const totalSpeed = all.reduce((sum, d) => sum + d.speed, 0)

  return (
    <div className="relative h-full">
      <Dock counts={counts} onAdd={() => setAdding(true)} onSettings={() => setSettingsOpen(true)} speed={totalSpeed} />

      {/* İçerik dock'un arkasından kayar; cam efekti bu sayede görünür. */}
      <main className="h-full overflow-y-auto px-6 pb-8 pt-28">
        <div className="mx-auto max-w-4xl">
          <header className="mb-5 flex flex-wrap items-center gap-3">
            <h1 className="mr-auto text-lg font-semibold tracking-tight">
              {filter === 'all' ? 'Tüm indirmeler' : filter === 'active' ? 'Devam edenler' : 'Tamamlananlar'}
            </h1>
            <label className="flex h-9 w-56 items-center gap-2 rounded-xl border border-line bg-panel px-3 text-sm focus-within:border-accent/60">
              <Search size={15} className="shrink-0 text-mute" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Ara…"
                aria-label="İndirmelerde ara"
                className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-mute"
              />
              {query && (
                <button type="button" onClick={() => setQuery('')} aria-label="Aramayı temizle" className="text-mute hover:text-ink">
                  <X size={14} />
                </button>
              )}
            </label>
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              aria-label="Türe göre süz"
              className="h-9 rounded-xl border border-line bg-panel px-3 text-sm outline-none focus:border-accent/60"
            >
              <option value="">Tüm türler</option>
              {CATEGORIES.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
            <select
              value={sort}
              onChange={(e) => setSort(e.target.value as SortKey)}
              aria-label="Sırala"
              className="h-9 rounded-xl border border-line bg-panel px-3 text-sm outline-none focus:border-accent/60"
            >
              {SORTS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </header>

          {visible.length === 0 ? (
            <div className="grid place-items-center py-24 text-center text-mute">
              <div>
                <Inbox size={40} className="mx-auto mb-3 opacity-50" />
                <p className="text-sm">{needle || category ? 'Süzgeçle eşleşen indirme yok.' : 'Burada henüz indirme yok.'}</p>
                {!needle && !category && (
                  <button onClick={() => setAdding(true)} className="mt-2 text-sm text-accent hover:underline">
                    İlk indirmeni ekle
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              {visible.map((d) => (
                <DownloadRow key={d.id} d={d} />
              ))}
            </div>
          )}
        </div>
      </main>

      <NotificationPanel />
      <Toasts />
      <UpdateBanner />
      {whatsNew && <WhatsNewDialog info={whatsNew} onClose={() => setWhatsNew(null)} />}
      {adding && <AddDialog onClose={() => setAdding(false)} />}
      {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
    </div>
  )
}
