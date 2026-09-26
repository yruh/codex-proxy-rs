<script setup lang="ts">
import type { AccountListResponse, AccountModelsResponse } from '@/api/modules/accounts'
import { BaseButton, BaseFormItem, BaseInput, BaseModal, BaseSelect } from '@codex-proxy/ui'
import { ref } from 'vue'
import { portalRequest } from '@/api/modules/portal'

interface Pricing { globalMultiplier: string, modelMultipliers: Record<string, string> }
const emit = defineEmits<{ saved: [] }>()
const open = ref(false)
const busy = ref(false)
const loaded = ref(false)
const error = ref('')
const saved = ref(false)
const globalMultiplier = ref('1')
const models = ref<{ model: string, multiplier: string, custom: boolean }[]>([])
const catalog = ref<{ label: string, value: string }[]>([])
const catalogBusy = ref(false)
const catalogError = ref('')
async function loadCatalog() {
  catalogBusy.value = true
  catalogError.value = ''
  const found = new Map<string, string>()
  try {
    let page = 1
    let totalPages = 1
    let failures = 0
    do {
      const accounts = await portalRequest<AccountListResponse>(`/api/admin/accounts?page=${page}&pageSize=100`)
      totalPages = accounts.page.totalPages
      for (let start = 0; start < accounts.items.length; start += 5) {
        const results = await Promise.allSettled(accounts.items.slice(start, start + 5).map(account =>
          portalRequest<AccountModelsResponse>(`/api/admin/accounts/models?accountId=${encodeURIComponent(account.id)}`),
        ))
        for (const result of results) {
          if (result.status === 'rejected') {
            failures++
            continue
          }
          for (const model of result.value.models)
            found.set(model.id, model.label)
        }
      }
      page++
    } while (page <= totalPages)
    if (failures)
      catalogError.value = '部分账号的模型加载失败，可以重试或手动输入模型 ID。'
  }
  catch { catalogError.value = '模型列表加载失败，可以重试或手动输入模型 ID。' }
  finally {
    catalog.value = [...found].sort(([a], [b]) => a.localeCompare(b)).map(([value, label]) => ({ value, label: value, description: label === value ? undefined : label }))
    catalogBusy.value = false
  }
}
function modelOptions(current: string) {
  const options = catalog.value.map(option => ({ ...option, disabled: models.value.some(row => row.model === option.value && row.model !== current) }))
  if (current && !options.some(option => option.value === current))
    options.unshift({ value: current, label: current, disabled: false })
  return options
}
async function load() {
  open.value = true
  busy.value = true
  loaded.value = false
  error.value = ''
  saved.value = false
  if (!catalogBusy.value)
    void loadCatalog()
  try {
    const data = await portalRequest<Pricing>('/api/admin/portal/pricing')
    globalMultiplier.value = data.globalMultiplier
    models.value = Object.entries(data.modelMultipliers).map(([model, multiplier]) => ({ model, multiplier, custom: false }))
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
    emit('saved')
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
      <BaseFormItem label="全局倍率" description="1 为原价，1.5 为加价 50%，0.8 为八折" required>
        <BaseInput v-model="globalMultiplier" :disabled="busy || !loaded" inputmode="decimal" />
      </BaseFormItem>
      <div class="flex items-center justify-between gap-3">
        <h3 class="font-semibold">
          单模型覆盖
        </h3><BaseButton size="sm" :disabled="busy || !loaded || models.length >= 200" @click="models.push({ model: '', multiplier: '1', custom: false })">
          添加模型
        </BaseButton>
      </div>
      <p class="text-xs leading-relaxed text-cp-text-tertiary">
        精确匹配请求中的模型 ID。单模型倍率覆盖全局倍率，不叠乘；删除覆盖后恢复使用全局倍率。
      </p>
      <div class="flex items-center justify-between gap-3 text-xs text-cp-text-tertiary">
        <span>{{ catalogError || (catalogBusy ? '正在加载账号模型…' : `已汇总 ${catalog.length} 个模型，重复模型已合并。`) }}</span>
        <BaseButton size="sm" :disabled="catalogBusy" @click="loadCatalog">
          重新加载模型
        </BaseButton>
      </div>
      <div v-for="(row, index) in models" :key="index" class="grid grid-cols-[minmax(0,1fr)_6rem_auto] items-start gap-3">
        <BaseFormItem label="模型 ID">
          <div class="space-y-2">
            <BaseInput v-if="row.custom" v-model="row.model" :disabled="busy" placeholder="输入自定义模型 ID" :aria-label="`模型 ID ${index + 1}`" />
            <BaseSelect v-else v-model="row.model" class="w-full" :options="modelOptions(row.model)" :disabled="busy || catalogBusy" placeholder="选择已有模型" empty-text="暂无模型，请手动输入或重新加载" :aria-label="`选择模型 ${index + 1}`" />
            <BaseButton size="sm" variant="ghost" :disabled="busy" @click="row.custom = !row.custom">
              {{ row.custom ? '从已有模型选择' : '手动输入模型 ID' }}
            </BaseButton>
          </div>
        </BaseFormItem>
        <BaseFormItem label="倍率">
          <BaseInput v-model="row.multiplier" :disabled="busy" inputmode="decimal" :aria-label="`模型倍率 ${index + 1}`" />
        </BaseFormItem>
        <BaseButton class="mt-6" variant="ghost" :disabled="busy" :aria-label="`删除模型 ${index + 1}`" @click="models.splice(index, 1)">
          删除
        </BaseButton>
      </div>
      <p v-if="!models.length && loaded" class="rounded-cp bg-cp-fill-quaternary p-4 text-sm text-cp-text-secondary">
        全部模型使用全局倍率。
      </p>
      <p class="text-xs leading-relaxed text-cp-text-tertiary">
        保存后对新请求生效。进行中的请求和历史扣费保留原规则；双端统计保留原价，并额外按当前规则预测计费容量。倍率范围为 0.000001 至 1000。
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
