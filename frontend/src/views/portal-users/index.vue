<script setup lang="ts">
import type { PortalUser, WalletPolicy, WalletResponse } from '@/api/modules/portal'
import type { BaseTableColumn } from '@/components/base/BaseTable/columns'
import { KeyRound, Plus, RefreshCw, Users, Wallet } from '@lucide/vue'
import { computed, onMounted, ref } from 'vue'
import { getApiKeys } from '@/api/modules/api-keys'
import { portalRequest } from '@/api/modules/portal'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseCard from '@/components/base/BaseCard.vue'
import FormItem from '@/components/base/BaseForm/FormItem.vue'
import BaseInput from '@/components/base/BaseInput.vue'
import BaseModal from '@/components/base/BaseModal/index.vue'
import BasePageHeader from '@/components/base/BasePageHeader.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseTable from '@/components/base/BaseTable/index.vue'
import PricingSettings from './PricingSettings.vue'

const availableKeys = ref<{ id: string, name: string }[]>([])
const users = ref<PortalUser[]>([])
const search = ref('')
const visibleUsers = computed(() => users.value.filter(u => u.username.toLowerCase().includes(search.value.toLowerCase())))
const username = ref('')
const password = ref('')
const keyId = ref('')
const userId = ref('')
const busy = ref(false)
const message = ref('')
const failed = ref(false)
const showCreate = ref(false)
const showAssign = ref(false)
const showWallet = ref(false)
const showReset = ref(false)
const selectedUser = ref<PortalUser | null>(null)
const walletData = ref<WalletResponse | null>(null)
const policy = ref<WalletPolicy>({ dailyLimitUsd: '0', weeklyLimitUsd: '0', maxConcurrency: 0 })
const concurrency = ref('0')
const credit = ref('')
const creditNote = ref('')
const creditOperation = ref('')
const resetPassword = ref('')
const money = (value: string) => Number(value).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 8 })
const userColumns: BaseTableColumn<PortalUser>[] = [
  { key: 'username', label: '用户', kind: 'identity' },
  { key: 'enabled', label: '状态', kind: 'status' },
  { key: 'billing', label: '计费规则', kind: 'meta', size: '2xl' },
  { key: 'actions', label: '操作', kind: 'actions', size: '4xl' },
]
const eventColumns: BaseTableColumn<WalletResponse['events'][number]>[] = [
  { key: 'createdAt', label: '时间', kind: 'datetime', format: v => new Date(String(v)).toLocaleString() },
  { key: 'kind', label: '类型', size: 'sm', format: v => v === 'credit' ? '充值' : '调用扣费' },
  { key: 'amountUsd', label: '金额 USD', kind: 'numeric' },
  { key: 'note', label: '备注', kind: 'text' },
]
async function action(work: () => Promise<void>) {
  if (busy.value)
    return
  busy.value = true
  message.value = ''
  failed.value = false
  try {
    await work()
  }
  catch (e) {
    failed.value = true
    message.value = e instanceof Error ? e.message : '操作失败'
  }
  finally { busy.value = false }
}
async function load() {
  const [userData, keyData] = await Promise.all([
    portalRequest<{ items: PortalUser[] }>('/api/admin/portal/users'),
    getApiKeys({ limit: 1000 }),
  ])
  users.value = userData.items
  availableKeys.value = keyData.items
}
async function reloadWallet() {
  if (!selectedUser.value)
    return
  walletData.value = await portalRequest<WalletResponse>(`/api/admin/portal/wallet?userId=${encodeURIComponent(selectedUser.value.id)}`)
  const w = walletData.value.wallet
  policy.value = { dailyLimitUsd: w.dailyLimitUsd, weeklyLimitUsd: w.weeklyLimitUsd, maxConcurrency: w.maxConcurrency }
  concurrency.value = String(w.maxConcurrency)
}
async function openWallet(user: PortalUser) {
  selectedUser.value = user
  walletData.value = null
  credit.value = creditNote.value = creditOperation.value = ''
  showWallet.value = true
  await action(reloadWallet)
}
async function savePolicy() {
  await action(async () => {
    await portalRequest('/api/admin/portal/wallet/policy', { userId: selectedUser.value!.id, policy: { ...policy.value, maxConcurrency: Number(concurrency.value) } })
    await reloadWallet()
    message.value = '共享限制已保存'
  })
}
async function recharge() {
  await action(async () => {
    creditOperation.value ||= crypto.randomUUID()
    await portalRequest('/api/admin/portal/wallet/credit', { userId: selectedUser.value!.id, operationId: creditOperation.value, amount: credit.value, note: creditNote.value })
    credit.value = creditNote.value = creditOperation.value = ''
    await reloadWallet()
    message.value = '充值成功'
  })
}
async function create() {
  await action(async () => {
    await portalRequest('/api/admin/portal/users', { username: username.value, password: password.value })
    username.value = password.value = ''
    showCreate.value = false
    await load()
    message.value = '用户已创建，充值并分配密钥后即可使用'
  })
}
async function assign() {
  await action(async () => {
    if (!userId.value || !keyId.value)
      throw new Error('请选择用户和密钥')
    await portalRequest('/api/admin/portal/keys/assign', { keyId: keyId.value, userId: userId.value })
    keyId.value = ''
    showAssign.value = false
    await load()
    message.value = '密钥已分配'
  })
}
async function toggle(user: PortalUser) {
  await action(async () => {
    await portalRequest('/api/admin/portal/users/update', { id: user.id, enabled: !user.enabled })
    await load()
  })
}
async function reset() {
  await action(async () => {
    const user = selectedUser.value!
    await portalRequest('/api/admin/portal/users/update', { id: user.id, enabled: user.enabled, password: resetPassword.value })
    resetPassword.value = ''
    showReset.value = false
    message.value = '密码已重置，旧登录已失效'
  })
}
onMounted(() => action(load))
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-5">
    <BasePageHeader title="用户管理" description="管理独立用户、共享余额和访问限制">
      <template #actions>
        <PricingSettings />
        <RouterLink to="/portal" class="text-cp-link text-sm font-semibold">
          用户入口 ↗
        </RouterLink>
        <BaseButton :loading="busy" @click="action(load)">
          <template #icon>
            <RefreshCw :size="16" />
          </template>刷新
        </BaseButton>
      </template>
    </BasePageHeader>
    <p v-if="message && !showWallet && !showCreate && !showAssign && !showReset" role="status" class="rounded-cp p-3 text-sm" :class="failed ? 'bg-cp-error-container text-cp-error-on-container' : 'bg-cp-success-container text-cp-success-on-container'">
      {{ message }}
    </p>
    <div class="grid gap-4 sm:grid-cols-3">
      <BaseCard padding="compact">
        <div class="flex items-center gap-3">
          <Users class="text-cp-primary" :size="20" /><span class="text-sm text-cp-text-secondary">用户总数</span><strong class="ml-auto text-2xl tabular-nums">{{ users.length }}</strong>
        </div>
      </BaseCard>
      <BaseCard padding="compact">
        <div class="flex items-center gap-3">
          <span class="size-2 rounded-full bg-cp-success" /><span class="text-sm text-cp-text-secondary">已启用</span><strong class="ml-auto text-2xl tabular-nums">{{ users.filter(u => u.enabled).length }}</strong>
        </div>
      </BaseCard>
      <BaseCard padding="compact">
        <div class="flex items-center gap-3">
          <Wallet class="text-cp-primary" :size="20" /><div>
            <p class="text-sm font-semibold">
              预付余额
            </p><p class="mt-1 text-xs text-cp-text-tertiary">
              余额耗尽自动停止新请求
            </p>
          </div>
        </div>
      </BaseCard>
    </div>
    <BaseCard class="min-h-90" title="用户列表" description="同一用户的所有密钥共享余额与并发限制">
      <template #actions>
        <div class="flex flex-wrap gap-2">
          <BaseButton @click="showAssign = true; message = ''">
            <template #icon>
              <KeyRound :size="16" />
            </template>分配密钥
          </BaseButton>
          <BaseButton variant="primary" @click="showCreate = true; message = ''">
            <template #icon>
              <Plus :size="16" />
            </template>创建用户
          </BaseButton>
        </div>
      </template>
      <BaseInput v-model="search" class="mb-4 max-w-xs" placeholder="搜索用户名" aria-label="搜索用户名" />
      <BaseTable :rows="visibleUsers" :columns="userColumns" :loading="busy && !users.length" empty-text="暂无用户，创建用户后充值并分配密钥">
        <template #enabled="{ row }">
          <span class="rounded-cp-sm px-2 py-1 text-xs font-semibold" :class="row.enabled ? 'bg-cp-success-container text-cp-success-on-container' : 'bg-cp-fill-tertiary text-cp-text-tertiary'">{{ row.enabled ? '已启用' : '已停用' }}</span>
        </template>
        <template #billing>
          余额不足自动停止
        </template>
        <template #actions="{ row }">
          <div class="flex gap-1">
            <BaseButton variant="soft" size="sm" :disabled="busy" @click="openWallet(row)">
              余额与限制
            </BaseButton>
            <BaseButton variant="ghost" size="sm" :disabled="busy" @click="selectedUser = row; resetPassword = ''; showReset = true; message = ''">
              重置密码
            </BaseButton>
            <BaseButton variant="ghost" size="sm" :disabled="busy" @click="toggle(row)">
              {{ row.enabled ? '停用' : '启用' }}
            </BaseButton>
          </div>
        </template>
      </BaseTable>
    </BaseCard>
    <BaseModal v-model="showCreate" title="创建用户" description="创建独立登录身份，初始余额为零" :dismissible="!busy">
      <form id="create-user" class="space-y-5" @submit.prevent="create">
        <FormItem label="用户名" required>
          <BaseInput v-model="username" maxlength="100" autocomplete="off" />
        </FormItem>
        <FormItem label="初始密码" description="至少 12 个字符" required>
          <BaseInput v-model="password" type="password" minlength="12" maxlength="1024" autocomplete="new-password" />
        </FormItem>
        <p v-if="message" role="alert" class="text-sm text-cp-error-text">
          {{ message }}
        </p>
      </form>
      <template #footer>
        <BaseButton :disabled="busy" @click="showCreate = false">
          取消
        </BaseButton><BaseButton type="submit" form="create-user" variant="primary" :loading="busy">
          创建用户
        </BaseButton>
      </template>
    </BaseModal>
    <BaseModal v-model="showAssign" title="分配密钥" description="选择已创建的独立密钥，历史归属不能转移" :dismissible="!busy">
      <form id="assign-key" class="space-y-5" @submit.prevent="assign">
        <FormItem label="用户" required>
          <BaseSelect v-model="userId" :options="users.map(u => ({ label: u.username, value: u.id }))" placeholder="选择用户" />
        </FormItem>
        <FormItem label="密钥" required>
          <BaseSelect v-model="keyId" :options="availableKeys.map(k => ({ label: k.name, value: k.id }))" placeholder="选择密钥" />
        </FormItem>
        <p v-if="message" role="alert" class="text-sm text-cp-error-text">
          {{ message }}
        </p>
      </form>
      <template #footer>
        <BaseButton :disabled="busy" @click="showAssign = false">
          取消
        </BaseButton><BaseButton type="submit" form="assign-key" variant="primary" :loading="busy">
          分配密钥
        </BaseButton>
      </template>
    </BaseModal>
    <BaseModal v-model="showReset" :title="`重置密码 · ${selectedUser?.username || ''}`" description="保存后该用户的全部旧登录立即失效" :dismissible="!busy">
      <form id="reset-password" class="space-y-5" @submit.prevent="reset">
        <FormItem label="新密码" required>
          <BaseInput v-model="resetPassword" type="password" minlength="12" maxlength="1024" autocomplete="new-password" />
        </FormItem>
        <p v-if="message" role="alert" class="text-sm text-cp-error-text">
          {{ message }}
        </p>
      </form>
      <template #footer>
        <BaseButton :disabled="busy" @click="showReset = false">
          取消
        </BaseButton><BaseButton type="submit" form="reset-password" variant="primary" :loading="busy">
          保存密码
        </BaseButton>
      </template>
    </BaseModal>
    <BaseModal v-model="showWallet" :title="`余额与限制 · ${selectedUser?.username || ''}`" description="所有所属密钥共同记账" size="xl" :dismissible="!busy">
      <p v-if="message" role="status" class="mb-4 rounded-cp p-3 text-sm" :class="failed ? 'bg-cp-error-container text-cp-error-on-container' : 'bg-cp-success-container text-cp-success-on-container'">
        {{ message }}
      </p>
      <p v-if="!walletData" class="py-8 text-center text-cp-text-tertiary">
        {{ busy ? '正在加载钱包…' : '钱包加载失败，请关闭后重试' }}
      </p>
      <div v-else class="space-y-6">
        <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div v-for="item in [{ label: '可用余额', value: `$${money(walletData.wallet.balanceUsd)}` }, { label: '累计消费', value: `$${money(walletData.wallet.totalSpentUsd)}` }, { label: '今日消费', value: `$${money(walletData.wallet.dailyUsedUsd)}` }, { label: '本周消费', value: `$${money(walletData.wallet.weeklyUsedUsd)}` }]" :key="item.label" class="rounded-cp bg-cp-fill-quaternary p-4">
            <p class="text-xs text-cp-text-tertiary">
              {{ item.label }}
            </p><p class="mt-2 break-all text-xl font-bold tabular-nums">
              {{ item.value }}
            </p>
          </div>
        </div>
        <div class="rounded-cp bg-cp-info-container p-3 text-sm text-cp-info-on-container">
          余额为零或负数时自动拒绝新请求，充值后恢复使用。
        </div>
        <form class="space-y-4" @submit.prevent="recharge">
          <h3 class="font-bold">
            充值余额
          </h3>
          <div class="grid items-end gap-3 sm:grid-cols-[1fr_1.5fr_auto]">
            <FormItem label="金额 USD" required>
              <BaseInput v-model="credit" type="number" min="0.00000001" step="0.00000001" @update:model-value="creditOperation = ''" />
            </FormItem>
            <FormItem label="备注">
              <BaseInput v-model="creditNote" maxlength="500" placeholder="例如：本月用量" @update:model-value="creditOperation = ''" />
            </FormItem>
            <BaseButton type="submit" variant="primary" :loading="busy">
              确认充值
            </BaseButton>
          </div>
        </form>
        <form class="space-y-4" @submit.prevent="savePolicy">
          <div class="flex items-center justify-between">
            <h3 class="font-bold">
              使用限制
            </h3><span class="text-xs text-cp-text-tertiary">当前并发 {{ walletData.wallet.activeRequests }}</span>
          </div>
          <div class="grid gap-3 sm:grid-cols-3">
            <FormItem label="每日上限 USD" required>
              <BaseInput v-model="policy.dailyLimitUsd" type="number" min="0" step="0.00000001" />
            </FormItem>
            <FormItem label="每周上限 USD" required>
              <BaseInput v-model="policy.weeklyLimitUsd" type="number" min="0" step="0.00000001" />
            </FormItem>
            <FormItem label="共享并发上限" required>
              <BaseInput v-model="concurrency" type="number" min="0" max="10000" step="1" />
            </FormItem>
          </div>
          <div class="flex flex-wrap items-center justify-between gap-3">
            <p class="max-w-lg text-xs leading-relaxed text-cp-text-tertiary">
              上限填 0 表示不限。日／周按北京时间重置。进行中的请求仍会结算，可能使余额变负。
            </p><BaseButton type="submit" :loading="busy">
              保存限制
            </BaseButton>
          </div>
        </form>
        <div>
          <h3 class="mb-3 font-bold">
            最近流水 <span class="text-xs font-normal text-cp-text-tertiary">最多 100 笔</span>
          </h3><BaseTable :columns="eventColumns" :rows="walletData.events" empty-text="暂无充值或扣费记录" />
        </div>
      </div>
    </BaseModal>
  </div>
</template>
