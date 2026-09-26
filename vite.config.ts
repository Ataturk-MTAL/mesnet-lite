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

  // Rotalar tembel yüklendiği (`() => import('../views/...')`) ve OpenVue
  // bileşenleri `unplugin-vue-components` ile şablondan çözüldüğü için, henüz
  // ziyaret edilmemiş bir ekranın bileşeni Vite'ın açılıştaki tarayıcısında
  // GÖRÜNMEZ: kaynakta `import` satırı yoktur, dönüşüm anında eklenir. O ekrana
  // ilk tıklandığında Vite yeni bağımlılığı keşfeder, yeniden paketler ve TAM
  // SAYFA YENİLEMESİ yapar ("optimized dependencies changed. reloading").
  // Yenileme bellekteki oturumu siler ve kullanıcıdan PIN yeniden istenir.
  // Hepsini baştan bildirmek keşfi ve dolayısıyla yenilemeyi ortadan kaldırır.
  //
  // Listeyi yenilemek için (yeni bir OpenVue bileşeni kullanıldığında) şu iki
  // taramanın birleşimini al:
  //   rg -o "typeof import\('(openvue/[a-z]+)'\)" -r '$1' components.d.ts | sort -u
  //   rg -o --no-filename "from '(openvue/[a-z]+)'" -r '$1' src/ | sort -u
  optimizeDeps: {
    include: [
      "openvue/accordion",
      "openvue/accordioncontent",
      "openvue/accordionheader",
      "openvue/accordionpanel",
      "openvue/autocomplete",
      "openvue/button",
      "openvue/card",
      "openvue/checkbox",
      "openvue/chip",
      "openvue/column",
      "openvue/config",
      "openvue/confirmationservice",
      "openvue/confirmdialog",
      "openvue/datatable",
      "openvue/datepicker",
      "openvue/dialog",
      "openvue/drawer",
      "openvue/inputnumber",
      "openvue/inputtext",
      "openvue/menu",
      "openvue/message",
      "openvue/panel",
      "openvue/password",
      "openvue/progressbar",
      "openvue/select",
      "openvue/selectbutton",
      "openvue/tag",
      "openvue/textarea",
      "openvue/toast",
      "openvue/toastservice",
      "openvue/toggleswitch",
      "openvue/tooltip",
      "openvue/useconfirm",
      "openvue/usetoast",
    ],
  },

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
