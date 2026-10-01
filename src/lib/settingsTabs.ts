import { Bell, Gauge, Info, Puzzle, SlidersHorizontal } from 'lucide-react'
import type { SettingsTab } from '../types'

export const SETTINGS_TABS: { id: SettingsTab; label: string; title: string; hint: string; icon: typeof Bell }[] = [
  { id: 'general', label: 'Genel', title: 'Genel', hint: 'Dosyalar, güvenlik ve açılış davranışı.', icon: SlidersHorizontal },
  { id: 'transfer', label: 'İndirme', title: 'İndirme', hint: 'Bağlantılar, hız sınırı ve zamanlayıcı.', icon: Gauge },
  { id: 'notifications', label: 'Bildirimler', title: 'Bildirimler', hint: 'Hangi olaylarda haber verileceği.', icon: Bell },
  { id: 'browser', label: 'Tarayıcı', title: 'Tarayıcı', hint: 'Tarayıcı eklentisi ve yansı adresleri.', icon: Puzzle },
  { id: 'about', label: 'Hakkında', title: 'Hakkında', hint: 'Sürüm bilgisi ve güncellemeler.', icon: Info },
]
