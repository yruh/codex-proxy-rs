<script setup lang="ts">
import type { Account } from '@/api/modules/accounts'
import type { BaseTableColumn } from '@/components/base/BaseTable/columns'
import { Download, Laptop, Plus, RefreshCw, Server, Sigma } from '@lucide/vue'
import { computed, onMounted, ref } from 'vue'
import { getAccounts } from '@/api/modules/accounts'
import { portalRequest } from '@/api/modules/portal'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseCard from '@/components/base/BaseCard.vue'
import FormItem from '@/components/base/BaseForm/FormItem.vue'
import BaseInput from '@/components/base/BaseInput.vue'
import BaseModal from '@/components/base/BaseModal/index.vue'
import BasePageHeader from '@/components/base/BasePageHeader.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseTable from '@/components/base/BaseTable/index.vue'

interface Daily { day: string, source: string, requests: number, inputTokens: string, outputTokens: string, cachedTokens: string, estimatedUsd: string | null, pricedRequests: number }
interface Device { id: string, name: string, accountId: string, enabled: boolean, lastSyncAt: string | null }
const showDevice = ref(false)
const columns: BaseTableColumn<Daily>[] = [
  { key: 'day', label: '日期', size: 'lg' },
  { key: 'source', label: '来源', size: 'lg' },
  { key: 'requests', label: '响应', kind: 'numeric' },
  { key: 'inputTokens', label: '输入', kind: 'numeric' },
  { key: 'cachedTokens', label: '缓存（含于输入）', kind: 'numeric', size: 'xl' },
  { key: 'outputTokens', label: '输出', kind: 'numeric' },
  { key: 'estimatedUsd', label: 'API 等价 USD', kind: 'numeric', size: 'lg' },
]
const rows = ref<Daily[]>([])
const devices = ref<Device[]>([])
const accounts = ref<Account[]>([])
const accountId = ref('')
const days = ref(7)
const label = (source: string) => ({ local: '本地直连', proxy: '服务器代理', all: '合计' })[source] || source
const sourceFilter = ref('all')
const selectedDay = ref('')
const metric = ref<'tokens' | 'cost' | 'requests'>('tokens')
const dayKey = (date: Date) => new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Shanghai', year: 'numeric', month: '2-digit', day: '2-digit' }).format(date)
const filteredRows = computed(() => rows.value.filter(r => (sourceFilter.value === 'all' || r.source === sourceFilter.value) && (!selectedDay.value || r.day === selectedDay.value)))
const calendar = computed(() => {
  const today = new Date(`${dayKey(new Date())}T00:00:00+08:00`)
  return Array.from({ length: days.value }, (_, i) => {
    const day = dayKey(new Date(today.getTime() - (days.value - 1 - i) * 86400000))
    const values = rows.value.filter(r => r.day === day && (sourceFilter.value === 'all' || r.source === sourceFilter.value))
    return { day, local: values.filter(r => r.source === 'local').reduce((s, r) => s + metricValue(r), 0), proxy: values.filter(r => r.source === 'proxy').reduce((s, r) => s + metricValue(r), 0) }
  })
})
const peak = computed(() => Math.max(1, ...calendar.value.map(d => d.local + d.proxy)))
const activeDays = computed(() => calendar.value.filter(d => d.local + d.proxy > 0).length)
const scopedAccounts = computed(() => accounts.value.filter(a => !accountId.value || a.id === accountId.value))
function metricValue(r: Daily) {
  return metric.value === 'tokens' ? Number(r.inputTokens) + Number(r.outputTokens) : metric.value === 'cost' ? Number(r.estimatedUsd || 0) : r.requests
}
function metricLabel(n: number) {
  return metric.value === 'cost' ? `$${n.toFixed(4)}` : n.toLocaleString('zh-CN', { maximumFractionDigits: 0 })
}
function exportCsv() {
  const csv = ['日期,来源,响应,输入,缓存,输出,USD,已计价响应', ...filteredRows.value.map(r => [r.day, label(r.source), r.requests, r.inputTokens, r.cachedTokens, r.outputTokens, r.estimatedUsd ?? '', r.pricedRequests].join(','))].join('\r\n')
  const url = URL.createObjectURL(new Blob(['\uFEFF', csv], { type: 'text/csv;charset=utf-8' }))
  const a = document.createElement('a')
  a.href = url
  a.download = 'combined-usage.csv'
  a.click()
  URL.revokeObjectURL(url)
}
const busy = ref(false)
const error = ref('')
const deviceName = ref('我的电脑')
const deviceAccount = ref('')
const newToken = ref('')
const updated = ref('')
const loaded = ref(false)
const cards = computed(() => ['local', 'proxy', 'all'].map((source) => {
  const selected = filteredRows.value.filter(r => source === 'all' || r.source === source)
  return { source, tokens: selected.reduce((s, r) => s + BigInt(r.inputTokens) + BigInt(r.outputTokens), 0n).toLocaleString('zh-CN'), requests: selected.reduce((s, r) => s + r.requests, 0), usd: selected.reduce((s, r) => s + Number(r.estimatedUsd || 0), 0), priced: selected.reduce((s, r) => s + r.pricedRequests, 0) }
}))
async function loadAccounts() {
  const items: Account[] = []
  for (let page = 1; ; page++) {
    const result = await getAccounts({ page, pageSize: 200 })
    items.push(...result.items)
    if (page >= result.page.totalPages || !result.items.length)
      break
  }
  accounts.value = items
}
async function load() {
  const end = new Date()
  const start = new Date(new Date(`${dayKey(end)}T00:00:00+08:00`).getTime() - (days.value - 1) * 86400000)
  selectedDay.value = ''
  const query = new URLSearchParams({ startTime: start.toISOString(), endTime: end.toISOString() })
  if (accountId.value)
    query.set('accountId', accountId.value)
  const results = await Promise.allSettled([
    portalRequest<{ items: Daily[] }>(`/api/admin/usage/combined?${query}`).then((result) => {
      rows.value = result.items
      loaded.value = true
      updated.value = new Date().toLocaleTimeString()
    }),
    portalRequest<{ items: Device[] }>('/api/admin/sync/devices').then((result) => { devices.value = result.items }),
    loadAccounts(),
  ])
  const failure = results.find(result => result.status === 'rejected')
  if (failure?.status === 'rejected')
    throw failure.reason
}
async function action(work: () => Promise<void>) {
  if (busy.value)
    return
  busy.value = true
  error.value = ''
  try {
    await work()
  }
  catch (e) { error.value = e instanceof Error ? e.message : '查询失败' }
  finally { busy.value = false }
}
async function create() {
  await action(async () => {
    if (!deviceAccount.value)
      throw new Error('请选择绑定账号')
    const result = await portalRequest<{ token: string }>('/api/admin/sync/devices', { name: deviceName.value, accountId: deviceAccount.value })
    newToken.value = result.token
    await load()
  })
}
async function toggle(d: Device) {
  await action(async () => {
    await portalRequest('/api/admin/sync/devices/update', { id: d.id, enabled: !d.enabled })
    await load()
  })
}
onMounted(() => action(load))
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-5">
    <BasePageHeader title="双端合并统计" description="本地直连与服务器代理，一个视图掌握用量">
      <template #actions>
        <BaseButton :disabled="!loaded" @click="exportCsv">
          <template #icon>
            <Download :size="16" />
          </template>导出明细
        </BaseButton><BaseButton :loading="busy" @click="action(load)">
          <template #icon>
            <RefreshCw :size="16" />
          </template>刷新
        </BaseButton>
      </template>
    </BasePageHeader>
    <p v-if="error && !showDevice" role="alert" class="rounded-cp bg-cp-error-container p-3 text-sm text-cp-error-on-container">
      {{ error }}；部分内容未加载，请重试。
    </p>
    <BaseCard padding="compact">
      <div class="flex flex-wrap items-end gap-3">
        <FormItem label="上游账号" class="min-w-48 flex-1">
          <BaseSelect v-model="accountId" :disabled="busy" :options="[{ label: '全部账号', value: '' }, ...accounts.map(a => ({ label: a.name, value: a.id }))]" @update:model-value="action(load)" />
        </FormItem>
        <FormItem label="时间范围" class="min-w-36">
          <BaseSelect :model-value="String(days)" :disabled="busy" :options="[{ label: '最近一天', value: '1' }, { label: '最近一周', value: '7' }, { label: '最近一月', value: '30' }, { label: '最近一年', value: '365' }]" @update:model-value="days = Number($event); action(load)" />
        </FormItem>
        <FormItem label="数据来源" class="min-w-40">
          <BaseSelect v-model="sourceFilter" :options="[{ label: '双端合计', value: 'all' }, { label: '本地直连', value: 'local' }, { label: '服务器代理', value: 'proxy' }]" />
        </FormItem>
        <p class="pb-2 text-xs text-cp-text-tertiary">
          北京时间 <span v-if="updated">· 更新于 {{ updated }}</span>
        </p>
      </div>
    </BaseCard>
    <div class="grid gap-4 md:grid-cols-3">
      <BaseCard v-for="card in cards" :key="card.source">
        <div class="flex items-center justify-between">
          <span class="text-sm font-semibold text-cp-text-secondary">{{ label(card.source) }}</span><component :is="card.source === 'local' ? Laptop : card.source === 'proxy' ? Server : Sigma" :size="19" class="text-cp-primary" />
        </div>
        <p class="mt-4 break-all text-3xl font-bold tabular-nums">
          {{ loaded ? card.tokens : '—' }} <span class="text-xs font-normal text-cp-text-tertiary">tokens</span>
        </p>
        <div class="mt-4 flex flex-wrap justify-between gap-2 text-xs text-cp-text-secondary">
          <span>{{ loaded ? card.requests.toLocaleString() : '—' }} 次响应</span><span>API 等价 {{ card.priced ? `${card.usd.toFixed(4)} USD` : '—' }}</span>
        </div>
        <p class="mt-2 text-xs text-cp-text-tertiary">
          已计价 {{ loaded ? `${card.priced}/${card.requests}` : '—' }}
        </p>
      </BaseCard>
    </div>
    <BaseCard title="用量趋势" :description="`${days} 天内 · ${activeDays} 个活跃日`">
      <template #actions>
        <BaseSelect v-model="metric" :options="[{ label: 'Tokens', value: 'tokens' }, { label: 'API 等价 USD', value: 'cost' }, { label: '响应次数', value: 'requests' }]" aria-label="趋势指标" />
      </template>
      <div class="mb-4 flex flex-wrap gap-4 text-xs text-cp-text-secondary">
        <span class="flex items-center gap-2"><i class="size-2 rounded-full bg-cp-success" />本地直连</span><span class="flex items-center gap-2"><i class="size-2 rounded-full bg-cp-info" />服务器代理</span><span class="text-cp-text-tertiary">点击日期筛选明细</span>
      </div>
      <div class="flex h-48 items-end gap-1 overflow-x-auto" role="group" aria-label="每日用量趋势">
        <button v-for="day in calendar" :key="day.day" type="button" class="flex h-full min-w-2 flex-1 flex-col justify-end rounded-t-sm bg-transparent outline-offset-2 hover:bg-cp-fill-quaternary focus-visible:outline-2 focus-visible:outline-cp-control-outline" :title="`${day.day} · 本地 ${metricLabel(day.local)} · 代理 ${metricLabel(day.proxy)}`" :aria-label="`${day.day} ${metricLabel(day.local + day.proxy)}`" :aria-pressed="selectedDay === day.day" @click="selectedDay = selectedDay === day.day ? '' : day.day">
          <span class="block w-full rounded-t-sm bg-cp-info" :style="{ height: `${day.proxy / peak * 170}px` }" /><span class="block w-full bg-cp-success" :style="{ height: `${day.local / peak * 170}px` }" />
        </button>
      </div>
      <div class="mt-2 flex justify-between text-xs text-cp-text-tertiary">
        <span>{{ calendar[0]?.day }}</span><span>{{ calendar.at(-1)?.day }}</span>
      </div>
      <div class="mt-6 flex flex-wrap items-center justify-between gap-2">
        <h3 class="text-sm font-semibold">
          每日活跃度
        </h3><span class="text-xs text-cp-text-tertiary">颜色越深，用量越多 · 未计价记录不计入费用趋势</span>
      </div>
      <div class="mt-3 flex flex-wrap gap-1.5">
        <button v-for="day in calendar" :key="day.day" type="button" class="size-4 rounded-sm outline-offset-2 focus-visible:outline-2 focus-visible:outline-cp-control-outline" :class="selectedDay === day.day ? 'ring-2 ring-cp-primary' : ''" :aria-pressed="selectedDay === day.day" :aria-label="`${day.day} ${metricLabel(day.local + day.proxy)}`" :title="`${day.day} · ${metricLabel(day.local + day.proxy)}`" :style="{ background: day.local + day.proxy ? `color-mix(in srgb, var(--cp-color-success) ${22 + 78 * (day.local + day.proxy) / peak}%, var(--cp-color-success-container))` : 'var(--cp-color-fill-tertiary)' }" @click="selectedDay = selectedDay === day.day ? '' : day.day" />
      </div>
    </BaseCard>
    <BaseCard :title="`${selectedDay || '每日'}用量明细`" description="输入包含缓存；本地与代理分别记账后合计">
      <template v-if="selectedDay" #actions>
        <BaseButton size="sm" @click="selectedDay = ''">
          清除日期筛选
        </BaseButton>
      </template>
      <BaseTable :columns="columns" :rows="filteredRows" :row-key="row => `${row.day}-${row.source}`" :loading="busy && !loaded" :empty-text="loaded ? '当前范围没有已同步用量' : '用量尚未加载'">
        <template #source="{ row }">
          <span class="rounded-cp-sm px-2 py-1 text-xs font-semibold" :class="row.source === 'local' ? 'bg-cp-success-container text-cp-success-on-container' : 'bg-cp-info-container text-cp-info-on-container'">{{ label(row.source) }}</span>
        </template>
      </BaseTable>
    </BaseCard>
    <BaseCard title="共享账号额度" description="两端共用上游额度，每个账号单独展示">
      <p v-if="!scopedAccounts.length" class="py-6 text-center text-sm text-cp-text-tertiary">
        暂无账号数据
      </p>
      <div class="grid gap-4 lg:grid-cols-2">
        <div v-for="account in scopedAccounts" :key="account.id" class="rounded-cp bg-cp-fill-quaternary p-4">
          <h3 class="font-semibold">
            {{ account.name }}
          </h3><p class="mt-1 text-xs text-cp-text-tertiary">
            观测于 {{ account.quota.refreshedAtDisplay }}
          </p>
          <div v-for="window in account.quota.windows" :key="window.key" class="mt-4">
            <div class="mb-2 flex justify-between gap-3 text-xs">
              <span class="text-cp-text-secondary">{{ window.labelDisplay }} · {{ window.windowLabelDisplay }}</span><strong>{{ window.usedPercentDisplay }}</strong>
            </div>
            <div class="h-1.5 overflow-hidden rounded-full bg-cp-fill-tertiary" role="progressbar" :aria-label="window.labelDisplay" :aria-valuenow="window.usedPercent ?? undefined" :aria-valuemin="0" :aria-valuemax="100">
              <div class="h-full rounded-full bg-cp-primary" :style="{ width: `${Math.max(0, Math.min(100, window.usedPercent ?? 0))}%` }" />
            </div>
            <p class="mt-2 text-xs text-cp-text-tertiary">
              重置 {{ window.resetAtDisplay }}
            </p>
          </div>
          <p v-if="!account.quota.windows.length" class="mt-4 text-xs text-cp-text-tertiary">
            暂无额度观测
          </p>
        </div>
      </div>
    </BaseCard>
    <BaseCard title="同步设备" description="关联本机登录的上游账号，停用保留历史用量">
      <template #actions>
        <BaseButton @click="showDevice = true; error = ''; newToken = ''">
          <template #icon>
            <Plus :size="16" />
          </template>添加设备
        </BaseButton>
      </template>
      <p v-if="!devices.length" class="py-6 text-center text-sm text-cp-text-tertiary">
        尚未添加同步设备
      </p>
      <div v-for="d in devices" :key="d.id" class="mb-2 flex flex-wrap items-center justify-between gap-3 rounded-cp bg-cp-fill-quaternary p-4">
        <div class="flex items-center gap-3">
          <Laptop class="text-cp-text-tertiary" :size="20" /><div>
            <p class="font-semibold">
              {{ d.name }} <span class="ml-2 text-xs font-normal" :class="d.enabled ? 'text-cp-success-text' : 'text-cp-text-tertiary'">{{ d.enabled ? '已启用' : '已停用' }}</span>
            </p><p class="mt-1 text-xs text-cp-text-tertiary">
              {{ d.lastSyncAt ? `最后同步 ${new Date(d.lastSyncAt).toLocaleString()}` : '等待首次同步' }}
            </p>
          </div>
        </div><BaseButton size="sm" :disabled="busy" @click="toggle(d)">
          {{ d.enabled ? '停用' : '启用' }}
        </BaseButton>
      </div>
    </BaseCard>
    <BaseModal v-model="showDevice" title="添加同步设备" description="绑定本机 Codex 登录的同一个上游账号" :dismissible="!busy">
      <p v-if="error" role="alert" class="mb-4 text-sm text-cp-error-text">
        {{ error }}
      </p>
      <div v-if="newToken" class="space-y-4">
        <p class="text-sm text-cp-text-secondary">
          凭据仅本次显示，请复制到 Lens 同步设置。
        </p><code class="block break-all rounded-cp bg-cp-fill-tertiary p-4 text-sm">{{ newToken }}</code><BaseButton @click="newToken = ''; showDevice = false">
          已保存，关闭
        </BaseButton>
      </div>
      <form v-else id="create-device" class="space-y-5" @submit.prevent="create">
        <FormItem label="设备名称" required>
          <BaseInput v-model="deviceName" maxlength="100" />
        </FormItem><FormItem label="绑定账号" required>
          <BaseSelect v-model="deviceAccount" :options="accounts.map(a => ({ label: a.name, value: a.id }))" placeholder="选择本机使用的账号" />
        </FormItem>
      </form>
      <template v-if="!newToken" #footer>
        <BaseButton :disabled="busy" @click="showDevice = false">
          取消
        </BaseButton><BaseButton type="submit" form="create-device" variant="primary" :loading="busy">
          创建同步凭据
        </BaseButton>
      </template>
    </BaseModal>
  </div>
</template>
