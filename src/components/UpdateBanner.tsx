import { Download, RefreshCw, X } from 'lucide-react'
import { useUpdate } from '../lib/updater'

export function UpdateBanner() {
  const { phase, version, progress, dismissed, dismiss, install } = useUpdate()
  if (dismissed || phase === 'idle') return null

  return (
    <div className="fixed bottom-5 right-5 z-20 flex w-[340px] max-w-[calc(100vw-2.5rem)] items-center gap-3 rounded-2xl border border-white/15 bg-panel/85 p-3 pl-4 shadow-[0_16px_50px_rgba(0,0,0,0.45),inset_0_1px_0_rgba(255,255,255,0.15)] backdrop-blur-2xl">
      <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-accent text-white">
        {phase === 'downloading' ? <Download size={16} /> : <RefreshCw size={16} />}
      </div>
      <div className="min-w-0 flex-1">
        {phase === 'downloading' && (
          <>
            <div className="text-sm font-medium">Güncelleme indiriliyor</div>
            <div className="text-xs text-mute">
              {version && `v${version}`}
              {progress !== null && ` · %${Math.round(progress * 100)}`}
            </div>
          </>
        )}
        {phase === 'ready' && (
          <>
            <div className="text-sm font-medium">Yeni sürüm hazır</div>
            <div className="text-xs text-mute">v{version} · süren indirmeler duraklatılır</div>
          </>
        )}
        {phase === 'installing' && <div className="text-sm font-medium">Güncelleme kuruluyor…</div>}
        {phase === 'error' && (
          <>
            <div className="text-sm font-medium">Güncelleme kurulamadı</div>
            <div className="text-xs text-mute">Bir sonraki denetimde yeniden denenecek.</div>
          </>
        )}
      </div>
      {phase === 'ready' && (
        <button
          type="button"
          onClick={() => void install()}
          className="h-9 shrink-0 rounded-xl bg-accent px-3 text-xs font-medium text-white shadow-md shadow-accent/30 transition-transform active:scale-95"
        >
          Yenile
        </button>
      )}
      {phase !== 'installing' && (
        <button
          type="button"
          onClick={dismiss}
          aria-label="Kapat"
          className="grid h-8 w-8 shrink-0 place-items-center rounded-lg text-mute transition-colors hover:bg-white/10 hover:text-ink"
        >
          <X size={15} />
        </button>
      )}
    </div>
  )
}
