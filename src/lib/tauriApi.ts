import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Api, AppNotification, Download, Settings, SpeedUpdate } from '../types'

/** Rust motoruna Tauri komutları ve olayları üzerinden bağlanır. */
export function createTauriApi(): Api {
  return {
    list: () => invoke<Download[]>('list_downloads'),
    add: (url, connections, mirrors, extras) =>
      invoke<Download>('add_download', {
        url,
        connections,
        mirrors,
        sha256: extras?.sha256 || null,
        username: extras?.username || null,
        password: extras?.password || null,
        cookie: extras?.cookie || null,
      }),
    speedTest: (url, downloadId) => invoke<SpeedUpdate[]>('speed_test', { url, id: downloadId ?? null }),
    cancelSpeedTest: () => invoke('cancel_speed_test'),
    subscribeSpeedTest(onUpdate) {
      const off = listen<SpeedUpdate>('speedtest-update', (e) => onUpdate(e.payload))
      return () => void off.then((f) => f())
    },
    getSettings: () => invoke<Settings>('get_settings'),
    setSettings: (settings) => invoke<Settings>('set_settings', { settings }),
    defaultDownloadDir: () => invoke<string>('default_download_dir'),
    pickFolder: (start) => invoke<string | null>('pick_folder', { start: start ?? null }),
    pause: (id) => invoke('pause_download', { id }),
    resume: (id) => invoke('resume_download', { id }),
    remove: (id) => invoke('remove_download', { id, deleteFile: false }),
    open: (id) => invoke('open_download', { id }),
    reveal: (id) => invoke('reveal_download', { id }),
    hash: (id) => invoke('hash_download', { id }),
    scan: (id) => invoke('scan_download', { id }),
    listNotifications: () => invoke<AppNotification[]>('list_notifications'),
    markNotificationRead: (id) => invoke('mark_notification_read', { id }),
    markAllNotificationsRead: () => invoke('mark_all_notifications_read'),
    deleteNotification: (id) => invoke('delete_notification', { id }),
    clearNotifications: () => invoke('clear_notifications'),
    subscribeNotifications(onNew) {
      const off = listen<AppNotification>('notification', (e) => onNew(e.payload))
      return () => void off.then((f) => f())
    },
    subscribe(onUpdate, onRemove) {
      const offs = [
        listen<Download>('download-update', (e) => onUpdate(e.payload)),
        listen<string>('download-remove', (e) => onRemove(e.payload)),
      ]
      return () => offs.forEach((p) => void p.then((off) => off()))
    },
  }
}
