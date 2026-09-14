<script setup lang="ts">
import type { PortalKey, PortalUsage, PortalUser, WalletResponse } from '@/api/modules/portal'
import type { BaseTableColumn } from '@/components/base/BaseTable/columns'
import { onMounted, ref } from 'vue'
import { portalRequest } from '@/api/modules/portal'
import AppBrandMark from '@/components/AppBrandMark.vue'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseCard from '@/components/base/BaseCard.vue'
import FormItem from '@/components/base/BaseForm/FormItem.vue'
import BaseInput from '@/components/base/BaseInput.vue'
import BaseModal from '@/components/base/BaseModal/index.vue'
import BasePageHeader from '@/components/base/BasePageHeader.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseTable from '@/components/base/BaseTable/index.vue'

const showPassword = ref(false)
const showLedger = ref(false)
const columns: BaseTableColumn<PortalUsage>[] = [
  { key: 'occurredAt', label: '时间', kind: 'datetime', format: v => new Date(String(v)).toLocaleString() },
  { key: 'model', label: '模型', kind: 'identity' },
  { key: 'inputTokens', label: '输入', kind: 'numeric' },
  { key: 'cachedTokens', label: '缓存', kind: 'numeric' },
  { key: 'outputTokens', label: '输出', kind: 'numeric' },
  { key: 'estimatedUsd', label: '费用 USD', kind: 'numeric' },
]
const ledgerColumns: BaseTableColumn<WalletResponse['events'][number]>[] = [
  { key: 'createdAt', label: '时间', kind: 'datetime', format: v => new Date(String(v)).toLocaleString() },
  { key: 'kind', label: '类型', size: 'sm', format: v => v === 'credit' ? '充值' : '调用扣费' },
  { key: 'amountUsd', label: '金额 USD', kind: 'numeric' },
  { key: 'note', label: '备注' },
]

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
    walletData.value = null
    user.value = null
    keys.value = []
    rows.value = []
    revealed.value = []
    notice.value = '密码已修改，所有旧登录已失效，请使用新密码登录。'
    showPassword.value = false
  })
}
const money = (n: string) => Number(n).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 6 })

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
    walletData.value = null
    notice.value = ''
    user.value = await portalRequest<PortalUser>('/api/portal/login', { username: username.value, password: password.value })
    password.value = ''
    await load()
  })
}
async function logout() {
  await action(async () => {
    await portalRequest('/api/portal/logout', {})
    user.value = null
    walletData.value = null
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
  <main class="min-h-dvh bg-cp-bg-layout px-4 py-8 text-cp-text sm:px-8">
    <div class="mx-auto max-w-6xl space-y-5">
      <div class="mb-8 flex items-center justify-between gap-4">
        <div class="flex items-center gap-3">
          <AppBrandMark class="size-10" /><span class="text-lg font-bold">Codex Proxy</span><span class="text-sm text-cp-text-tertiary">用户中心</span>
        </div><div v-if="user" class="flex gap-2">
          <BaseButton @click="showPassword = true; error = ''">
            修改密码
          </BaseButton><BaseButton :disabled="busy" @click="logout">
            退出登录
          </BaseButton>
        </div><RouterLink v-else to="/login" class="text-sm text-cp-link">
          管理员入口 ↗
        </RouterLink>
      </div>
      <p v-if="error && !showPassword" role="alert" class="rounded-cp bg-cp-error-container p-3 text-sm text-cp-error-on-container">
        {{ error }}
      </p>
      <p v-if="notice" role="status" class="rounded-cp bg-cp-success-container p-3 text-sm text-cp-success-on-container">
        {{ notice }}
      </p>
      <BaseCard v-if="!user" class="mx-auto mt-16 max-w-md" title="欢迎回来" description="登录查看你的余额、密钥和调用记录">
        <form class="space-y-5" @submit.prevent="login">
          <FormItem label="用户名" required>
            <BaseInput v-model="username" autocomplete="username" maxlength="100" />
          </FormItem>
          <FormItem label="密码" required>
            <BaseInput v-model="password" type="password" autocomplete="current-password" maxlength="1024" />
          </FormItem>
          <BaseButton class="w-full" type="submit" variant="primary" :loading="busy">
            登录
          </BaseButton>
        </form>
      </BaseCard>
      <template v-else>
        <BasePageHeader :title="`${user.username}，你好`" description="你的余额与使用情况">
          <template #actions>
            <BaseButton :loading="busy" @click="action(load)">
              刷新用量
            </BaseButton>
          </template>
        </BasePageHeader>
        <div v-if="walletData" class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <BaseCard>
            <p class="text-sm text-cp-text-secondary">
              可用余额 · USD
            </p><p class="mt-3 break-all text-3xl font-bold tabular-nums">
              {{ money(walletData.wallet.balanceUsd) }}
            </p><p class="mt-3 text-xs" :class="Number(walletData.wallet.balanceUsd) > 0 ? 'text-cp-success-text' : 'text-cp-error-text'">
              {{ Number(walletData.wallet.balanceUsd) > 0 ? '余额可用' : '余额不足，请联系管理员充值' }}
            </p><BaseButton size="sm" variant="ghost" class="mt-3" @click="showLedger = true">
              查看充值与扣费 →
            </BaseButton>
          </BaseCard>
          <BaseCard>
            <p class="text-sm text-cp-text-secondary">
              今日消费 · USD
            </p><p class="mt-3 break-all text-3xl font-bold tabular-nums">
              {{ money(walletData.wallet.dailyUsedUsd) }}
            </p><p class="mt-3 text-xs text-cp-text-tertiary">
              每日上限 {{ Number(walletData.wallet.dailyLimitUsd) ? money(walletData.wallet.dailyLimitUsd) : '不限' }}
            </p>
          </BaseCard>
          <BaseCard>
            <p class="text-sm text-cp-text-secondary">
              本周消费 · USD
            </p><p class="mt-3 break-all text-3xl font-bold tabular-nums">
              {{ money(walletData.wallet.weeklyUsedUsd) }}
            </p><p class="mt-3 text-xs text-cp-text-tertiary">
              每周上限 {{ Number(walletData.wallet.weeklyLimitUsd) ? money(walletData.wallet.weeklyLimitUsd) : '不限' }}
            </p>
          </BaseCard>
          <BaseCard>
            <p class="text-sm text-cp-text-secondary">
              共享并发
            </p><p class="mt-3 text-3xl font-bold tabular-nums">
              {{ walletData.wallet.activeRequests }} <span class="text-base font-normal text-cp-text-tertiary">/ {{ walletData.wallet.maxConcurrency || '不限' }}</span>
            </p><p class="mt-3 text-xs text-cp-text-tertiary">
              全部密钥共用 · 累计消费 {{ money(walletData.wallet.totalSpentUsd) }} USD
            </p>
          </BaseCard>
        </div>
        <p class="text-xs leading-relaxed text-cp-text-tertiary">
          余额耗尽自动停止新请求。日／周按北京时间重置，已开始的请求按实际费用结算。
        </p>
        <BaseCard title="我的密钥" description="仅显示管理员分配给你的密钥">
          <p v-if="!keys.length" class="py-8 text-center text-sm text-cp-text-tertiary">
            尚未分配密钥，请联系管理员。
          </p>
          <div v-for="key in keys" :key="key.id" class="mb-2 flex flex-wrap items-center justify-between gap-3 rounded-cp bg-cp-fill-quaternary p-4">
            <div class="min-w-0">
              <div class="flex items-center gap-3">
                <strong>{{ key.name }}</strong><span class="text-xs" :class="key.enabled ? 'text-cp-success-text' : 'text-cp-text-tertiary'">{{ key.enabled ? '可用' : '已停用' }}</span>
              </div><code class="mt-2 block break-all text-sm text-cp-text-secondary">{{ revealed.includes(key.id) ? key.key : '••••••••••••••••••••••••' }}</code>
            </div><BaseButton size="sm" @click="toggle(key.id)">
              {{ revealed.includes(key.id) ? '隐藏' : '显示密钥' }}
            </BaseButton>
          </div>
        </BaseCard>
        <BaseCard title="我的调用记录" description="所属密钥的代理调用，缓存已包含在输入中">
          <template #actions>
            <BaseSelect :model-value="String(days)" :disabled="busy" :options="[{ label: '最近一天', value: '1' }, { label: '最近一周', value: '7' }, { label: '最近一月', value: '30' }]" aria-label="时间范围" @update:model-value="days = Number($event); page = 1; action(load)" />
          </template>
          <BaseTable :columns="columns" :rows="rows" :loading="busy" empty-text="当前范围没有调用记录" />
          <div class="mt-4 flex items-center justify-between gap-2">
            <span class="text-xs text-cp-text-tertiary">第 {{ page }} 页 · 每页最多 100 条</span><div class="flex gap-2">
              <BaseButton size="sm" :disabled="busy || page <= 1" @click="page--; action(load)">
                上一页
              </BaseButton><BaseButton size="sm" :disabled="busy || rows.length < 100" @click="page++; action(load)">
                下一页
              </BaseButton>
            </div>
          </div>
        </BaseCard>
      </template>
      <BaseModal v-model="showPassword" title="修改密码" description="修改后全部旧登录失效，请使用新密码重新登录" :dismissible="!busy">
        <form id="change-password" class="space-y-5" @submit.prevent="changePassword">
          <FormItem label="当前密码" required>
            <BaseInput v-model="oldPassword" type="password" autocomplete="current-password" maxlength="1024" />
          </FormItem>
          <FormItem label="新密码" description="至少 12 个字符" required>
            <BaseInput v-model="newPassword" type="password" autocomplete="new-password" minlength="12" maxlength="1024" />
          </FormItem>
          <FormItem label="确认新密码" required>
            <BaseInput v-model="confirmPassword" type="password" autocomplete="new-password" minlength="12" maxlength="1024" />
          </FormItem>
          <p v-if="error" role="alert" class="text-sm text-cp-error-text">
            {{ error }}
          </p>
        </form>
        <template #footer>
          <BaseButton :disabled="busy" @click="showPassword = false">
            取消
          </BaseButton><BaseButton type="submit" form="change-password" variant="primary" :loading="busy">
            修改密码
          </BaseButton>
        </template>
      </BaseModal>
      <BaseModal v-model="showLedger" title="充值与扣费流水" description="最近 100 笔记录" size="lg">
        <BaseTable :rows="walletData?.events || []" :columns="ledgerColumns" empty-text="暂无流水" />
      </BaseModal>
    </div>
  </main>
</template>
