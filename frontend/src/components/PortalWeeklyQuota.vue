<script setup lang="ts">
import type { PortalWallet } from '@/api/modules/portal'
import { computed } from 'vue'

const props = defineProps<{ wallet: PortalWallet }>()
const limit = computed(() => Number(props.wallet.weeklyLimitUsd))
const used = computed(() => Number(props.wallet.weeklyUsedUsd))
const limited = computed(() => limit.value > 0)
const remaining = computed(() => Math.max(0, limit.value - used.value))
const exhausted = computed(() => limited.value && remaining.value === 0)
const progress = computed(() => limited.value ? Math.min(100, Math.max(0, used.value / limit.value * 100)) : 0)
const money = (value: number) => value.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 8 })
const resetsAt = computed(() => {
  const date = new Date(props.wallet.weeklyResetsAt)
  return Number.isNaN(date.getTime()) ? '每周一 00:00' : date.toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false })
})
</script>

<template>
  <section aria-label="当前周限额" class="space-y-4">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="m-0 text-sm font-semibold text-cp-text">
        当前周限额
      </h3>
      <span class="rounded-cp-sm px-2 py-1 text-xs font-semibold" :class="exhausted ? 'bg-cp-error-container text-cp-error-on-container' : 'bg-cp-fill-tertiary text-cp-text-secondary'">{{ limited ? exhausted ? '本周额度已耗尽' : '限额生效中' : '未设置周上限' }}</span>
    </div>
    <div class="grid grid-cols-3 gap-3 tabular-nums">
      <div>
        <p class="text-xs text-cp-text-tertiary">
          本周上限 · USD
        </p><p class="mt-2 break-all text-lg font-bold text-cp-text">
          {{ limited ? money(limit) : '不限' }}
        </p>
      </div>
      <div>
        <p class="text-xs text-cp-text-tertiary">
          本周已用 · USD
        </p><p class="mt-2 break-all text-lg font-bold text-cp-text">
          {{ money(used) }}
        </p>
      </div>
      <div>
        <p class="text-xs text-cp-text-tertiary">
          本周剩余 · USD
        </p><p class="mt-2 break-all text-lg font-bold" :class="exhausted ? 'text-cp-error-text' : 'text-cp-primary-text'">
          {{ limited ? money(remaining) : '不限' }}
        </p>
      </div>
    </div>
    <div v-if="limited" class="h-2 overflow-hidden rounded-full bg-cp-fill-tertiary" role="progressbar" aria-label="本周额度已用比例" :aria-valuenow="progress" :aria-valuemin="0" :aria-valuemax="100">
      <div class="h-full rounded-full" :class="exhausted ? 'bg-cp-error' : 'bg-cp-primary'" :style="{ width: `${progress}%` }" />
    </div>
    <div class="flex flex-wrap justify-between gap-2 text-xs text-cp-text-tertiary">
      <span>{{ limited ? `已用 ${progress.toFixed(1)}%` : '仍受账户余额、日上限和并发限制' }}</span><span>重置 {{ resetsAt }} · 北京时间</span>
    </div>
    <p v-if="limited" class="m-0 text-xs leading-relaxed text-cp-text-tertiary">
      周剩余额度不等于钱包余额；新请求仍需同时满足余额、每日上限和并发限制。
    </p>
  </section>
</template>
