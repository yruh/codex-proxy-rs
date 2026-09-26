<script setup lang="ts">
import { BaseButton, BaseCard, BaseIconButton, BaseInput, BaseSwitch } from '@codex-proxy/ui'
import { Plus, Trash2 } from '@lucide/vue'

defineProps<{ disabled: boolean }>()
const disableLongContextPricing = defineModel<boolean>('disableLongContextPricing', { required: true })
const enabled = defineModel<boolean>('enabled', { required: true })
const mappings = defineModel<Array<{ requestedModel: string, upstreamModel: string }>>('mappings', { required: true })
</script>

<template>
  <BaseCard title="长上下文计价" description="控制 OpenAI 文本模型超过 272K 输入后的长上下文加价">
    <div class="grid max-w-6xl gap-3">
      <BaseSwitch v-model="disableLongContextPricing" label="免除长上下文加价" show-label :disabled="disabled" />
      <p class="m-0 text-cp text-cp-text-secondary">
        {{ disableLongContextPricing ? '已免除：长上下文沿用普通上下文单价，再应用计费倍率。' : '按内置长上下文价格计费：适用模型的输入与缓存通常为 2 倍，输出为 1.5 倍。' }}
        保存后对新请求生效，历史账单不变；不会改变上游账号的实际额度消耗。
      </p>
    </div>
  </BaseCard>
  <BaseCard title="子代理模型路由" description="仅对携带子代理标记的请求替换模型，主代理保持原模型">
    <div class="grid max-w-6xl gap-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <BaseSwitch v-model="enabled" label="启用子代理路由" show-label :disabled="disabled" />
        <BaseButton variant="secondary" :disabled="disabled" @click="mappings = [...mappings, { requestedModel: '', upstreamModel: '' }]">
          <template #icon>
            <Plus class="size-4" />
          </template>
          添加规则
        </BaseButton>
      </div>
      <p class="m-0 text-cp text-cp-text-secondary">
        精确匹配来源模型，直接路由到目标模型；未匹配时沿用全局模型映射。按实际目标模型计价，账号权限与并发限制仍然生效。
      </p>
      <div v-if="!mappings.length" class="rounded-cp bg-cp-fill-quaternary p-4 text-cp-text-secondary">
        尚未添加规则，子代理继续使用原模型。
      </div>
      <div v-for="(row, index) in mappings" :key="index" class="grid gap-3 rounded-cp-card bg-cp-fill-quaternary p-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)_auto] sm:items-center">
        <BaseInput v-model="row.requestedModel" placeholder="来源模型，例如 gpt-6-astra" aria-label="子代理来源模型" :disabled="disabled" />
        <span class="hidden text-cp-text-quaternary sm:block" aria-hidden="true">→</span>
        <BaseInput v-model="row.upstreamModel" placeholder="目标模型，例如 gpt-5.6-luna" aria-label="子代理目标模型" :disabled="disabled" />
        <BaseIconButton label="删除子代理规则" variant="ghost" :disabled="disabled" @click="mappings = mappings.filter((_, i) => i !== index)">
          <Trash2 class="size-4 text-cp-error" />
        </BaseIconButton>
      </div>
    </div>
  </BaseCard>
</template>
