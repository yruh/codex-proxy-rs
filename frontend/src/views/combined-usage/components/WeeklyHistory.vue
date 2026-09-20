<script setup lang="ts">
import type { Account } from '@/api/modules/accounts'
import type { BaseTableColumn } from '@/components/base/BaseTable/columns'
import { computed, ref, watch } from 'vue'
import { refreshAccountQuota } from '@/api/modules/accounts'
import { portalRequest } from '@/api/modules/portal'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseCard from '@/components/base/BaseCard.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseTable from '@/components/base/BaseTable/index.vue'

interface Cycle {
  start: string
  end: string
  resetAt: string
  observedAt: string
  usedPercent: number
  boundary: string
  uncertainAfter: string | null
  current: boolean
  requests: number
  tokens: number
  usd: number
  pricedUsd: number
  incompleteCost: boolean
}
const props = defineProps<{ accounts: Account[], pricingRevision: number }>()
const emit = defineEmits<{
  select: [range: { accountId: string, start: string, end: string }]
  accountUpdated: [account: Account]
}>()
const selected = ref('')
const rows = ref<Cycle[]>([])
const busy = ref(false)
const error = ref('')
let revision = 0
const options = computed(() => props.accounts.map(account => ({ label: account.name, value: account.id })))
const columns: BaseTableColumn<Cycle>[] = [
  { key: 'start', label: '额度周期 · 北京时间', size: '3xl' },
  { key: 'usedPercent', label: '最后观测用量', size: 'xl' },
  { key: 'tokens', label: '已记录 Tokens', kind: 'numeric', size: 'xl' },
  { key: 'usd', label: '原价 USD', kind: 'numeric', size: 'lg' },
  { key: 'pricedUsd', label: '当前倍率折算 USD', kind: 'numeric', size: 'xl' },
  { key: 'actions', label: '明细', kind: 'actions', size: 'sm' },
]
const date = (value: string) => new Date(value).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', hour12: false })
async function load(refresh = false) {
  const accountId = selected.value
  if (!accountId)
    return
  const request = ++revision
  busy.value = true
  error.value = ''
  rows.value = []
  try {
    if (refresh) {
      const account = await refreshAccountQuota({ accountId })
      emit('accountUpdated', account.account)
    }
    const result = await portalRequest<{ items: Cycle[] }>(`/api/admin/accounts/quota-cycles?accountId=${encodeURIComponent(accountId)}`)
    if (request === revision)
      rows.value = result.items
  }
  catch (cause) {
    if (request === revision)
      error.value = cause instanceof Error ? cause.message : '周期加载失败'
  }
  finally {
    if (request === revision)
      busy.value = false
  }
}
watch(options, (items) => {
  if (!items.some(item => item.value === selected.value))
    selected.value = items[0]?.value || ''
}, { immediate: true })
watch(selected, () => void load(), { immediate: true })
watch(() => props.pricingRevision, () => void load())
</script>

<template>
  <BaseCard title="历史周额度" description="按上游额度周期查看，包含本地直连和服务器代理">
    <template #actions>
      <BaseSelect v-model="selected" :options="options" aria-label="历史周额度账号" class="min-w-48" />
      <BaseButton :loading="busy" :disabled="!selected" @click="load(true)">
        刷新上游额度
      </BaseButton>
    </template>
    <p v-if="error" role="alert" class="mb-3 text-sm text-cp-error-text">
      {{ error }}
    </p>
    <BaseTable :columns="columns" :rows="rows" :row-key="row => `${row.start}:${row.resetAt}`" :loading="busy" empty-text="尚无可确认的周额度周期，刷新上游额度后重试">
      <template #start="{ row }">
        <p class="font-medium">
          {{ date(row.start) }} — {{ row.current ? '至今' : date(row.end) }}
        </p>
        <p class="mt-1 text-xs text-cp-text-tertiary">
          {{ row.current ? '当前周期' : '历史周期' }} · {{ row.requests.toLocaleString() }} 次响应
        </p>
        <p v-if="row.uncertainAfter" class="mt-1 text-xs text-cp-warning-text">
          {{ row.boundary === 'observed_reset' ? '观测到额度回落' : '提前换窗或额度调整' }}：{{ date(row.uncertainAfter) }} 至 {{ date(row.start) }}，按首次新观测分段
        </p>
      </template>
      <template #usedPercent="{ row }">
        <p>{{ row.usedPercent.toFixed(1) }}%</p>
        <p class="mt-1 text-xs text-cp-text-tertiary">
          {{ date(row.observedAt) }}
        </p>
      </template>
      <template #tokens="{ row }">
        {{ row.tokens.toLocaleString() }}
      </template>
      <template #usd="{ row }">
        ${{ row.usd.toFixed(4) }}<span v-if="row.incompleteCost" class="block text-xs text-cp-warning-text">部分未计价</span>
      </template>
      <template #pricedUsd="{ row }">
        ${{ row.pricedUsd.toFixed(4) }}
      </template>
      <template #actions="{ row }">
        <BaseButton size="sm" variant="ghost" @click="emit('select', { accountId: selected, start: row.start, end: row.end })">
          查看
        </BaseButton>
      </template>
    </BaseTable>
    <p class="mt-3 text-xs leading-relaxed text-cp-text-tertiary">
      按小时采样历史观测，显示最近 120 天内最多 16 个有记录的周期；无记录的周期不推算。百分比是最后一次上游观测，费用是已采集用量。倍率折算使用当前设置，历史扣款不变。提前重置只能定位到观测区间，区间内用量归属可能有误差。
    </p>
  </BaseCard>
</template>
