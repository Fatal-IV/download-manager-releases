import { create } from 'zustand'
import { api } from './lib/api'
import type { AddExtras, AppNotification, Download, Filter, Settings, SettingsTab, SortKey } from './types'

type Theme = 'dark' | 'light'

const LAST_DONE_KEY = 'dm.lastCompletedId'

function readLastDone(): string | null {
  try {
    return localStorage.getItem(LAST_DONE_KEY)
  } catch {
    return null
  }
}

function writeLastDone(id: string | null) {
  try {
    if (id) localStorage.setItem(LAST_DONE_KEY, id)
    else localStorage.removeItem(LAST_DONE_KEY)
  } catch {
    /* depolama kapalıysa yalnızca oturum boyunca tutulur */
  }
}

/** Kayıtlı kimlik hâlâ tamamlanmış bir indirmeyse o; değilse en son eklenen tamamlanmış indirme. */
function resolveLastDone(downloads: Record<string, Download>, saved: string | null): string | null {
  if (saved && downloads[saved]?.status === 'completed') return saved
  let best: Download | null = null
  for (const d of Object.values(downloads)) {
    if (d.status === 'completed' && (!best || d.addedAt > best.addedAt)) best = d
  }
  return best?.id ?? null
}

interface State {
  downloads: Record<string, Download>
  filter: Filter
  query: string
  /** Tür süzgeci; boş = tüm türler. */
  category: string
  setCategory: (c: string) => void
  sort: SortKey
  /** İndirme başına son hız örnekleri (bayt/sn), hız grafiği için. */
  speedHistory: Record<string, number[]>
  theme: Theme
  settings: Settings
  /** Bildirim geçmişi, yeni üstte. */
  notifications: AppNotification[]
  /** Ekranda görünen toast bildirimleri (en çok 3). */
  toasts: AppNotification[]
  panelOpen: boolean
  /** Ayarlar sayfası açık mı; açıkken Dock'ta kategoriler görünür. */
  settingsOpen: boolean
  settingsTab: SettingsTab
  toggleSettings: () => void
  closeSettings: () => void
  setSettingsTab: (tab: SettingsTab) => void
  /** Bildirimden gidilen indirme; satır bunu görünce vurgulanır. */
  focusId: string | null
  /** En son tamamlanan indirme; satırı hareketli çerçeveyle vurgulanır. */
  lastCompletedId: string | null
  setPanelOpen: (open: boolean) => void
  markRead: (id: number) => void
  markAllRead: () => void
  removeNotification: (id: number) => void
  clearNotifications: () => void
  dismissToast: (id: number) => void
  openNotification: (n: AppNotification) => void
  focusDownload: (id: string) => void
  clearFocus: () => void
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
  settings: { connections: 8, spreadIps: true, maxConcurrent: 3, downloadDir: '', speedLimitKbps: 0, scheduleEnabled: false, scheduleStart: 120, scheduleEnd: 480, notifyOnComplete: true, sortByCategory: false, scanOnComplete: false, startMinimized: true, notifyKinds: { downloadComplete: true, downloadFailed: true, hash: true, verify: true, scan: true } },

  notifications: [],
  toasts: [],
  panelOpen: false,
  settingsOpen: false,
  settingsTab: 'general',
  focusId: null,
  lastCompletedId: null,
  category: '',
  setCategory: (category) => set({ category }),

  saveSettings(patch) {
    const next = { ...get().settings, ...patch }
    set({ settings: next })
    api
      .setSettings(next)
      .then((saved) => set({ settings: saved }))
      .catch(() => {})
  },

  setPanelOpen: (panelOpen) => set({ panelOpen }),
  toggleSettings: () => set((s) => ({ settingsOpen: !s.settingsOpen, panelOpen: false })),
  closeSettings: () => set({ settingsOpen: false }),
  setSettingsTab: (settingsTab) => set({ settingsTab }),
  markRead(id) {
    set((s) => ({
      notifications: s.notifications.map((n) => (n.id === id ? { ...n, read: true } : n)),
      toasts: s.toasts.map((n) => (n.id === id ? { ...n, read: true } : n)),
    }))
    void api.markNotificationRead(id).catch(() => {})
  },
  markAllRead() {
    set((s) => ({ notifications: s.notifications.map((n) => ({ ...n, read: true })) }))
    void api.markAllNotificationsRead().catch(() => {})
  },
  removeNotification(id) {
    set((s) => ({
      notifications: s.notifications.filter((n) => n.id !== id),
      toasts: s.toasts.filter((n) => n.id !== id),
    }))
    void api.deleteNotification(id).catch(() => {})
  },
  clearNotifications() {
    set({ notifications: [], toasts: [] })
    void api.clearNotifications().catch(() => {})
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((n) => n.id !== id) })),
  focusDownload(id) {
    set({ filter: 'all', query: '', category: '', focusId: id })
  },
  clearFocus: () => set({ focusId: null }),
  openNotification(n) {
    get().markRead(n.id)
    set((s) => ({ panelOpen: false, toasts: s.toasts.filter((t) => t.id !== n.id) }))
    // Silinmiş indirmeye gidilemez; yalnızca okundu işaretlenir.
    if (n.downloadId && get().downloads[n.downloadId]) get().focusDownload(n.downloadId)
  },

  init() {
    api
      .listNotifications()
      .then((notifications) => set({ notifications }))
      .catch(() => {})
    const offNotifications = api.subscribeNotifications((n) =>
      set((s) => ({
        notifications: [n, ...s.notifications.filter((x) => x.id !== n.id)].slice(0, 100),
        toasts: [n, ...s.toasts.filter((x) => x.id !== n.id)].slice(0, 3),
      })),
    )
    api
      .getSettings()
      .then((settings) => set({ settings }))
      .catch(() => {})
    api.list().then((list) =>
      set(() => {
        const downloads = Object.fromEntries(list.map((d) => [d.id, d]))
        return { downloads, lastCompletedId: resolveLastDone(downloads, readLastDone()) }
      }),
    )
    const offDownloads = api.subscribe(
      (d) =>
        set((s) => {
          const next: Partial<State> = { downloads: { ...s.downloads, [d.id]: d } }
          const before = s.downloads[d.id]
          if (d.status === 'completed' && before && before.status !== 'completed') {
            next.lastCompletedId = d.id
            writeLastDone(d.id)
          }
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
          const lastCompletedId = resolveLastDone(rest, s.lastCompletedId === id ? null : s.lastCompletedId)
          writeLastDone(lastCompletedId)
          return { downloads: rest, speedHistory: history, lastCompletedId }
        }),
    )
    return () => {
      offDownloads()
      offNotifications()
    }
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
