const $ = (id) => document.getElementById(id)

async function checkApp() {
  $('status').textContent = 'Uygulama denetleniyor…'
  $('dot').className = 'dot'
  $('retry').hidden = true
  try {
    const r = await fetch('http://127.0.0.1:38653/ping', { signal: AbortSignal.timeout(1500) })
    if (!r.ok) throw new Error('HTTP ' + r.status)
    $('status').textContent = 'Uygulama bağlı'
    $('dot').className = 'dot ok'
    chrome.action.setBadgeText({ text: '' })
  } catch (e) {
    $('status').textContent = `Uygulama açık değil (${e.message})`
    $('dot').className = 'dot bad'
    $('retry').hidden = false
  }
}

function ago(ts) {
  const s = Math.max(0, Math.round((Date.now() - ts) / 1000))
  if (s < 60) return 'az önce'
  if (s < 3600) return `${Math.floor(s / 60)} dk önce`
  if (s < 86400) return `${Math.floor(s / 3600)} sa önce`
  return `${Math.floor(s / 86400)} gün önce`
}

async function load() {
  const { enabled = true, minMb = 0, recent = [] } = await chrome.storage.local.get(['enabled', 'minMb', 'recent'])
  $('on').checked = enabled
  $('min').value = minMb
  const list = $('recent')
  list.replaceChildren()
  for (const item of recent) {
    const li = document.createElement('li')
    const name = document.createElement('span')
    name.className = 'name'
    name.textContent = item.name
    name.title = item.url
    const when = document.createElement('span')
    when.className = 'when'
    when.textContent = ago(item.time)
    li.append(name, when)
    list.append(li)
  }
  $('empty').hidden = recent.length > 0
}

$('on').addEventListener('change', () => chrome.storage.local.set({ enabled: $('on').checked }))
$('min').addEventListener('change', () => {
  const v = Math.max(0, Number($('min').value) || 0)
  $('min').value = v
  chrome.storage.local.set({ minMb: v })
})
$('retry').addEventListener('click', checkApp)

load()
checkApp()
