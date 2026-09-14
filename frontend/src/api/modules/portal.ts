export interface PortalUser { id: string, username: string, enabled: boolean }
export interface WalletPolicy { dailyLimitUsd: string, weeklyLimitUsd: string, maxConcurrency: number }
export interface PortalWallet extends WalletPolicy { userId: string, balanceUsd: string, totalSpentUsd: string, dailyUsedUsd: string, weeklyUsedUsd: string, activeRequests: number }
export interface WalletEvent { id: string, kind: string, amountUsd: string, note: string, createdAt: string }
export interface WalletResponse { wallet: PortalWallet, events: WalletEvent[] }
export interface PortalKey { id: string, name: string, key: string, enabled: boolean }
export interface PortalUsage {
  id: string
  keyId: string
  model: string | null
  occurredAt: string
  inputTokens: number | null
  outputTokens: number | null
  cachedTokens: number | null
  estimatedUsd: string | null
}

// 普通用户 Cookie 与管理员会话分离，401 不触发管理员 Store 的跳转。
export async function portalRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method: body === undefined ? 'GET' : 'POST',
    credentials: 'same-origin',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(20000),
  })
  const result = await response.json()
  if (!response.ok)
    throw new Error(result.message || '请求失败，请重新登录或稍后重试')
  return result.data as T
}
