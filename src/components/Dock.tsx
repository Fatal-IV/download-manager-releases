import { useRef, useState } from 'react'
import { CheckCircle2, Download as DownloadIcon, ListChecks, Moon, Plus, Settings, Sun } from 'lucide-react'
import clsx from 'clsx'
import { formatSpeed } from '../lib/format'
import { useStore } from '../store'
import type { Filter } from '../types'

const BASE = 44 // düğme yüksekliği (px)
const MAX_SCALE = 1.25
const SIGMA = 90 // büyütmenin imleçten yayılma genişliği (px)

const FILTERS: { id: Filter; label: string; icon: typeof ListChecks }[] = [
  { id: 'all', label: 'Tüm indirmeler', icon: ListChecks },
  { id: 'active', label: 'Devam edenler', icon: DownloadIcon },
  { id: 'completed', label: 'Tamamlananlar', icon: CheckCircle2 },
]

interface Item {
  key: string
  label: string
  icon: typeof ListChecks
  onClick: () => void
  active?: boolean
  count?: number
}

export function Dock({
  counts,
  onAdd,
  onSettings,
  speed,
}: {
  counts: Record<Filter, number>
  onAdd: () => void
  onSettings: () => void
  speed: number
}) {
  const filter = useStore((s) => s.filter)
  const setFilter = useStore((s) => s.setFilter)
  const theme = useStore((s) => s.theme)
  const toggleTheme = useStore((s) => s.toggleTheme)

  const items: Item[] = [
    ...FILTERS.map<Item>((f) => ({
      key: f.id,
      label: f.label,
      icon: f.icon,
      count: counts[f.id],
      active: filter === f.id,
      onClick: () => setFilter(f.id),
    })),
  ]

  const group = useRef<HTMLDivElement>(null)
  const cells = useRef<(HTMLDivElement | null)[]>([])
  const [scales, setScales] = useState<number[]>(() => items.map(() => 1))
  const [widths, setWidths] = useState<number[]>(() => items.map(() => BASE))

  // Düzen sabit kalır (ölçüm offsetLeft/offsetWidth ile, dönüşümlerden etkilenmez);
  // büyüme yalnızca transform ile yapılır, komşular kayarak yer açar.
  function onMove(e: React.MouseEvent<HTMLElement>) {
    if (!group.current) return
    const x = e.clientX - group.current.getBoundingClientRect().left
    const w: number[] = []
    const s = items.map((_, i) => {
      const el = cells.current[i]
      w[i] = el?.offsetWidth ?? BASE
      if (!el) return 1
      const d = x - (el.offsetLeft + el.offsetWidth / 2)
      return 1 + (MAX_SCALE - 1) * Math.exp(-(d * d) / (2 * SIGMA * SIGMA))
    })
    setWidths(w)
    setScales(s)
  }

  function onLeave() {
    setScales(items.map(() => 1))
  }

  const extra = scales.map((s, i) => widths[i] * (s - 1))
  const totalExtra = extra.reduce((a, b) => a + b, 0)
  const resting = scales.every((s) => s === 1)

  return (
    <div className="pointer-events-none absolute inset-x-0 top-3 z-20 px-4">
      <nav
        onMouseMove={onMove}
        onMouseLeave={onLeave}
        className="pointer-events-auto relative grid h-[66px] w-full grid-cols-[1fr_auto_1fr] items-center rounded-[26px] border border-white/25 bg-white/[0.07] px-5 shadow-[0_12px_40px_rgba(0,0,0,0.35),inset_0_1px_0_rgba(255,255,255,0.35),inset_0_-1px_0_rgba(255,255,255,0.08)] backdrop-blur-3xl backdrop-brightness-110 backdrop-saturate-200"
      >
        <div className="flex items-center gap-2.5">
          <div className="grid h-9 w-9 place-items-center rounded-xl bg-accent text-white shadow-md">
            <DownloadIcon size={18} />
          </div>
          <span className="hidden text-[15px] font-semibold tracking-tight xl:inline">
            İndirme Yöneticisi
          </span>
        </div>

        <div ref={group} className="relative flex items-center gap-2">
          {items.map((it, i) => {
            const before = extra.slice(0, i).reduce((a, b) => a + b, 0)
            const shift = before + extra[i] / 2 - totalExtra / 2
            const Icon = it.icon
            return (
              <div key={it.key} className="flex items-center gap-2">
                <div
                  ref={(el) => {
                    cells.current[i] = el
                  }}
                  className="relative"
                  style={{ height: BASE }}
                >
                  <button
                    onClick={it.onClick}
                    aria-pressed={it.active}
                    className={clsx(
                      'relative flex h-full items-center gap-2 whitespace-nowrap rounded-[14px] px-3.5 text-sm font-medium shadow-md will-change-transform',
                      it.active
                        ? 'bg-accent text-white'
                        : 'bg-white/10 text-ink hover:bg-white/15',
                    )}
                    style={{
                      transform: `translateX(${shift}px) scale(${scales[i]})`,
                      transition: resting ? 'transform 200ms cubic-bezier(.2,.8,.2,1)' : 'none',
                    }}
                  >
                    <Icon size={18} />
                    <span>{it.label}</span>
                    {it.count !== undefined && (
                      <span className="rounded-full bg-black/25 px-1.5 text-xs leading-5 tabular-nums">
                        {it.count}
                      </span>
                    )}
                  </button>

                  {it.active && (
                    <span className="pointer-events-none absolute -bottom-[9px] left-1/2 h-1 w-1 -translate-x-1/2 rounded-full bg-ink/80" />
                  )}
                </div>
              </div>
            )
          })}
        </div>

        <div className="flex items-center gap-3 justify-self-end">
          <span className="text-xs tabular-nums text-mute">
            {speed > 0 ? `↓ ${formatSpeed(speed)}` : 'Boşta'}
          </span>
          <button
            onClick={onAdd}
            title="Yeni indirme"
            aria-label="Yeni indirme"
            className="grid h-11 w-11 place-items-center rounded-[14px] bg-accent text-white shadow-md transition-transform hover:scale-105 active:scale-95"
          >
            <Plus size={20} />
          </button>
          <button
            onClick={onSettings}
            title="Ayarlar"
            aria-label="Ayarlar"
            className="grid h-11 w-11 place-items-center rounded-[14px] bg-white/10 text-ink shadow-md transition-transform hover:scale-105 hover:bg-white/15 active:scale-95"
          >
            <Settings size={18} />
          </button>
          <button
            onClick={toggleTheme}
            title={theme === 'dark' ? 'Açık tema' : 'Koyu tema'}
            aria-label={theme === 'dark' ? 'Açık temaya geç' : 'Koyu temaya geç'}
            className="grid h-11 w-11 place-items-center rounded-[14px] bg-white/10 text-ink shadow-md transition-transform hover:scale-105 hover:bg-white/15 active:scale-95"
          >
            {theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
          </button>
        </div>
      </nav>
    </div>
  )
}
