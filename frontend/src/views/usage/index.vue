<script setup lang="ts">
import type { PortalUser } from '@/api/modules/portal'
import { Eye } from '@lucide/vue'
import { computed, onMounted, shallowRef, watch } from 'vue'
import { portalRequest } from '@/api/modules/portal'

import BaseCard from '@/components/base/BaseCard.vue'
import BaseIconButton from '@/components/base/BaseIconButton.vue'
import BasePageHeader from '@/components/base/BasePageHeader.vue'
import BaseSegmented from '@/components/base/BaseSegmented.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseTablePagination from '@/components/base/BaseTable/BaseTablePagination.vue'
import ProviderFilterSegmented from '@/components/ProviderFilterSegmented.vue'
import PricingSummary from '../portal-users/PricingSummary.vue'
import OpsErrorPanel from './components/OpsErrorPanel.vue'
import UsageFilters from './components/UsageFilters.vue'
import UsageInsightsGrid from './components/UsageInsightsGrid.vue'
import UsageRecordDetailModal from './components/UsageRecordDetailModal.vue'
import UsageRecordsTable from './components/UsageRecordsTable.vue'
import UsageSummaryCards from './components/UsageSummaryCards.vue'
import { useUsageRecordDetail } from './composables/useUsageRecordDetail'
import { useUsageRecordsTable } from './composables/useUsageRecordsTable'
import { useUsageTimeRange } from './composables/useUsageTimeRange'
import { usageRecordColumns, usageTimeRangeOptions } from './constants'

const recordView = shallowRef('success')
const recordViewOptions = [
  { label: '成功记录', value: 'success' },
  { label: '错误排查', value: 'errors' },
]
const { timeRange, timeRangeParams, refreshTimeRangeEnd, latestTimeRangeParams }
  = useUsageTimeRange()

const {
  currentPage,
  searchQuery,
  providerQuery,
  userQuery,
  usagePagination,
  loading,
  analyticsLoading,
  records,
  summary,
  insights,
  refreshingList,
  diagnosticDimension,
  loadUsageRecords,
  refreshUsageRecords,
  handlePageChange,
  handlePageSizeChange,
} = useUsageRecordsTable({
  timeRangeParams,
  latestTimeRangeParams,
  active: computed(() => recordView.value === 'success'),
})

const { showDetailModal, selectedUsageRecord, handleViewDetail } = useUsageRecordDetail()
const users = shallowRef<PortalUser[]>([])
const usersError = shallowRef('')
const userOptions = computed(() => [
  { label: '全部用户', value: '' },
  ...users.value.map(user => ({ label: user.username, value: user.id })),
])
const scopeLabel = computed(() => userQuery.value
  ? `用户：${users.value.find(user => user.id === userQuery.value)?.username || userQuery.value} · 包含该用户全部密钥`
  : '全部用户及未分配密钥的请求')
onMounted(async () => {
  try {
    users.value = (await portalRequest<{ items: PortalUser[] }>('/api/admin/portal/users')).items
  }
  catch {
    usersError.value = '用户列表加载失败，请刷新页面重试'
  }
})

watch(timeRange, () => {
  refreshTimeRangeEnd()
  currentPage.value = 1
  void loadUsageRecords()
})
</script>

<template>
  <div class="w-full">
    <BasePageHeader title="使用统计" description="查看请求用量、性能趋势与调用错误记录">
      <template #actions>
        <BaseSelect v-model="userQuery" :options="userOptions" class="w-48" aria-label="按用户查看用量" />
        <BaseSelect v-model="timeRange" :options="usageTimeRangeOptions" class="w-34" />
        <ProviderFilterSegmented
          v-model="providerQuery"
          :disabled="refreshingList"
          class="w-31 shrink-0"
        />
      </template>
    </BasePageHeader>

    <p class="mb-5 text-cp font-emphasis text-cp-text-secondary">
      {{ usersError || scopeLabel }}
    </p>

    <UsageSummaryCards :summary="summary" />
    <PricingSummary />
    <UsageInsightsGrid
      v-model:diagnostic-dimension="diagnosticDimension"
      :overview="insights.overview"
      :diagnostics="insights.diagnostics"
      :loading="analyticsLoading"
    />

    <BaseCard
      class="mt-5 flex flex-col"
    >
      <template #header>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 class="m-0 text-xl leading-[1.15] font-heavy text-cp-text">
              请求明细
            </h2>
            <p
              class="mt-1.75 mb-0 text-cp leading-[1.15] font-emphasis text-cp-text-secondary"
            >
              成功请求与失败请求明细
            </p>
          </div>
          <BaseSegmented v-model="recordView" label="请求明细类型" :options="recordViewOptions" class="w-52" />
        </div>
      </template>

      <template #body>
        <div
          v-show="recordView === 'success'"
          class="grid min-h-130 min-w-0 flex-1 grid-rows-[auto_minmax(0,1fr)] gap-3"
        >
          <UsageFilters
            v-model:search="searchQuery"
            :loading="loading"
            :refreshing="refreshingList"
            @refresh="refreshUsageRecords"
          />

          <div class="flex min-h-0 min-w-0 flex-col">
            <UsageRecordsTable
              class="min-h-0 flex-1"
              :columns="usageRecordColumns"
              :rows="records"
              :loading="loading"
              empty-text="暂无使用记录"
            >
              <template #actions="{ row }">
                <div class="flex items-center justify-start">
                  <BaseIconButton
                    variant="ghost"
                    size="sm"
                    label="查看使用记录详情"
                    @click="handleViewDetail(row)"
                  >
                    <Eye class="size-3.5" />
                  </BaseIconButton>
                </div>
              </template>
            </UsageRecordsTable>
            <BaseTablePagination
              :pagination="usagePagination"
              :loading="loading"
              @page-change="handlePageChange"
              @page-size-change="handlePageSizeChange"
            />
          </div>
        </div>

        <div v-show="recordView === 'errors'" class="min-h-130 min-w-0 flex-1">
          <OpsErrorPanel
            :time-range-params="timeRangeParams"
            :latest-time-range-params="latestTimeRangeParams"
            :provider="providerQuery"
            :user-id="userQuery"
            :active="recordView === 'errors'"
          />
        </div>
      </template>
    </BaseCard>

    <UsageRecordDetailModal v-model="showDetailModal" :record="selectedUsageRecord" />
  </div>
</template>
