-- 用户下全部密钥共享钱包；默认关闭余额限制，避免升级时中断既有用户。
create table portal_wallets (
    user_id text primary key references portal_users(id),
    balance_usd numeric(24,8) not null default 0,
    total_spent_usd numeric(24,8) not null default 0,
    balance_enforced boolean not null default false,
    daily_limit_usd numeric(24,8) not null default 0 check (daily_limit_usd >= 0),
    weekly_limit_usd numeric(24,8) not null default 0 check (weekly_limit_usd >= 0),
    max_concurrency integer not null default 0 check (max_concurrency >= 0),
    updated_at timestamptz not null default now()
);
create table portal_wallet_events (
    id text primary key,
    user_id text not null references portal_users(id),
    kind text not null check (kind in ('credit','usage')),
    amount_usd numeric(24,8) not null,
    note text not null default '',
    created_at timestamptz not null default now()
);
create index portal_wallet_events_user_time on portal_wallet_events(user_id,created_at);
-- 准入时固定费用归属，断线或重启不丢失用户并发槽位。
create table portal_user_requests (
    request_id text primary key,
    user_id text not null references portal_users(id),
    key_id text not null,
    expires_at timestamptz not null,
    released boolean not null default false
);
create index portal_user_requests_active on portal_user_requests(user_id,expires_at) where not released;
