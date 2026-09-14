<script setup lang="ts">
import type { PortalKey, PortalUsage, PortalUser, WalletResponse } from '@/api/modules/portal'
import { onMounted, ref } from 'vue'
import { portalRequest } from '@/api/modules/portal'

const user = ref<PortalUser | null>(null)
const username = ref('')
const password = ref('')
const error = ref('')
const busy = ref(false)
const keys = ref<PortalKey[]>([])
const rows = ref<PortalUsage[]>([])
const walletData = ref<WalletResponse | null>(null)
const revealed = ref<string[]>([])
const days = ref(7)
const page = ref(1)
const oldPassword = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
const notice = ref('')
async function changePassword() {
  await action(async () => {
    if (newPassword.value !== confirmPassword.value)
      throw new Error('两次输入的新密码不一致')
    await portalRequest('/api/portal/password', { oldPassword: oldPassword.value, newPassword: newPassword.value })
    oldPassword.value = newPassword.value = confirmPassword.value = ''
    user.value = null
    keys.value = []
    rows.value = []
    revealed.value = []
    notice.value = '密码已修改，所有旧登录已失效，请使用新密码登录。'
  })
}
const fmt = (n: number | null) => n == null ? '—' : n.toLocaleString('zh-CN')

async function load() {
  const end = new Date()
  const start = new Date(end.getTime() - days.value * 86400000)
  const query = new URLSearchParams({ startTime: start.toISOString(), endTime: end.toISOString(), page: String(page.value) })
  const [keyData, usageData, wallet] = await Promise.all([
    portalRequest<{ items: PortalKey[] }>('/api/portal/keys'),
    portalRequest<{ items: PortalUsage[] }>(`/api/portal/usage?${query}`),
    portalRequest<WalletResponse>('/api/portal/wallet'),
  ])
  keys.value = keyData.items
  rows.value = usageData.items
  walletData.value = wallet
}
async function action(work: () => Promise<void>) {
  if (busy.value)
    return
  busy.value = true
  error.value = ''
  try {
    await work()
  }
  catch (e) { error.value = e instanceof Error ? e.message : '操作失败' }
  finally { busy.value = false }
}
async function login() {
  await action(async () => {
    user.value = await portalRequest<PortalUser>('/api/portal/login', { username: username.value, password: password.value })
    password.value = ''
    await load()
  })
}
async function logout() {
  await action(async () => {
    await portalRequest('/api/portal/logout', {})
    user.value = null
    keys.value = []
    rows.value = []
    revealed.value = []
  })
}
function toggle(id: string) {
  revealed.value = revealed.value.includes(id) ? revealed.value.filter(v => v !== id) : [...revealed.value, id]
}
onMounted(async () => {
  try {
    user.value = await portalRequest<PortalUser>('/api/portal/status')
  }
  catch { return }
  await action(load)
})
</script>

<template>
  <main class="portal">
    <header>
      <div><h1>Codex Proxy RS</h1><p>{{ user ? `${user.username} · 我的用量` : '用户登录' }}</p></div><button v-if="user" :disabled="busy" @click="logout">
        退出登录
      </button><RouterLink v-else to="/login">
        管理员入口
      </RouterLink>
    </header>
    <p v-if="error" role="alert" class="error">
      {{ error }}
    </p>
    <p v-if="notice" role="status">
      {{ notice }}
    </p>
    <form v-if="!user" class="card login" @submit.prevent="login">
      <h2>欢迎回来</h2><p>登录查看分配给你的密钥和调用记录。</p>
      <label for="portal-field-1">用户名<input id="portal-field-1" v-model="username" required autocomplete="username" maxlength="100"></label>
      <label for="portal-field-2">密码<input id="portal-field-2" v-model="password" required type="password" autocomplete="current-password" maxlength="1024"></label>
      <button :disabled="busy">
        {{ busy ? '正在登录…' : '登录' }}
      </button>
    </form>
    <template v-else>
      <section v-if="walletData" class="card">
        <h2>我的余额 · ${{ walletData.wallet.balanceUsd }}</h2>
        <p>累计消费 ${{ walletData.wallet.totalSpentUsd }} · {{ walletData.wallet.balanceEnforced ? '余额不足后暂停新请求' : '未开启余额限制' }}</p>
        <p>今日 ${{ walletData.wallet.dailyUsedUsd }} / {{ Number(walletData.wallet.dailyLimitUsd) ? `$${walletData.wallet.dailyLimitUsd}` : '不限' }} · 本周 ${{ walletData.wallet.weeklyUsedUsd }} / {{ Number(walletData.wallet.weeklyLimitUsd) ? `$${walletData.wallet.weeklyLimitUsd}` : '不限' }}</p>
        <p>共享并发 {{ walletData.wallet.activeRequests }} / {{ walletData.wallet.maxConcurrency || '不限' }}。所有密钥共享；日／周按北京时间重置，进行中的请求按实际费用结算。</p>
        <details>
          <summary>充值与扣费流水（最近 100 笔）</summary><p v-for="event in walletData.events" :key="event.id">
            {{ new Date(event.createdAt).toLocaleString() }} · {{ event.kind === 'credit' ? '充值' : '扣费' }} ${{ event.amountUsd }} · {{ event.note }}
          </p>
        </details>
      </section>
      <form class="card" @submit.prevent="changePassword">
        <h2>修改密码</h2>
        <label for="old-password">当前密码<input id="old-password" v-model="oldPassword" type="password" autocomplete="current-password" required maxlength="1024"></label>
        <label for="new-password">新密码<input id="new-password" v-model="newPassword" type="password" autocomplete="new-password" required minlength="12" maxlength="1024"></label>
        <label for="confirm-password">确认新密码<input id="confirm-password" v-model="confirmPassword" type="password" autocomplete="new-password" required minlength="12" maxlength="1024"></label>
        <p>至少 12 个字符。修改后需要重新登录。</p><button :disabled="busy">
          修改密码
        </button>
      </form>
      <section class="card">
        <h2>我的密钥</h2><p v-if="!keys.length">
          管理员尚未分配密钥。
        </p><div v-for="key in keys" :key="key.id" class="key">
          <div><strong>{{ key.name }}</strong><p>{{ key.enabled ? '可用' : '已停用' }}</p><code>{{ revealed.includes(key.id) ? key.key : '••••••••••••••••' }}</code></div><button @click="toggle(key.id)">
            {{ revealed.includes(key.id) ? '隐藏' : '显示密钥' }}
          </button>
        </div>
      </section>
      <section class="card">
        <div class="toolbar">
          <h2>我的调用记录</h2><label for="portal-field-3">时间范围<select id="portal-field-3" v-model="days" :disabled="busy" @change="page = 1; action(load)"><option :value="1">最近一天</option><option :value="7">最近一周</option><option :value="30">最近一月</option></select></label><button :disabled="busy" @click="action(load)">
            刷新
          </button>
        </div><p>仅包含分配给你的密钥通过代理完成的调用；金额为 API 等价估算。</p>
        <div class="scroll">
          <table>
            <thead><tr><th>时间</th><th>模型</th><th>输入</th><th>缓存</th><th>输出</th><th>估算 USD</th></tr></thead><tbody>
              <tr v-for="row in rows" :key="row.id">
                <td>{{ new Date(row.occurredAt).toLocaleString() }}</td><td>{{ row.model || '未记录' }}</td><td>{{ fmt(row.inputTokens) }}</td><td>{{ fmt(row.cachedTokens) }}</td><td>{{ fmt(row.outputTokens) }}</td><td>{{ row.estimatedUsd ?? '—' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-if="!rows.length">
          当前范围没有记录。
        </p><div class="toolbar">
          <span>第 {{ page }} 页 · 每页最多 100 条</span><button :disabled="busy || page <= 1" @click="page--; action(load)">
            上一页
          </button><button :disabled="busy || rows.length < 100" @click="page++; action(load)">
            下一页
          </button>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped>
.portal {
  min-height: 100dvh;
  padding: 32px max(20px, calc((100vw - 1180px) / 2));
  background: var(--cp-color-bg-layout);
  color: var(--cp-color-text);
}
header,
.toolbar,
.key {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}
h1 {
  font-size: 24px;
  font-weight: 650;
}
h2 {
  font-size: 20px;
  font-weight: 600;
}
p {
  color: var(--cp-color-text-secondary);
  margin: 8px 0;
}
.card {
  background: var(--cp-color-bg-container);
  padding: 24px;
  margin-top: 24px;
  border-radius: 16px;
}
.login {
  max-width: 440px;
  margin: 60px auto;
}
label {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 12px 0;
}
input,
select,
button {
  padding: 10px 14px;
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
  cursor: default;
}
.key {
  padding: 18px 0;
  border-bottom: 1px solid var(--cp-color-border);
}
code {
  overflow-wrap: anywhere;
}
.scroll {
  overflow: auto;
}
table {
  width: 100%;
  white-space: nowrap;
}
th,
td {
  padding: 14px;
  text-align: left;
  border-bottom: 1px solid var(--cp-color-border);
}
.error {
  color: var(--cp-color-error-text);
}
</style>
