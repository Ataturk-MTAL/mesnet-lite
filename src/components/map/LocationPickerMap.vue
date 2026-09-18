<template>
  <div class="picker">
    <div ref="mapEl" class="map" />

    <div class="coords">
      <div class="coord-field">
        <label for="lat-input">{{ labels.map.latitude }}</label>
        <InputNumber
          id="lat-input"
          :model-value="modelValue?.latitude ?? null"
          :min-fraction-digits="4"
          :max-fraction-digits="6"
          :min="-90"
          :max="90"
          @update:model-value="onLatitudeInput"
        />
      </div>
      <div class="coord-field">
        <label for="lng-input">{{ labels.map.longitude }}</label>
        <InputNumber
          id="lng-input"
          :model-value="modelValue?.longitude ?? null"
          :min-fraction-digits="4"
          :max-fraction-digits="6"
          :min="-180"
          :max="180"
          @update:model-value="onLongitudeInput"
        />
      </div>
      <Button
        :label="labels.map.clear"
        icon="pi pi-times"
        severity="secondary"
        outlined
        :disabled="modelValue === null"
        @click="emit('update:modelValue', null)"
      />
    </div>

    <small class="hint">{{ labels.map.hint }}</small>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, shallowRef, watch, ref } from 'vue'
import L from 'leaflet'
import { labels } from '../../i18n/labels'
import type { LatLng } from '../../types/models'

const props = withDefaults(
  defineProps<{
    modelValue: LatLng | null
    /** Konum boşken haritanın odaklanacağı yer. */
    fallbackCenter?: LatLng
    zoom?: number
  }>(),
  {
    fallbackCenter: () => ({ latitude: 36.8121, longitude: 34.6415 }),
    zoom: 13,
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: LatLng | null] }>()

const mapEl = ref<HTMLDivElement | null>(null)

// Leaflet kendi DOM'unu yönetir. Nesneleri ref() içine koymak Vue'nun onları
// proxy'lemesine ve Leaflet'in iç referans karşılaştırmalarının bozulmasına yol
// açar; bu yüzden shallowRef kullanılır.
const map = shallowRef<L.Map | null>(null)
let resizeObserver: ResizeObserver | null = null
const marker = shallowRef<L.Marker | null>(null)

function currentCenter(): L.LatLngExpression {
  const point = props.modelValue ?? props.fallbackCenter
  return [point.latitude, point.longitude]
}

function placeMarker(point: LatLng): void {
  if (!map.value) return

  if (marker.value) {
    marker.value.setLatLng([point.latitude, point.longitude])
    return
  }

  const created = L.marker([point.latitude, point.longitude], { draggable: true })
  created.on('dragend', () => {
    const position = created.getLatLng()
    emit('update:modelValue', { latitude: position.lat, longitude: position.lng })
  })
  created.addTo(map.value)
  marker.value = created
}

function removeMarker(): void {
  if (marker.value && map.value) {
    map.value.removeLayer(marker.value)
  }
  marker.value = null
}

function onLatitudeInput(value: number | null): void {
  if (value === null) return
  emit('update:modelValue', {
    latitude: value,
    longitude: props.modelValue?.longitude ?? props.fallbackCenter.longitude,
  })
}

function onLongitudeInput(value: number | null): void {
  if (value === null) return
  emit('update:modelValue', {
    latitude: props.modelValue?.latitude ?? props.fallbackCenter.latitude,
    longitude: value,
  })
}

onMounted(() => {
  if (!mapEl.value) return

  const created = L.map(mapEl.value).setView(currentCenter(), props.zoom)

  // OpenStreetMap kullanım koşulları katkı bildirimini zorunlu kılar.
  L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom: 19,
    attribution: '&copy; OpenStreetMap katkıda bulunanlar',
  }).addTo(created)

  created.on('click', (event: L.LeafletMouseEvent) => {
    emit('update:modelValue', { latitude: event.latlng.lat, longitude: event.latlng.lng })
  })

  map.value = created

  if (props.modelValue) {
    placeMarker(props.modelValue)
  }

  // Harita, boyutu henüz kesinleşmemiş bir kapsayıcıda (Card içeriği, açılan
  // diyalog) kurulduğunda kendi ölçüsünü yanlış hesaplar ve karoların bir kısmı
  // boş kalır. Düzen oturduktan sonra yeniden ölçtürmek gerekir.
  resizeObserver = new ResizeObserver(() => created.invalidateSize())
  resizeObserver.observe(mapEl.value)
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
  removeMarker()
  map.value?.remove()
  map.value = null
})

watch(
  () => props.modelValue,
  (value) => {
    if (!map.value) return
    if (value === null) {
      removeMarker()
      return
    }
    placeMarker(value)
    map.value.panTo([value.latitude, value.longitude])
  },
)
</script>

<style scoped>
.picker { display: flex; flex-direction: column; gap: 0.75rem; }
.map {
  height: 22rem;
  width: 100%;
  border-radius: var(--p-content-border-radius);
  border: 1px solid var(--p-content-border-color);
}
.coords { display: flex; gap: 1rem; align-items: flex-end; }
.coord-field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
</style>
