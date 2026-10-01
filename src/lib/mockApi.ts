import type { Api, AppNotification, Download, NotificationKind, NotificationLevel, Settings, SpeedUpdate } from '../types'
import { filenameFromUrl } from './format'

/**
 * Rust motoru hazır olana kadar arayüzü geliştirmek için simüle edilmiş motor.
 * Aşama 2'de Tauri komutlarına bağlanan gerçek uygulama ile değiştirilecek.
 */
export function createMockApi(): Api {
  const items = new Map<string, Download>()
  const updateSubs = new Set<(d: Download) => void>()
  const removeSubs = new Set<(id: string) => void>()
  const emit = (d: Download) => updateSubs.forEach((f) => f({ ...d }))

  // Bildirimler yalnızca bellekte tutulur; kalıcılık gerçek motorda.
  let notifications: AppNotification[] = []
  let nextNotifId = 1
  const notifSubs = new Set<(n: AppNotification) => void>()
  const pushNotification = (
    kind: NotificationKind,
    level: NotificationLevel,
    title: string,
    body: string,
    downloadId: string,
  ) => {
    const n: AppNotification = { id: nextNotifId++, kind, level, title, body, downloadId, createdAt: Date.now(), read: false }
    notifications = [n, ...notifications].slice(0, 100)
    notifSubs.forEach((f) => f({ ...n }))
  }

  setInterval(() => {
    for (const d of items.values()) {
      if (d.status !== 'downloading') continue
      const speed = (2 + Math.random() * 6) * 1024 * 1024 * Math.min(d.connections, 8) * 0.25
      d.speed = speed
      d.downloadedBytes = Math.min(d.totalBytes, d.downloadedBytes + speed / 4)
      if (d.downloadedBytes >= d.totalBytes) {
        d.status = 'completed'
        d.speed = 0
      }
      emit(d)
    }
  }, 250)

  let settings: Settings = { connections: 8, spreadIps: true, maxConcurrent: 3, downloadDir: 'C:\\Users\\kullanici\\Downloads', speedLimitKbps: 0, scheduleEnabled: false, scheduleStart: 120, scheduleEnd: 480, notifyOnComplete: true, sortByCategory: false, scanOnComplete: false, startMinimized: true, notifyKinds: { downloadComplete: true, downloadFailed: true, hash: true, verify: true, scan: true } }

  const speedSubs = new Set<(u: SpeedUpdate) => void>()
  let cancelled = false

  return {
    async speedTest() {
      cancelled = false
      const out: SpeedUpdate[] = []
      for (const n of [1, 4, 8, 16]) {
        if (cancelled) break
        const speed = Math.min(n, 6) * 2.5 * 1024 * 1024
        await new Promise((r) => setTimeout(r, 700))
        const u: SpeedUpdate = { connections: n, state: 'done', speed, detail: null }
        speedSubs.forEach((f) => f(u))
        out.push(u)
      }
      return out
    },
    async cancelSpeedTest() {
      cancelled = true
    },
    subscribeSpeedTest(onUpdate) {
      speedSubs.add(onUpdate)
      return () => void speedSubs.delete(onUpdate)
    },
    async getSettings() {
      return { ...settings }
    },
    async setSettings(s) {
      settings = { ...s }
      return { ...settings }
    },
    async defaultDownloadDir() {
      return 'C:\\Users\\kullanici\\Downloads'
    },
    async pickFolder() {
      return 'D:\\Indirilenler'
    },
    async list() {
      return [...items.values()].map((d) => ({ ...d }))
    },
    async add(url, connections) {
      const d: Download = {
        id: crypto.randomUUID(),
        url,
        filename: filenameFromUrl(url),
        totalBytes: (50 + Math.random() * 950) * 1024 * 1024,
        downloadedBytes: 0,
        speed: 0,
        status: 'downloading',
        connections,
        addedAt: Date.now(),
        segments: [],
        category: 'Diğer',
      }
      items.set(d.id, d)
      emit(d)
      return { ...d }
    },
    async pause(id) {
      const d = items.get(id)
      if (d && d.status === 'downloading') {
        d.status = 'paused'
        d.speed = 0
        emit(d)
      }
    },
    async resume(id) {
      const d = items.get(id)
      if (d && (d.status === 'paused' || d.status === 'failed')) {
        d.status = 'downloading'
        emit(d)
      }
    },
    async remove(id) {
      items.delete(id)
      removeSubs.forEach((f) => f(id))
    },
    async open() {},
    async reveal() {},
    async hash(id) {
      const d = items.get(id)
      if (!d || d.status !== 'completed') return
      d.sha256 = Array.from({ length: 64 }, () => Math.floor(Math.random() * 16).toString(16)).join('')
      emit(d)
      pushNotification('hash', 'info', 'SHA-256 hesaplandı', `${d.filename}\n${d.sha256}`, id)
    },
    async scan(id) {
      const d = items.get(id)
      if (!d || d.status !== 'completed') return
      d.scan = 'clean'
      emit(d)
      pushNotification('scan_clean', 'success', 'Virüs taraması temiz', d.filename, id)
    },
    async listNotifications() {
      return notifications.map((n) => ({ ...n }))
    },
    async markNotificationRead(id) {
      const n = notifications.find((x) => x.id === id)
      if (n) n.read = true
    },
    async markAllNotificationsRead() {
      notifications.forEach((n) => (n.read = true))
    },
    async deleteNotification(id) {
      notifications = notifications.filter((n) => n.id !== id)
    },
    async clearNotifications() {
      notifications = []
    },
    subscribeNotifications(onNew) {
      notifSubs.add(onNew)
      return () => void notifSubs.delete(onNew)
    },
    subscribe(onUpdate, onRemove) {
      updateSubs.add(onUpdate)
      removeSubs.add(onRemove)
      return () => {
        updateSubs.delete(onUpdate)
        removeSubs.delete(onRemove)
      }
    },
  }
}
