<script setup lang="ts">
import type { Account } from '@/api/modules/accounts'
import { computed, onMounted, ref } from 'vue'
import { getAccounts } from '@/api/modules/accounts'
import { portalRequest } from '@/api/modules/portal'

interface Daily { day: string, source: string, requests: number, inputTokens: string, outputTokens: string, cachedTokens: string, estimatedUsd: string | null, pricedRequests: number }
interface Device { id: string, name: string, accountId: string, enabled: boolean, lastSyncAt: string | null }
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
  <main class="combined">
    <header><div><h1>双端合并统计</h1><p>本地直连与服务器代理分别记账，共享账号额度不相加。</p></div><span v-if="updated">更新于 {{ updated }}</span></header>
    <p v-if="error" role="alert">
      {{ error }}；部分内容未加载，请重试。
    </p>
    <div class="toolbar">
      <label for="combined-account">上游账号<select id="combined-account" v-model="accountId" :disabled="busy" @change="action(load)"><option value="">全部账号</option><option v-for="a in accounts" :key="a.id" :value="a.id">{{ a.name }}</option></select></label><label for="combined-period">时间<select id="combined-period" v-model="days" :disabled="busy" @change="action(load)"><option :value="1">最近一天</option><option :value="7">最近一周</option><option :value="30">最近一月</option><option :value="365">最近一年</option></select></label><button :disabled="busy" @click="action(load)">
        刷新
      </button>
    </div>
    <div class="toolbar">
      <label for="combined-source">数据来源<select id="combined-source" v-model="sourceFilter"><option value="all">双端合计</option><option value="local">仅本地直连</option><option value="proxy">仅服务器代理</option></select></label>
      <span>北京时间 · {{ days }} 天 · {{ activeDays }} 个活跃日</span><button :disabled="!loaded" @click="exportCsv">
        导出当前明细
      </button>
    </div>
    <div class="cards">
      <section v-for="card in cards" :key="card.source">
        <h2>{{ label(card.source) }}</h2><strong class="number">{{ loaded ? card.tokens : '—' }}</strong><p>tokens · {{ loaded ? card.requests : '—' }} 次响应</p><p>API 等价 {{ card.priced ? `$${card.usd.toFixed(4)}` : '—' }} · 已计价 {{ loaded ? `${card.priced}/${card.requests}` : '—' }}</p>
      </section>
    </div>
    <section>
      <div class="toolbar">
        <h2>双端使用趋势</h2><label for="trend-metric">指标<select id="trend-metric" v-model="metric"><option value="tokens">Tokens</option><option value="cost">API 等价 USD</option><option value="requests">响应次数</option></select></label>
      </div>
      <p>绿色为本地直连，蓝色为服务器代理。点击某天查看明细；未计价请求不计入费用趋势。</p>
      <div class="trend" role="group" aria-label="每日用量趋势">
        <button v-for="day in calendar" :key="day.day" class="bar" :title="`${day.day} · 本地 ${metricLabel(day.local)} · 代理 ${metricLabel(day.proxy)}`" :aria-label="`${day.day} ${metricLabel(day.local + day.proxy)}`" @click="selectedDay = selectedDay === day.day ? '' : day.day">
          <span class="proxy-bar" :style="{ height: `${day.proxy / peak * 160}px` }" /><span class="local-bar" :style="{ height: `${day.local / peak * 160}px` }" />
        </button>
      </div>
      <div class="toolbar">
        <span>{{ calendar[0]?.day }}</span><span>{{ calendar.at(-1)?.day }}</span>
      </div>
      <h3>用量热力图</h3><div class="heatmap">
        <button v-for="day in calendar" :key="day.day" :aria-label="`${day.day} ${metricLabel(day.local + day.proxy)}`" :title="`${day.day} · ${metricLabel(day.local + day.proxy)}`" :style="{ background: day.local + day.proxy ? `rgba(38, 145, 101, ${0.2 + 0.8 * (day.local + day.proxy) / peak})` : 'var(--cp-color-bg-layout)' }" @click="selectedDay = selectedDay === day.day ? '' : day.day" />
      </div>
    </section>
    <section>
      <h2>共享账号额度</h2><p>这是上游账号的统一额度，两端共用。分别展示每个账号，不把额度百分比相加。</p>
      <div v-for="account in scopedAccounts" :key="account.id" class="quota-account">
        <h3>{{ account.name }}</h3><p>观测时间 {{ account.quota.refreshedAtDisplay }}</p>
        <div v-for="window in account.quota.windows" :key="window.key">
          <div class="toolbar">
            <span>{{ window.labelDisplay }} · {{ window.windowLabelDisplay }}</span><strong>{{ window.usedPercentDisplay }}</strong>
          </div><label :for="`quota-${account.id}-${window.key}`">额度已用</label><progress :id="`quota-${account.id}-${window.key}`" :value="window.usedPercent ?? undefined" max="100" /><p>重置 {{ window.resetAtDisplay }}</p>
        </div>
        <p v-if="!account.quota.windows.length">
          暂无额度观测数据
        </p>
      </div>
    </section>
    <section>
      <div class="toolbar">
        <h2>{{ selectedDay || '每日' }}用量明细</h2><button v-if="selectedDay" @click="selectedDay = ''">
          显示全部日期
        </button>
      </div><div class="scroll">
        <table>
          <thead><tr><th>日期</th><th>来源</th><th>响应</th><th>输入</th><th>缓存（含于输入）</th><th>输出</th><th>API 等价 USD</th></tr></thead><tbody>
            <tr v-for="row in filteredRows" :key="`${row.day}-${row.source}`">
              <td>{{ row.day }}</td><td>{{ label(row.source) }}</td><td>{{ row.requests }}</td><td>{{ BigInt(row.inputTokens).toLocaleString() }}</td><td>{{ BigInt(row.cachedTokens).toLocaleString() }}</td><td>{{ BigInt(row.outputTokens).toLocaleString() }}</td><td>{{ row.estimatedUsd ?? '—' }}</td>
            </tr>
          </tbody>
        </table>
      </div><p v-if="!filteredRows.length">
        {{ loaded ? '当前范围没有已同步的用量。' : '用量尚未加载。' }}
      </p>
    </section>
    <section>
      <h2>同步设备</h2><p>设备绑定本机登录的同一个上游账号。停用设备不会删除历史记录。</p><div v-for="d in devices" :key="d.id" class="toolbar">
        <span>{{ d.name }} · {{ d.enabled ? '启用' : '停用' }} · {{ d.lastSyncAt ? `最后同步 ${new Date(d.lastSyncAt).toLocaleString()}` : '尚未同步' }}</span><button :disabled="busy" @click="toggle(d)">
          {{ d.enabled ? '停用' : '启用' }}
        </button>
      </div>
      <form class="toolbar" @submit.prevent="create">
        <label for="device-name">设备名称<input id="device-name" v-model="deviceName" required maxlength="100"></label><label for="device-account">绑定账号<select id="device-account" v-model="deviceAccount" required><option value="" disabled>选择本机使用的账号</option><option v-for="a in accounts" :key="a.id" :value="a.id">{{ a.name }}</option></select></label><button :disabled="busy">
          创建同步凭据
        </button>
      </form><div v-if="newToken">
        <p>新凭据仅本次显示，用于配置 Lens 同步。</p><code>{{ newToken }}</code><button @click="newToken = ''">
          隐藏
        </button>
      </div>
    </section>
  </main>
</template>

<style scoped>
.trend {
  display: flex;
  align-items: end;
  height: 180px;
  gap: 3px;
  overflow-x: auto;
  border-bottom: 1px solid var(--cp-color-border);
}
.bar {
  display: flex;
  flex-direction: column;
  justify-content: end;
  flex: 1;
  min-width: 6px;
  height: 175px;
  padding: 0 !important;
  border: 0 !important;
  background: transparent !important;
}
.bar span {
  display: block;
  width: 100%;
  min-height: 1px;
}
.local-bar {
  background: #269165;
}
.proxy-bar {
  background: #508cd5;
}
.heatmap {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 12px;
}
.heatmap button {
  width: 16px;
  height: 16px;
  padding: 0;
  border-radius: 3px;
}
.quota-account {
  padding: 16px 0;
  border-bottom: 1px solid var(--cp-color-border);
}
progress {
  width: 100%;
  height: 9px;
  accent-color: #269165;
}
.combined {
  padding: 28px;
  color: var(--cp-color-text);
}
header,
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}
.toolbar {
  justify-content: flex-start;
  margin: 16px 0;
}
h1 {
  font-size: 26px;
  font-weight: 650;
}
h2 {
  font-size: 18px;
  font-weight: 600;
}
p {
  color: var(--cp-color-text-secondary);
  margin: 10px 0;
}
section {
  background: var(--cp-color-bg-container);
  border-radius: 14px;
  padding: 22px;
  margin-top: 22px;
}
.cards {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 18px;
}
.number {
  display: block;
  font-size: 28px;
  margin-top: 18px;
  overflow-wrap: anywhere;
}
label {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
input,
select,
button {
  padding: 9px 12px;
  border: 1px solid var(--cp-color-border);
  border-radius: 8px;
  background: var(--cp-color-bg-container);
  color: inherit;
}
button {
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
}
.scroll {
  overflow: auto;
}
table {
  width: 100%;
  white-space: nowrap;
}
td,
th {
  padding: 12px;
  text-align: left;
  border-bottom: 1px solid var(--cp-color-border);
}
code {
  overflow-wrap: anywhere;
}
@media (max-width: 800px) {
  .cards {
    grid-template-columns: 1fr;
  }
  .combined {
    padding: 16px;
  }
}
</style>
