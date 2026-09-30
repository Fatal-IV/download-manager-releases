import type { Api } from '../types'
import { createMockApi } from './mockApi'
import { createTauriApi } from './tauriApi'

// Tauri penceresinde gerçek motor; düz tarayıcıda (npm run dev) arayüz denemesi için simülasyon.
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export const api: Api = inTauri ? createTauriApi() : createMockApi()
