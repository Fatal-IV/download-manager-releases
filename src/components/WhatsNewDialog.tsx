import { useEffect } from 'react'
import { Sparkles } from 'lucide-react'
import type { WhatsNew } from '../lib/updater'

export function WhatsNewDialog({ info, onClose }: { info: WhatsNew; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const lines = info.notes
    .split('\n')
    .map((l) => l.replace(/^\s*[-*]\s+/, '').trim())
    .filter(Boolean)

  return (
    <div
      className="dialog-overlay fixed inset-0 z-30 grid place-items-center bg-black/55 px-4 backdrop-blur-md"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="dialog-card max-h-[92vh] w-[460px] max-w-full overflow-y-auto rounded-[28px] border border-white/15 bg-panel/85 shadow-[0_24px_80px_rgba(0,0,0,0.55),inset_0_1px_0_rgba(255,255,255,0.18)] backdrop-blur-3xl backdrop-saturate-150">
        <div className="flex items-center gap-3 px-6 pt-6">
          <div className="grid h-11 w-11 place-items-center rounded-2xl bg-accent text-white shadow-lg shadow-accent/30">
            <Sparkles size={20} />
          </div>
          <div className="flex-1">
            <h2 className="text-[17px] font-semibold leading-tight tracking-tight">Güncelleme tamamlandı</h2>
            <p className="text-xs text-mute">Sürüm {info.version} kuruldu.</p>
          </div>
        </div>

        <div className="px-6 pb-6 pt-5">
          <div className="mb-2 px-1 text-sm font-medium">Yenilikler</div>
          {lines.length > 0 ? (
            <ul className="space-y-2 rounded-2xl bg-black/20 p-4 text-sm">
              {lines.map((l, i) => (
                <li key={i} className="flex gap-2.5">
                  <span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />
                  <span>{l}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="rounded-2xl bg-black/20 p-4 text-sm text-mute">Bu sürüm için not eklenmemiş.</p>
          )}
          <button
            type="button"
            onClick={onClose}
            className="mt-5 h-11 w-full rounded-2xl bg-accent text-sm font-medium text-white shadow-md shadow-accent/30 transition-transform active:scale-[0.98]"
          >
            Tamam
          </button>
        </div>
      </div>
    </div>
  )
}
