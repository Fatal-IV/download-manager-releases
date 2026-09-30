import { create } from 'zustand'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { check, type Update } from '@tauri-apps/plugin-updater'

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** Uygulama açık kalırsa güncelleme için GitHub'ı bu aralıkla yeniden denetler. */
const CHECK_INTERVAL_MS = 15 * 60 * 1000

const WHATS_NEW_KEY = 'dm-whats-new'

export interface WhatsNew {
  version: string
  notes: string
}

/** Güncelleme sonrası ilk açılışta, kurulan sürümün notlarını döndürür (bir kez). */
export async function takeWhatsNew(): Promise<WhatsNew | null> {
  if (!inTauri) return null
  try {
    const raw = localStorage.getItem(WHATS_NEW_KEY)
    if (!raw) return null
    const saved = JSON.parse(raw) as WhatsNew
    const current = await getVersion()
    // Kurulum henüz tamamlanmadıysa (eski sürüm çalışıyorsa) not saklı kalır.
    if (saved.version !== current) return null
    localStorage.removeItem(WHATS_NEW_KEY)
    return saved
  } catch {
    return null
  }
}

export type UpdatePhase = 'idle' | 'downloading' | 'ready' | 'installing' | 'error'

interface UpdateState {
  phase: UpdatePhase
  version: string | null
  /** 0-1 arası indirme ilerlemesi; boyut bilinmiyorsa null. */
  progress: number | null
  dismissed: boolean
  /** Ayarlar'daki elle denetimin durumu: sürüm yok ("latest") ya da denetlenemedi ("failed"). */
  manual: 'idle' | 'checking' | 'latest' | 'failed'
  dismiss: () => void
  install: () => Promise<void>
}

let pending: Update | null = null
let busy = false

export const useUpdate = create<UpdateState>((set, get) => ({
  phase: 'idle',
  version: null,
  progress: null,
  dismissed: false,
  manual: 'idle',

  dismiss: () => set({ dismissed: true }),

  async install() {
    if (!pending || get().phase !== 'ready') return
    set({ phase: 'installing' })
    try {
      // Süren indirmeler duraklatılıp diske yazılır; kurulum uygulamayı kapatıp yeniden açar.
      await invoke('prepare_for_update')
      // Yeni sürüm açıldığında "Yenilikler" penceresinde gösterilmek üzere sakla.
      try {
        localStorage.setItem(WHATS_NEW_KEY, JSON.stringify({ version: pending.version, notes: pending.body ?? '' }))
      } catch {
        // Depolama kullanılamıyorsa yalnızca yenilik penceresi gösterilmez.
      }
      await pending.install()
    } catch {
      set({ phase: 'error' })
    }
  },
}))

/** Yeni sürüm varsa arka planda indirir ve kullanıcı onayıyla kurulmaya hazır bırakır. */
async function checkOnce(manual = false) {
  if (busy || pending) {
    // Elle denetimde hazır/inen güncelleme zaten bannerda görünür; tekrar aramaya gerek yok.
    if (manual) useUpdate.setState({ manual: 'idle', dismissed: false })
    return
  }
  busy = true
  if (manual) useUpdate.setState({ manual: 'checking' })
  try {
    const update = await check()
    if (!update) {
      if (manual) useUpdate.setState({ manual: 'latest' })
      return
    }
    if (manual) useUpdate.setState({ manual: 'idle' })
    useUpdate.setState({ phase: 'downloading', version: update.version, progress: null, dismissed: false })
    let total = 0
    let received = 0
    await update.download((e) => {
      if (e.event === 'Started') total = e.data.contentLength ?? 0
      else if (e.event === 'Progress') {
        received += e.data.chunkLength
        if (total > 0) useUpdate.setState({ progress: Math.min(1, received / total) })
      }
    })
    pending = update
    useUpdate.setState({ phase: 'ready', progress: 1 })
  } catch {
    // Ağ yok, sürüm yok ya da GitHub'a ulaşılamadı; bir sonraki denetimde yeniden denenir.
    useUpdate.setState({ phase: 'idle', ...(manual && { manual: 'failed' as const }) })
  } finally {
    busy = false
  }
}

/** Ayarlar > Güncellemeler: yeni sürümü hemen denetler. */
export function checkForUpdatesNow(): Promise<void> {
  return inTauri ? checkOnce(true) : Promise.resolve()
}

/** Açılışta bir kez, ardından her 15 dakikada denetler. Aboneliği bitiren fonksiyonu döndürür. */
export function startUpdateChecks(): () => void {
  if (!inTauri) return () => {}
  void checkOnce()
  const timer = window.setInterval(() => void checkOnce(), CHECK_INTERVAL_MS)
  return () => window.clearInterval(timer)
}
