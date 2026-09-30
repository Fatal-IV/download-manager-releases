const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

export function formatBytes(n: number): string {
  if (!n) return '0 B'
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), UNITS.length - 1)
  const v = n / 1024 ** i
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${UNITS[i]}`
}

export function formatSpeed(bps: number): string {
  return `${formatBytes(bps)}/sn`
}

export function formatEta(remaining: number, speed: number): string {
  if (!speed || !remaining) return '—'
  const s = Math.round(remaining / speed)
  if (s < 60) return `${s} sn`
  if (s < 3600) return `${Math.floor(s / 60)} dk ${s % 60} sn`
  return `${Math.floor(s / 3600)} sa ${Math.floor((s % 3600) / 60)} dk`
}

export function filenameFromUrl(url: string): string {
  try {
    const last = new URL(url).pathname.split('/').filter(Boolean).pop()
    return last ? decodeURIComponent(last) : 'indirme'
  } catch {
    return 'indirme'
  }
}
