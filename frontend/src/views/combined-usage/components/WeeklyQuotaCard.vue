<script setup lang="ts">
import type { Account } from '@/api/modules/accounts'
import { useNow } from '@vueuse/core'
import { computed, ref, toRef, watch } from 'vue'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseCard from '@/components/base/BaseCard.vue'
import { useAccountQuotaForecast } from '../../accounts/composables/useAccountQuotaForecast'

const props = defineProps<{ account: Account }>()
const emit = defineEmits<{ accountUpdated: [account: Account] }>()
const { report, loading, refreshing, error, load, refresh } = useAccountQuotaForecast(toRef(() => props.account.id), ref(true), account => emit('accountUpdated', account))
const now = useNow({ interval: 30_000 })
const forecast = computed(() => report.value?.forecasts.find(f => f.period === 'weekly' && !f.extrapolated))
const window = computed(() => props.account.quota.windows.find(w => w.windowSeconds === 7 * 86400))
const expired = computed(() => Boolean(forecast.value?.source && new Date(forecast.value.source.resetAt) <= now.value))
const used = computed(() => forecast.value?.source?.usedPercent ?? window.value?.usedPercent ?? null)
const remaining = computed(() => used.value == null ? null : Math.max(0, 100 - used.value))
const unavailable = computed(() => expired.value ? '当前周窗口已结束，请刷新额度。' : forecast.value?.unavailableReason || (!loading.value && !forecast.value ? '暂无当前周额度窗口，请刷新账号额度。' : ''))
const valid = computed(() => !loading.value && !refreshing.value && !expired.value && !unavailable.value && !error.value)
const resetAt = computed(() => forecast.value?.source ? new Date(forecast.value.source.resetAt).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', hour12: false }) : window.value?.resetAtDisplay || '—')
watch(() => props.account.quota.refreshedAtDisplay, () => {
  void load()
})
</script>

<template>
  <BaseCard :title="account.name" description="上游 Codex · 当前周额度">
    <template #actions>
      <BaseButton size="sm" :loading="loading || refreshing" @click="refresh">
        刷新额度
      </BaseButton>
    </template>
    <p v-if="error" role="alert" class="mb-3 text-sm text-cp-error-text">
      额度预测加载失败。<BaseButton size="sm" variant="ghost" @click="load">
        重试
      </BaseButton>
    </p>
    <div class="grid grid-cols-2 gap-4 xl:grid-cols-4">
      <div>
        <p class="text-xs text-cp-text-tertiary">
          本周已用
        </p><p class="mt-2 text-2xl font-bold tabular-nums">
          {{ used == null ? '—' : `${used.toFixed(1)}%` }}
        </p>
      </div>
      <div>
        <p class="text-xs text-cp-text-tertiary">
          本周剩余
        </p><p class="mt-2 text-2xl font-bold tabular-nums text-cp-primary-text">
          {{ remaining == null ? '—' : `${remaining.toFixed(1)}%` }}
        </p>
      </div>
      <div>
        <p class="text-xs text-cp-text-tertiary">
          预计整周容量 · USD
        </p><p class="mt-2 break-all text-2xl font-bold tabular-nums">
          {{ valid ? forecast?.estimatedUsdDisplay || '—' : '—' }}
        </p>
      </div>
      <div>
        <p class="text-xs text-cp-text-tertiary">
          预计剩余可用 · USD
        </p><p class="mt-2 break-all text-2xl font-bold tabular-nums text-cp-primary-text">
          {{ valid ? forecast?.remainingUsdDisplay || '—' : '—' }}
        </p>
      </div>
    </div>
    <div v-if="used != null" class="mt-4 h-2 overflow-hidden rounded-full bg-cp-fill-tertiary" role="progressbar" :aria-label="`${account.name}周额度已用比例`" :aria-valuenow="used" :aria-valuemin="0" :aria-valuemax="100">
      <div class="h-full rounded-full" :class="used >= 100 ? 'bg-cp-error' : 'bg-cp-primary'" :style="{ width: `${Math.max(0, Math.min(100, used))}%` }" />
    </div>
    <div class="mt-3 flex flex-wrap justify-between gap-2 text-xs text-cp-text-tertiary">
      <span>观测 {{ forecast?.source?.observedAtDisplay || account.quota.refreshedAtDisplay }}</span><span>重置 {{ resetAt }} · 北京时间</span>
    </div>
    <p v-if="loading || refreshing" role="status" class="mt-3 text-xs text-cp-text-tertiary">
      正在获取当前周额度…
    </p>
    <p v-else-if="unavailable && !error" class="mt-3 rounded-cp bg-cp-info-container p-3 text-xs text-cp-info-on-container">
      {{ unavailable }}
    </p>
    <p v-else-if="valid && forecast?.lowSample" class="mt-3 text-xs text-cp-warning-text">
      样本较少，预测值可能波动。
    </p>
    <p v-if="valid && forecast?.incompleteCost" class="mt-2 text-xs text-cp-warning-text">
      部分请求缺少计价，美元预测可能偏低。
    </p>
    <p class="mt-3 text-xs leading-relaxed text-cp-text-tertiary">
      按本地直连与服务器代理已记录的用量估算，非官方美元额度；未采集的设备用量可能导致低估。此卡片始终显示当前周，不随下方日期和来源筛选改变。
    </p>
  </BaseCard>
</template>
