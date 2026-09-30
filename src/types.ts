export type DownloadStatus =
  | 'queued'
  | 'downloading'
  | 'paused'
  | 'completed'
  | 'failed'

export interface Download {
  id: string
  url: string
  filename: string
  /** Bilinmiyorsa 0. */
  totalBytes: number
  downloadedBytes: number
  /** Bayt/sn. */
  speed: number
  status: DownloadStatus
  /** Aynı anda açık bağlantı sayısı. */
  connections: number
  error?: string
  addedAt: number
  /** Çok parçalı indirmede her parçanın durumu; tek parçaysa boş. */
  segments: Segment[]
  /** Dosya türü: Video, Müzik, Görseller, Belgeler, Arşivler, Programlar, Diğer. */
  category: string
  /** Uygulama duraklattıysa nedeni: ağ yok ya da zamanlayıcı dışı. */
  autoPaused?: 'network' | 'schedule' | null
  /** Hesaplanmış SHA-256 özeti. */
  sha256?: string | null
  /** Beklenen özetle karşılaştırma: true uyuştu, false uyuşmadı, yok: özet verilmedi. */
  verified?: boolean | null
  scan?: 'scanning' | 'clean' | 'threat' | 'error' | null
}

/** Yeni indirmeye eklenebilen isteğe bağlı doğrulama ve kimlik bilgileri. */
export interface AddExtras {
  sha256?: string
  username?: string
  password?: string
  cookie?: string
}

/** Bir parçanın boyutu, ilerlemesi ve sunucuyla yaptığı gerçek konuşmanın kaydı. */
export interface Segment {
  start: number
  end: number
  size: number
  downloaded: number
  /** Gönderilen Range başlığı. */
  range: string
  /** Sunucunun dönüş kodu; parça isteğinde 206 beklenir. 0: henüz yanıt yok. */
  httpStatus: number
  contentRange: string | null
  /** Bağlanılan sunucu adresi (IP:port). */
  remote: string | null
  connectionsOpened: number
  /** Parçanın indirildiği adres (ana adres ya da yansı). */
  source: string
}

/** Hız testinin bir bağlantı seviyesi için canlı ya da nihai sonucu. */
export interface SpeedUpdate {
  connections: number
  state: 'running' | 'done' | 'limited' | 'failed'
  /** Bayt/sn. */
  speed: number
  detail: string | null
}

export interface Settings {
  /** Yeni indirmeler için varsayılan bağlantı (parça) sayısı. */
  connections: number
  /** Bağlantıları sunucunun birden çok IP adresine dağıt (CDN). */
  spreadIps: boolean
  /** Yeni indirmelerin kaydedileceği klasör. */
  downloadDir: string
  /** Aynı anda çalışacak indirme sayısı; fazlası sırada bekler. */
  maxConcurrent: number
  /** Toplam hız sınırı (KB/sn); 0 = sınırsız. */
  speedLimitKbps: number
  /** Açıksa indirmeler yalnızca aşağıdaki saat aralığında çalışır. */
  scheduleEnabled: boolean
  /** Gece yarısından itibaren dakika. */
  scheduleStart: number
  scheduleEnd: number
  notifyOnComplete: boolean
  /** Yeni indirmeleri türüne göre alt klasöre kaydet. */
  sortByCategory: boolean
  /** Bitince Windows Defender ile tara. */
  scanOnComplete: boolean
}

export const CATEGORIES = ['Video', 'Müzik', 'Görseller', 'Belgeler', 'Arşivler', 'Programlar', 'Diğer'] as const

export type Filter = 'all' | 'active' | 'completed'

export type SortKey = 'newest' | 'oldest' | 'name' | 'size' | 'progress' | 'speed'

/** UI'ın motorla konuştuğu tek yüzey. Gerçek uygulamada Tauri komutlarına bağlanır. */
export interface Api {
  list(): Promise<Download[]>
  add(url: string, connections: number, mirrors?: string[], extras?: AddExtras): Promise<Download>
  speedTest(url: string, downloadId?: string): Promise<SpeedUpdate[]>
  cancelSpeedTest(): Promise<void>
  subscribeSpeedTest(onUpdate: (u: SpeedUpdate) => void): () => void
  getSettings(): Promise<Settings>
  setSettings(s: Settings): Promise<Settings>
  /** İşletim sisteminin varsayılan İndirilenler klasörü. */
  defaultDownloadDir(): Promise<string>
  /** Klasör seçme penceresi; iptal edilirse null. */
  pickFolder(start?: string): Promise<string | null>
  pause(id: string): Promise<void>
  resume(id: string): Promise<void>
  remove(id: string): Promise<void>
  open(id: string): Promise<void>
  reveal(id: string): Promise<void>
  /** Tamamlanan dosyanın SHA-256 özetini hesaplar; sonuç abonelik olayıyla gelir. */
  hash(id: string): Promise<void>
  /** Tamamlanan dosyayı Windows Defender ile tarar; sonuç abonelik olayıyla gelir. */
  scan(id: string): Promise<void>
  /** Motor olaylarına abone olur; aboneliği bitiren fonksiyonu döndürür. */
  subscribe(onUpdate: (d: Download) => void, onRemove: (id: string) => void): () => void
}
