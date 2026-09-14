<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { getAccounts } from '@/api/modules/accounts'
import { portalRequest } from '@/api/modules/portal'

interface Daily { day: string, source: string, requests: number, inputTokens: string, outputTokens: string, cachedTokens: string, estimatedUsd: string | null, pricedRequests: number }
interface Device { id: string, name: string, accountId: string, enabled: boolean, lastSyncAt: string | null }
const rows = ref<Daily[]>([])
const devices = ref<Device[]>([])
const accounts = ref<{ id: string, name: string }[]>([])
const accountId = ref('')
const days = ref(7)
const busy = ref(false)
const error = ref('')
const deviceName = ref('我的电脑')
const deviceAccount = ref('')
const newToken = ref('')
const updated = ref('')
const cards = computed(() => ['local', 'proxy', 'all'].map((source) => {
  const selected = rows.value.filter(r => source === 'all' || r.source === source)
  return { source, tokens: selected.reduce((s, r) => s + BigInt(r.inputTokens) + BigInt(r.outputTokens), 0n).toLocaleString('zh-CN'), requests: selected.reduce((s, r) => s + r.requests, 0), usd: selected.reduce((s, r) => s + Number(r.estimatedUsd || 0), 0), priced: selected.reduce((s, r) => s + r.pricedRequests, 0) }
}))
const label = (source: string) => ({ local: '本地直连', proxy: '服务器代理', all: '合计' })[source] || source
async function load() {
  const end = new Date()
  const start = new Date(end.getTime() - days.value * 86400000)
  const query = new URLSearchParams({ startTime: start.toISOString(), endTime: end.toISOString() })
  if (accountId.value)
    query.set('accountId', accountId.value)
  rows.value = (await portalRequest<{ items: Daily[] }>(`/api/admin/usage/combined?${query}`)).items
  devices.value = (await portalRequest<{ items: Device[] }>('/api/admin/sync/devices')).items
  updated.value = new Date().toLocaleTimeString()
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
onMounted(() => action(async () => {
  accounts.value = (await getAccounts({ page: 1, pageSize: 1000 })).items
  await load()
}))
</script>

<template>
  <main class="combined">
    <header><div><h1>双端合并统计</h1><p>本地直连与服务器代理分别记账，共享账号额度不相加。</p></div><span v-if="updated">更新于 {{ updated }}</span></header>
    <p v-if="error" role="alert">
      {{ error }}；当前内容可能为上次加载的数据。
    </p>
    <div class="toolbar">
      <label for="combined-account">上游账号<select id="combined-account" v-model="accountId" :disabled="busy" @change="action(load)"><option value="">全部账号</option><option v-for="a in accounts" :key="a.id" :value="a.id">{{ a.name }}</option></select></label><label for="combined-period">时间<select id="combined-period" v-model="days" :disabled="busy" @change="action(load)"><option :value="1">最近一天</option><option :value="7">最近一周</option><option :value="30">最近一月</option><option :value="365">最近一年</option></select></label><button :disabled="busy" @click="action(load)">
        刷新
      </button>
    </div>
    <div class="cards">
      <section v-for="card in cards" :key="card.source">
        <h2>{{ label(card.source) }}</h2><strong class="number">{{ card.tokens }}</strong><p>tokens · {{ card.requests }} 次响应</p><p>API 等价 {{ card.priced ? `$${card.usd.toFixed(4)}` : '—' }} · 已计价 {{ card.priced }}/{{ card.requests }}</p>
      </section>
    </div>
    <section>
      <h2>每日用量</h2><div class="scroll">
        <table>
          <thead><tr><th>日期</th><th>来源</th><th>响应</th><th>输入</th><th>缓存（含于输入）</th><th>输出</th><th>API 等价 USD</th></tr></thead><tbody>
            <tr v-for="row in rows" :key="`${row.day}-${row.source}`">
              <td>{{ row.day }}</td><td>{{ label(row.source) }}</td><td>{{ row.requests }}</td><td>{{ BigInt(row.inputTokens).toLocaleString() }}</td><td>{{ BigInt(row.cachedTokens).toLocaleString() }}</td><td>{{ BigInt(row.outputTokens).toLocaleString() }}</td><td>{{ row.estimatedUsd ?? '—' }}</td>
            </tr>
          </tbody>
        </table>
      </div><p v-if="!rows.length">
        当前范围没有已同步的用量。
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
