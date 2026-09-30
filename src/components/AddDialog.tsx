import { useEffect, useMemo, useRef, useState } from 'react'
import { ChevronDown, ClipboardPaste, CornerDownLeft, Download, File, KeyRound, Link2, X } from 'lucide-react'
import clsx from 'clsx'
import { filenameFromUrl } from '../lib/format'
import { useStore } from '../store'

const CONNECTION_OPTIONS = [1, 2, 4, 8, 16, 32]

function parseUrl(v: string): URL | null {
  try {
    const u = new URL(v)
    return u.protocol === 'http:' || u.protocol === 'https:' ? u : null
  } catch {
    return null
  }
}

export function AddDialog({ onClose }: { onClose: () => void }) {
  const add = useStore((s) => s.add)
  const [url, setUrl] = useState('')
  const defaultConnections = useStore((s) => s.settings.connections)
  const [connections, setConnections] = useState(defaultConnections)
  const [mirrorsOpen, setMirrorsOpen] = useState(false)
  const [mirrorText, setMirrorText] = useState('')
  const [advancedOpen, setAdvancedOpen] = useState(false)
  const [sha256, setSha256] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [cookie, setCookie] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const parsed = useMemo(() => parseUrl(url.trim()), [url])
  const filename = parsed ? filenameFromUrl(parsed.href) : ''
  const ext = filename.includes('.') ? filename.split('.').pop()!.slice(0, 5).toUpperCase() : ''

  useEffect(() => {
    inputRef.current?.focus()
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  async function paste() {
    try {
      const text = (await navigator.clipboard.readText()).trim()
      if (text) setUrl(text)
    } catch {
      // Pano izni verilmediyse sessizce geç; kullanıcı elle yapıştırabilir.
    }
    inputRef.current?.focus()
  }

  const mirrors = useMemo(
    () =>
      mirrorText
        .split(/\s+/)
        .map((m) => m.trim())
        .filter((m) => parseUrl(m)),
    [mirrorText],
  )

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    if (!parsed || busy) return
    setBusy(true)
    try {
      await add(parsed.href, connections, mirrors, { sha256, username, password, cookie })
      onClose()
    } catch (err) {
      setError(String(err))
      setBusy(false)
    }
  }

  return (
    <div
      className="dialog-overlay fixed inset-0 z-30 grid place-items-center bg-black/55 px-4 backdrop-blur-md"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <form
        onSubmit={submit}
        className="dialog-card w-[560px] max-w-full overflow-hidden rounded-[28px] border border-white/15 bg-panel/85 shadow-[0_24px_80px_rgba(0,0,0,0.55),inset_0_1px_0_rgba(255,255,255,0.18)] backdrop-blur-3xl backdrop-saturate-150"
      >
        {/* Başlık */}
        <div className="flex items-center gap-3 px-6 pt-6">
          <div className="grid h-11 w-11 place-items-center rounded-2xl bg-accent text-white shadow-lg shadow-accent/30">
            <Download size={20} />
          </div>
          <div className="flex-1">
            <h2 className="text-[17px] font-semibold leading-tight tracking-tight">Yeni indirme</h2>
            <p className="text-xs text-mute">Bağlantıyı yapıştır, gerisini biz halledelim.</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Kapat"
            className="grid h-9 w-9 place-items-center rounded-xl text-mute transition-colors hover:bg-white/10 hover:text-ink"
          >
            <X size={18} />
          </button>
        </div>

        <div className="space-y-5 px-6 pb-2 pt-5">
          {/* URL alanı */}
          <div>
            <div
              className={clsx(
                'flex items-center gap-2 rounded-2xl border bg-black/20 pl-4 pr-1.5 transition-colors focus-within:border-accent',
                url && !parsed ? 'border-bad/70' : 'border-white/10',
              )}
            >
              <Link2 size={18} className="shrink-0 text-mute" />
              <input
                ref={inputRef}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://ornek.com/dosya.zip"
                spellCheck={false}
                className="h-12 min-w-0 flex-1 select-text bg-transparent text-sm outline-none placeholder:text-mute/70"
              />
              <button
                type="button"
                onClick={paste}
                className="flex h-9 shrink-0 items-center gap-1.5 rounded-xl bg-white/10 px-3 text-xs font-medium text-ink transition-colors hover:bg-white/15"
              >
                <ClipboardPaste size={14} /> Yapıştır
              </button>
            </div>
            {error && <p className="mt-2 px-1 text-xs text-bad">{error}</p>}
            {url && !parsed && (
              <p className="mt-2 px-1 text-xs text-bad">Geçerli bir http(s) bağlantısı girin.</p>
            )}
          </div>

          {/* Canlı önizleme */}
          <div
            className={clsx(
              'flex items-center gap-3 rounded-2xl border border-white/10 bg-white/[0.04] p-3 transition-opacity',
              parsed ? 'opacity-100' : 'opacity-40',
            )}
          >
            <div className="relative grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-white/10 text-mute">
              <File size={20} />
              {ext && (
                <span className="absolute -bottom-1.5 rounded-md bg-accent px-1 text-[9px] font-bold leading-4 text-white">
                  {ext}
                </span>
              )}
            </div>
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium">{filename || 'Dosya adı burada görünecek'}</div>
              <div className="truncate text-xs text-mute">{parsed ? parsed.host : 'Sunucu'}</div>
            </div>
          </div>

          {/* Bağlantı sayısı */}
          <div>
            <div className="mb-2 flex items-baseline justify-between px-1">
              <span className="text-xs font-medium text-mute">Paralel bağlantı</span>
              <span className="text-xs text-mute">Sunucu reddederse otomatik azaltılır</span>
            </div>
            <div className="grid grid-cols-6 gap-1.5 rounded-2xl bg-black/20 p-1.5">
              {CONNECTION_OPTIONS.map((n) => (
                <button
                  key={n}
                  type="button"
                  onClick={() => setConnections(n)}
                  aria-pressed={connections === n}
                  className={clsx(
                    'h-10 rounded-xl text-sm font-medium tabular-nums transition-all active:scale-95',
                    connections === n
                      ? 'bg-accent text-white shadow-md shadow-accent/30'
                      : 'text-mute hover:bg-white/10 hover:text-ink',
                  )}
                >
                  {n}
                </button>
              ))}
            </div>
          </div>

          {/* Yansı adresleri */}
          <div>
            <button
              type="button"
              onClick={() => setMirrorsOpen(!mirrorsOpen)}
              className="flex items-center gap-1.5 px-1 text-xs font-medium text-mute transition-colors hover:text-ink"
            >
              <ChevronDown size={14} className={clsx('transition-transform', mirrorsOpen && 'rotate-180')} />
              Yansı adresleri (isteğe bağlı){mirrors.length > 0 && ` · ${mirrors.length}`}
            </button>
            {mirrorsOpen && (
              <>
                <textarea
                  value={mirrorText}
                  onChange={(e) => setMirrorText(e.target.value)}
                  placeholder="Aynı dosyanın başka adresleri, her satıra bir tane"
                  rows={3}
                  spellCheck={false}
                  className="mt-2 w-full resize-none select-text rounded-2xl border border-white/10 bg-black/20 px-4 py-3 text-sm outline-none transition-colors placeholder:text-mute/70 focus:border-accent"
                />
                <p className="mt-1.5 px-1 text-xs text-mute">
                  Parçalar bu adreslere paylaştırılır. Boyutu farklı olan veya parça indirmeyi desteklemeyen
                  adresler otomatik atlanır.
                </p>
              </>
            )}
          </div>

          {/* Doğrulama ve kimlik bilgileri */}
          <div>
            <button
              type="button"
              onClick={() => setAdvancedOpen(!advancedOpen)}
              className="flex items-center gap-1.5 px-1 text-xs font-medium text-mute transition-colors hover:text-ink"
            >
              <ChevronDown size={14} className={clsx('transition-transform', advancedOpen && 'rotate-180')} />
              Doğrulama ve giriş bilgileri (isteğe bağlı)
            </button>
            {advancedOpen && (
              <div className="mt-2 space-y-2">
                <input
                  value={sha256}
                  onChange={(e) => setSha256(e.target.value)}
                  placeholder="Beklenen SHA-256 (bitince dosyayla karşılaştırılır)"
                  spellCheck={false}
                  className="h-11 w-full select-text rounded-2xl border border-white/10 bg-black/20 px-4 font-mono text-xs outline-none transition-colors placeholder:font-sans placeholder:text-mute/70 focus:border-accent"
                />
                <div className="grid grid-cols-2 gap-2">
                  <input
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    placeholder="Kullanıcı adı"
                    autoComplete="off"
                    spellCheck={false}
                    className="h-11 select-text rounded-2xl border border-white/10 bg-black/20 px-4 text-sm outline-none transition-colors placeholder:text-mute/70 focus:border-accent"
                  />
                  <input
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="Parola"
                    type="password"
                    autoComplete="off"
                    className="h-11 select-text rounded-2xl border border-white/10 bg-black/20 px-4 text-sm outline-none transition-colors placeholder:text-mute/70 focus:border-accent"
                  />
                </div>
                <input
                  value={cookie}
                  onChange={(e) => setCookie(e.target.value)}
                  placeholder="Çerez (ör. oturum=abc123; kullanici=1)"
                  spellCheck={false}
                  className="h-11 w-full select-text rounded-2xl border border-white/10 bg-black/20 px-4 text-sm outline-none transition-colors placeholder:text-mute/70 focus:border-accent"
                />
                <p className="flex items-start gap-1.5 px-1 text-xs text-mute">
                  <KeyRound size={13} className="mt-0.5 shrink-0" />
                  <span>
                    Kullanıcı adı ve parola HTTP Basic kimlik doğrulaması olarak, çerez olduğu gibi yalnızca ana
                    adrese gönderilir; yansı adreslerine gitmez. Bilgiler bu bilgisayarda, indirme kaydıyla
                    birlikte saklanır.
                  </span>
                </p>
              </div>
            )}
          </div>
        </div>

        {/* Alt çubuk */}
        <div className="mt-4 flex items-center justify-between border-t border-white/10 bg-black/10 px-6 py-4">
          <div className="hidden items-center gap-3 text-xs text-mute sm:flex">
            <span className="flex items-center gap-1.5">
              <kbd className="rounded-md border border-white/15 px-1.5 py-0.5 text-[10px]">Esc</kbd> kapat
            </span>
            <span className="flex items-center gap-1.5">
              <CornerDownLeft size={12} /> indir
            </span>
          </div>
          <div className="ml-auto flex gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-xl px-4 py-2.5 text-sm text-mute transition-colors hover:bg-white/10 hover:text-ink"
            >
              Vazgeç
            </button>
            <button
              type="submit"
              disabled={!parsed || busy}
              className="flex items-center gap-2 rounded-xl bg-accent px-5 py-2.5 text-sm font-semibold text-white shadow-lg shadow-accent/30 transition-all enabled:hover:brightness-110 enabled:active:scale-95 disabled:opacity-40"
            >
              <Download size={16} /> İndir
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
