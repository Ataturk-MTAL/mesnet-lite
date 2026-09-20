import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import Components from "unplugin-vue-components/vite";
// OpenVue fork'u export adını korumuş: paket @openvue olsa da sembol PrimeVueResolver.
import { PrimeVueResolver } from "@openvue/auto-import-resolver";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [
    vue(),
    // OpenVue bileşenleri otomatik çözümlenir; tek tek import yolu yazılmaz.
    Components({ resolvers: [PrimeVueResolver()] }),
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  test: {
    environment: "jsdom",
    // jsdom'da bulunmayan tarayıcı API'leri burada sağlanır.
    setupFiles: ["./src/test-setup.ts"],
    // Varsayılan havuz 'forks': izolasyon açıkken (isolate: true, varsayılan)
    // her spec dosyası için ayrı bir işlem çatallanır ve jsdom ortamı sıfırdan
    // kurulur. 16 dosya için 16 çatallama, makine yükü altında saniyelerce
    // sürebiliyor ve o sırada asıl testler CPU'dan mahrum kalıp 5 sn'lik
    // testTimeout'u aşıyor (rastgele testler zaman aşımına düşüyor). vmThreads
    // aynı işçi thread'lerini dosyalar arasında yeniden kullanır, her dosyaya
    // yalnızca ayrı bir vm bağlamı açar; dosya başına izolasyon korunur ama
    // işlem çatallama ve ortam yeniden kurma maliyeti ortadan kalkar.
    pool: "vmThreads",
    // Makine yoğunken (paralel işçiler, arka plan süreçleri) tek bir testin
    // gerçek çalışma süresi 1 sn'nin altındayken bile zamanlayıcı bekleyişleri
    // ve DOM güncellemeleri birkaç saniyeye çıkabilir; 5 sn'lik varsayılan pay
    // bırakmıyor. 15 sn, normal çalışma süresinin (~100-600 ms) çok üstünde bir
    // güvenlik payı sağlar, testleri yavaşlatmaz.
    testTimeout: 15000,
  },
}));
