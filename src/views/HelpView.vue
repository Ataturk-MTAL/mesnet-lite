<template>
  <div class="page">
    <div class="help-container">
      <h1 class="page-title">{{ labels.help.title }}</h1>

      <Accordion value="0">
        <AccordionPanel v-for="(section, index) in labels.help.sections" :key="section.title" :value="String(index)">
          <AccordionHeader>{{ section.title }}</AccordionHeader>
          <AccordionContent>
            <p v-for="(paragraph, pIndex) in section.paragraphs" :key="pIndex" class="paragraph">
              {{ paragraph }}
            </p>
          </AccordionContent>
        </AccordionPanel>
      </Accordion>
    </div>
  </div>
</template>

<script setup lang="ts">
// Bölüm metinleri tamamen labels.ts'ten gelir; burada Türkçe dizgi YOKTUR.
import { labels } from '../i18n/labels'
</script>

<style scoped>
.page {
  padding: 1.5rem;
  display: flex;
  justify-content: center;
  align-items: flex-start;
}

.help-container {
  max-width: 70ch;
  width: 100%;
}

.page-title {
  font-size: 1.5rem;
  font-weight: 600;
  margin: 0 0 1rem;
}

.paragraph {
  margin: 0 0 0.75rem;
  line-height: 1.6;
}

.paragraph:last-child {
  margin-bottom: 0;
}
</style>
