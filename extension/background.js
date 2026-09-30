const APP = 'http://127.0.0.1:38653'
const MAX_RECENT = 5

async function settings() {
  const { enabled = true, minMb = 0 } = await chrome.storage.local.get(['enabled', 'minMb'])
  return { enabled, minBytes: minMb * 1024 * 1024 }
}

async function appRunning() {
  try {
    const r = await fetch(`${APP}/ping`, { signal: AbortSignal.timeout(1500) })
    return r.ok
  } catch {
    return false
  }
}

async function cookieHeader(url) {
  try {
    const cookies = await chrome.cookies.getAll({ url })
    return cookies.map((c) => `${c.name}=${c.value}`).join('; ')
  } catch {
    return ''
  }
}

function nameOf(url, filename) {
  const raw = filename || decodeURIComponent(new URL(url).pathname.split('/').filter(Boolean).pop() || url)
  return raw.split(/[\\/]/).pop()
}

async function remember(url, filename) {
  const { recent = [] } = await chrome.storage.local.get('recent')
  recent.unshift({ name: nameOf(url, filename), url, time: Date.now() })
  await chrome.storage.local.set({ recent: recent.slice(0, MAX_RECENT) })
}

async function send({ url, filename, referrer }) {
  const body = {
    url,
    filename,
    referrer,
    userAgent: navigator.userAgent,
    cookie: await cookieHeader(url),
  }
  const r = await fetch(`${APP}/add`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (r.ok) {
    await remember(url, filename)
    chrome.action.setBadgeText({ text: '' })
  }
  return r.ok
}

function flagAppClosed() {
  chrome.action.setBadgeBackgroundColor({ color: '#ff6b6b' })
  chrome.action.setBadgeText({ text: '!' })
}

chrome.downloads.onCreated.addListener(async (item) => {
  try {
    const { enabled, minBytes } = await settings()
    if (!enabled) return
    const url = item.finalUrl || item.url
    if (!/^https?:/.test(url)) return // blob:, data: vb. tarayıcıda kalır
    // Boyut henüz bilinmiyorsa (0 ya da -1) küçük dosya varsayamayız; devral.
    if (minBytes > 0 && item.totalBytes > 0 && item.totalBytes < minBytes) return
    if (!(await appRunning())) {
      flagAppClosed() // uygulama kapalıysa tarayıcı kendi indirsin
      return
    }
    if (await send({ url, referrer: item.referrer })) {
      await chrome.downloads.cancel(item.id)
      await chrome.downloads.erase({ id: item.id })
    }
  } catch (e) {
    console.warn('İndirme devredilemedi', e)
  }
})

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: 'dm-link',
    title: 'İndirme Yöneticisi ile indir',
    contexts: ['link', 'video', 'audio', 'image'],
  })
})

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl
  if (!url || !/^https?:/.test(url)) return
  if (!(await appRunning())) {
    flagAppClosed()
    return
  }
  await send({ url, referrer: tab?.url })
})
