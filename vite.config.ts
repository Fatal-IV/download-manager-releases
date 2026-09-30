import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri geliştirme sunucusundan sabit port bekler.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Rust derlemesi src-tauri/target içindeki dosyaları kilitler; izlemek EBUSY ile çöker.
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
