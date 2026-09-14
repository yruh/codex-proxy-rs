<script setup lang="ts">
import type { PortalUser, WalletPolicy, WalletResponse } from '@/api/modules/portal'
import { onMounted, ref } from 'vue'
import { getApiKeys } from '@/api/modules/api-keys'
import { portalRequest } from '@/api/modules/portal'

const availableKeys = ref<{ id: string, name: string }[]>([])

const users = ref<PortalUser[]>([])
const username = ref('')
const password = ref('')
const keyId = ref('')
const userId = ref('')
const busy = ref(false)
const message = ref('')
const selectedUser = ref<PortalUser | null>(null)
const walletData = ref<WalletResponse | null>(null)
const policy = ref<WalletPolicy>({ balanceEnforced: false, dailyLimitUsd: '0', weeklyLimitUsd: '0', maxConcurrency: 0 })
const credit = ref('')
const creditNote = ref('')
const creditOperation = ref('')
async function openWallet(user: PortalUser) {
  await action(async () => {
    selectedUser.value = user
    walletData.value = null
    credit.value = creditNote.value = creditOperation.value = ''
    await reloadWallet()
  })
}
async function reloadWallet() {
  if (!selectedUser.value)
    return
  walletData.value = await portalRequest<WalletResponse>(`/api/admin/portal/wallet?userId=${encodeURIComponent(selectedUser.value.id)}`)
  const w = walletData.value.wallet
  policy.value = { balanceEnforced: w.balanceEnforced, dailyLimitUsd: w.dailyLimitUsd, weeklyLimitUsd: w.weeklyLimitUsd, maxConcurrency: w.maxConcurrency }
}
async function savePolicy() {
  await action(async () => {
    await portalRequest('/api/admin/portal/wallet/policy', { userId: selectedUser.value!.id, policy: policy.value })
    await reloadWallet()
    message.value = '用户共享限制已保存'
  })
}
async function recharge() {
  await action(async () => {
    creditOperation.value ||= crypto.randomUUID()
    await portalRequest('/api/admin/portal/wallet/credit', { userId: selectedUser.value!.id, operationId: creditOperation.value, amount: credit.value, note: creditNote.value })
    credit.value = creditNote.value = creditOperation.value = ''
    await reloadWallet()
    message.value = '充值成功，已记录流水'
  })
}
async function load() {
  users.value = (await portalRequest<{ items: PortalUser[] }>('/api/admin/portal/users')).items
  availableKeys.value = (await getApiKeys({ limit: 1000 })).items
}
async function action(work: () => Promise<void>) {
  if (busy.value)
    return
  busy.value = true
  message.value = ''
  try {
    await work()
    await load()
  }
  catch (e) { message.value = e instanceof Error ? e.message : '操作失败' }
  finally { busy.value = false }
}
async function create() {
  await action(async () => {
    await portalRequest('/api/admin/portal/users', { username: username.value, password: password.value })
    username.value = ''
    password.value = ''
    message.value = '用户已创建'
  })
}
async function assign() {
  await action(async () => {
    await portalRequest('/api/admin/portal/keys/assign', { keyId: keyId.value, userId: userId.value })
    keyId.value = ''
    message.value = '密钥已分配'
  })
}
async function toggle(user: PortalUser) {
  await action(async () => {
    await portalRequest('/api/admin/portal/users/update', { id: user.id, enabled: !user.enabled })
  })
}
const resetId = ref('')
const resetPassword = ref('')
async function reset() {
  const user = users.value.find(u => u.id === resetId.value)
  if (!user)
    return
  await action(async () => {
    await portalRequest('/api/admin/portal/users/update', { id: user.id, enabled: user.enabled, password: resetPassword.value })
    resetPassword.value = ''
    resetId.value = ''
    message.value = '密码已重置，旧会话已失效'
  })
}
onMounted(() => action(load))
</script>

<template>
  <main class="users">
    <header>
      <h1>用户管理</h1><p>为同学创建独立登录身份，并分配现有客户端密钥。</p><RouterLink to="/portal">
        打开普通用户入口 →
      </RouterLink>
    </header>
    <p v-if="message" role="status">
      {{ message }}
    </p>
    <section>
      <h2>创建用户</h2><form @submit.prevent="create">
        <label for="portal-users-field-1">用户名<input id="portal-users-field-1" v-model="username" required maxlength="100" autocomplete="off"></label><label for="portal-users-field-2">初始密码<input id="portal-users-field-2" v-model="password" required type="password" minlength="12" maxlength="1024" autocomplete="new-password"></label><button :disabled="busy">
          创建用户
        </button>
      </form>
    </section>
    <section>
      <h2>用户列表</h2><p v-if="!users.length">
        还没有普通用户。
      </p><div v-for="user in users" :key="user.id" class="row">
        <span>{{ user.username }} · {{ user.enabled ? '已启用' : '已停用' }}</span><div>
          <button :disabled="busy" @click="openWallet(user)">
            余额与限制
          </button>
          <button :disabled="busy" @click="toggle(user)">
            {{ user.enabled ? '停用' : '启用' }}
          </button><button :disabled="busy" @click="resetId = user.id; resetPassword = ''">
            重置密码
          </button>
        </div>
      </div><form v-if="resetId" @submit.prevent="reset">
        <label for="portal-users-field-3">新密码<input id="portal-users-field-3" v-model="resetPassword" required type="password" minlength="12" maxlength="1024" autocomplete="new-password"></label><button :disabled="busy">
          保存新密码
        </button><button type="button" @click="resetId = ''">
          取消
        </button>
      </form>
    </section>
    <section>
      <h2>分配密钥</h2><p>先在密钥管理中创建独立密钥，再选择要分配的用户和密钥。</p><form @submit.prevent="assign">
        <label for="portal-users-field-4">用户<select id="portal-users-field-4" v-model="userId" required><option value="" disabled>请选择用户</option><option v-for="user in users" :key="user.id" :value="user.id">{{ user.username }}</option></select></label><label for="portal-users-field-5">密钥<select id="portal-users-field-5" v-model="keyId" required><option value="" disabled>请选择密钥</option><option v-for="key in availableKeys" :key="key.id" :value="key.id">{{ key.name }}</option></select></label><button :disabled="busy">
          分配
        </button>
      </form>
    </section>
    <section v-if="selectedUser && walletData">
      <h2>{{ selectedUser.username }} · 余额与限制</h2>
      <p>余额 ${{ walletData.wallet.balanceUsd }} · 累计消费 ${{ walletData.wallet.totalSpentUsd }} · 当前并发 {{ walletData.wallet.activeRequests }}</p>
      <p>今日 ${{ walletData.wallet.dailyUsedUsd }} · 本周 ${{ walletData.wallet.weeklyUsedUsd }}。全部所属密钥共享，日／周按北京时间零点及周一重置。</p>
      <form @submit.prevent="savePolicy">
        <label for="wallet-enforced"><input id="wallet-enforced" v-model="policy.balanceEnforced" type="checkbox">余额不足时停止新请求</label>
        <label for="wallet-daily">每日上限 USD<input id="wallet-daily" v-model="policy.dailyLimitUsd" required type="number" min="0" step="0.00000001"></label>
        <label for="wallet-weekly">每周上限 USD<input id="wallet-weekly" v-model="policy.weeklyLimitUsd" required type="number" min="0" step="0.00000001"></label>
        <label for="wallet-concurrency">共享并发上限<input id="wallet-concurrency" v-model.number="policy.maxConcurrency" required type="number" min="0" max="10000" step="1"></label>
        <button :disabled="busy">
          保存限制
        </button>
      </form>
      <p>上限填 0 表示不限。进行中的请求仍会结算，可能使余额短暂为负；费用采用代理的 USD 计价。</p>
      <form @submit.prevent="recharge">
        <label for="wallet-credit">充值金额 USD<input id="wallet-credit" v-model="credit" required type="number" min="0.00000001" step="0.00000001" @input="creditOperation = ''"></label>
        <label for="wallet-note">备注<input id="wallet-note" v-model="creditNote" maxlength="500" @input="creditOperation = ''"></label>
        <button :disabled="busy">
          确认充值
        </button>
      </form>
      <h3>最近 100 笔流水</h3>
      <div v-for="event in walletData.events" :key="event.id" class="row">
        <span>{{ new Date(event.createdAt).toLocaleString() }} · {{ event.kind === 'credit' ? '充值' : '调用扣费' }} · {{ event.note }}</span><strong>${{ event.amountUsd }}</strong>
      </div>
      <p v-if="!walletData.events.length">
        暂无流水
      </p>
    </section>
  </main>
</template>

<style scoped>
.users {
  padding: 28px;
  color: var(--cp-color-text);
}
h1 {
  font-size: 26px;
  font-weight: 650;
}
h2 {
  font-size: 19px;
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
form,
.row {
  display: flex;
  gap: 16px;
  align-items: end;
  flex-wrap: wrap;
  margin-top: 14px;
}
.row {
  justify-content: space-between;
  align-items: center;
  padding: 12px 0;
  border-bottom: 1px solid var(--cp-color-border);
}
label {
  display: flex;
  flex-direction: column;
  gap: 7px;
}
input,
select,
button {
  padding: 9px 12px;
  background: var(--cp-color-bg-container);
  border: 1px solid var(--cp-color-border);
  border-radius: 8px;
  color: inherit;
}
button {
  cursor: pointer;
  margin-right: 8px;
}
button:disabled {
  opacity: 0.5;
}
</style>
