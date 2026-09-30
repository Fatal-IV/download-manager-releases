import { create } from 'zustand'
import { api } from './lib/api'
import type { AddExtras, Download, Filter, Settings, SortKey } from './types'

type Theme = 'dark' | 'light'

interface State {
  downloads: Record<string, Download>
  filter: Filter
  query: string
  sort: SortKey
  /** İndirme başına son hız örnekleri (bayt/sn), hız grafiği için. */
  speedHistory: Record<string, number[]>
  theme: Theme
  settings: Settings
  saveSettings: (patch: Partial<Settings>) => void
  init: () => () => void
  add: (url: string, connections: number, mirrors?: string[], extras?: AddExtras) => Promise<void>
  hash: (id: string) => void
  scan: (id: string) => void
  pause: (id: string) => void
  resume: (id: string) => void
  remove: (id: string) => void
  open: (id: string) => void
  reveal: (id: string) => void
  setFilter: (f: Filter) => void
  setQuery: (q: string) => void
  setSort: (k: SortKey) => void
  toggleTheme: () => void
}

const savedTheme = (localStorage.getItem('theme') as Theme | null) ?? 'dark'
document.documentElement.dataset.theme = savedTheme

const HISTORY_LEN = 60
const SAMPLE_MS = 1000
const lastSample: Record<string, number> = {}

export const useStore = create<State>((set, get) => ({
  downloads: {},
  filter: 'all',
  query: '',
  sort: (localStorage.getItem('sort') as SortKey | null) ?? 'newest',
  speedHistory: {},
  theme: savedTheme,
  settings: { connections: 8, spreadIps: true, maxConcurrent: 3, downloadDir: '', speedLimitKbps: 0, scheduleEnabled: false, scheduleStart: 120, scheduleEnd: 480, notifyOnComplete: true, sortByCategory: false, scanOnComplete: false },

  saveSettings(patch) {
    const next = { ...get().settings, ...patch }
    set({ settings: next })
    api
      .setSettings(next)
      .then((saved) => set({ settings: saved }))
      .catch(() => {})
  },

  init() {
    api
      .getSettings()
      .then((settings) => set({ settings }))
      .catch(() => {})
    api.list().then((list) =>
      set({ downloads: Object.fromEntries(list.map((d) => [d.id, d])) }),
    )
    return api.subscribe(
      (d) =>
        set((s) => {
          const next: Partial<State> = { downloads: { ...s.downloads, [d.id]: d } }
          const now = Date.now()
          if (d.status === 'downloading' && now - (lastSample[d.id] ?? 0) >= SAMPLE_MS) {
            lastSample[d.id] = now
            next.speedHistory = {
              ...s.speedHistory,
              [d.id]: [...(s.speedHistory[d.id] ?? []), d.speed].slice(-HISTORY_LEN),
            }
          }
          return next
        }),
      (id) =>
        set((s) => {
          const { [id]: _removed, ...rest } = s.downloads
          const { [id]: _h, ...history } = s.speedHistory
          delete lastSample[id]
          return { downloads: rest, speedHistory: history }
        }),
    )
  },

  async add(url, connections, mirrors, extras) {
    await api.add(url, connections, mirrors, extras)
  },
  hash: (id) => void api.hash(id).catch(() => {}),
  scan: (id) => void api.scan(id).catch(() => {}),
  pause: (id) => void api.pause(id),
  resume: (id) => void api.resume(id),
  remove: (id) => void api.remove(id),
  open: (id) => void api.open(id).catch(() => {}),
  reveal: (id) => void api.reveal(id).catch(() => {}),
  setFilter: (filter) => set({ filter }),
  setQuery: (query) => set({ query }),
  setSort(sort) {
    localStorage.setItem('sort', sort)
    set({ sort })
  },

  toggleTheme() {
    const theme: Theme = get().theme === 'dark' ? 'light' : 'dark'
    document.documentElement.dataset.theme = theme
    localStorage.setItem('theme', theme)
    set({ theme })
  },
}))
