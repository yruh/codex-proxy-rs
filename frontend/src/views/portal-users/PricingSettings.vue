<script setup lang="ts">
import { ref } from 'vue'
import { portalRequest } from '@/api/modules/portal'
import BaseButton from '@/components/base/BaseButton.vue'
import FormItem from '@/components/base/BaseForm/FormItem.vue'
import BaseInput from '@/components/base/BaseInput.vue'
import BaseModal from '@/components/base/BaseModal/index.vue'

interface Pricing { globalMultiplier: string, modelMultipliers: Record<string, string> }
const open = ref(false)
const busy = ref(false)
const loaded = ref(false)
const error = ref('')
const saved = ref(false)
const globalMultiplier = ref('1')
const models = ref<{ model: string, multiplier: string }[]>([])
async function load() {
  open.value = true
  busy.value = true
  loaded.value = false
  error.value = ''
  saved.value = false
  try {
    const data = await portalRequest<Pricing>('/api/admin/portal/pricing')
    globalMultiplier.value = data.globalMultiplier
    models.value = Object.entries(data.modelMultipliers).map(([model, multiplier]) => ({ model, multiplier }))
    loaded.value = true
  }
  catch (e) { error.value = e instanceof Error ? e.message : '加载失败' }
  finally { busy.value = false }
}
async function save() {
  if (busy.value || !loaded.value)
    return
  error.value = ''
  saved.value = false
  busy.value = true
  try {
    const entries = models.value.map(row => [row.model.trim(), row.multiplier] as const)
    if (new Set(entries.map(([model]) => model)).size !== entries.length)
      throw new Error('模型 ID 不能重复')
    await portalRequest('/api/admin/portal/pricing', { globalMultiplier: globalMultiplier.value, modelMultipliers: Object.fromEntries(entries) })
    saved.value = true
  }
  catch (e) { error.value = e instanceof Error ? e.message : '保存失败' }
  finally { busy.value = false }
}
</script>

<template>
  <BaseButton @click="load">
    计费倍率
  </BaseButton>
  <BaseModal v-model="open" title="计费倍率" description="用户扣费 = 原始成本 × 生效倍率" size="lg" :dismissible="!busy">
    <form id="portal-pricing" class="space-y-5" @submit.prevent="save">
      <FormItem label="全局倍率" description="1 为原价，1.5 为加价 50%，0.8 为八折" required>
        <BaseInput v-model="globalMultiplier" :disabled="busy || !loaded" inputmode="decimal" />
      </FormItem>
      <div class="flex items-center justify-between gap-3">
        <h3 class="font-semibold">
          单模型覆盖
        </h3><BaseButton size="sm" :disabled="busy || !loaded || models.length >= 200" @click="models.push({ model: '', multiplier: '1' })">
          添加模型
        </BaseButton>
      </div>
      <p class="text-xs leading-relaxed text-cp-text-tertiary">
        精确匹配请求中的模型 ID。单模型倍率覆盖全局倍率，不叠乘；删除覆盖后恢复使用全局倍率。
      </p>
      <div v-for="(row, index) in models" :key="index" class="grid grid-cols-[minmax(0,1fr)_6rem_auto] items-end gap-3">
        <FormItem label="模型 ID">
          <BaseInput v-model="row.model" :disabled="busy" placeholder="例如 gpt-5.4" :aria-label="`模型 ID ${index + 1}`" />
        </FormItem>
        <FormItem label="倍率">
          <BaseInput v-model="row.multiplier" :disabled="busy" inputmode="decimal" :aria-label="`模型倍率 ${index + 1}`" />
        </FormItem>
        <BaseButton variant="ghost" :disabled="busy" :aria-label="`删除模型 ${index + 1}`" @click="models.splice(index, 1)">
          删除
        </BaseButton>
      </div>
      <p v-if="!models.length && loaded" class="rounded-cp bg-cp-fill-quaternary p-4 text-sm text-cp-text-secondary">
        全部模型使用全局倍率。
      </p>
      <p class="text-xs leading-relaxed text-cp-text-tertiary">
        保存后对新请求生效。进行中的请求和历史扣费保留原规则；双端统计的上游等价成本保持原价。倍率范围为 0.000001 至 1000。
      </p>
      <p v-if="error" role="alert" class="text-sm text-cp-error-text">
        {{ error }}
      </p>
      <p v-if="saved" role="status" class="text-sm text-cp-success-text">
        计费倍率已保存，新请求开始使用新规则。
      </p>
    </form>
    <template #footer>
      <BaseButton :disabled="busy" @click="open = false">
        关闭
      </BaseButton><BaseButton v-if="!loaded" :loading="busy" @click="load">
        重新加载
      </BaseButton><BaseButton v-else type="submit" form="portal-pricing" variant="primary" :loading="busy">
        保存倍率
      </BaseButton>
    </template>
  </BaseModal>
</template>
