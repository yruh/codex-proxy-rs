<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { portalRequest } from '@/api/modules/portal'
import PricingSettings from './PricingSettings.vue'

const emit = defineEmits<{ saved: [] }>()
const policy = ref<{ globalMultiplier: string, modelMultipliers: Record<string, string> }>()
const error = ref('')
async function load() {
  try {
    policy.value = await portalRequest('/api/admin/portal/pricing')
    error.value = ''
  }
  catch { error.value = '计费倍率加载失败，可打开设置重试。' }
}
async function saved() {
  await load()
  emit('saved')
}
onMounted(load)
</script>

<template>
  <div class="mb-5 flex flex-wrap items-center justify-between gap-3 rounded-cp bg-cp-fill-tertiary p-4">
    <div class="min-w-0 text-sm">
      <p class="font-semibold">
        当前计费倍率
      </p>
      <p v-if="policy" class="mt-1 break-words text-cp-text-secondary">
        全局 {{ Number(policy.globalMultiplier) }}×<span v-for="(multiplier, model) in policy.modelMultipliers" :key="model"> · {{ model }} {{ Number(multiplier) }}×</span>
      </p>
      <p v-else class="mt-1 text-cp-text-secondary">
        {{ error || '加载中…' }}
      </p>
      <p class="mt-1 text-xs text-cp-text-tertiary">
        单模型覆盖全局倍率。历史实际扣款保留当次倍率，容量预测按当前规则折算。
      </p>
    </div>
    <PricingSettings @saved="saved" />
  </div>
</template>
