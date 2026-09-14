-- 普通用户与管理员分开存储，普通用户身份不能成为管理会话。
create table portal_users (
  id text primary key,
  username text not null unique,
  password_hash text not null,
  enabled boolean not null default true,
  session_version bigint not null default 1 check (session_version > 0),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check (length(username) between 1 and 100 and username = lower(btrim(username)))
);

-- 归属只由管理员设置；不改动原密钥表及扣费事实。
create table portal_key_owners (
  client_api_key_id text primary key references client_api_keys(id) on delete cascade,
  user_id text not null references portal_users(id) on delete cascade
);
create index portal_key_owners_user_idx on portal_key_owners(user_id);

-- 会话只保存随机令牌的摘要，密码重置通过版本号统一使旧会话失效。
create table portal_sessions (
  token_hash text primary key,
  user_id text not null references portal_users(id) on delete cascade,
  session_version bigint not null,
  expires_at timestamptz not null
);
create index portal_sessions_expiry_idx on portal_sessions(expires_at);

-- 同步设备属于管理员，绑定上游身份，不允许写入代理请求或密钥扣费表。
create table usage_sync_devices (
  id text primary key,
  name text not null,
  provider_account_id text not null references provider_accounts(id) on delete restrict,
  token_hash text not null unique,
  enabled boolean not null default true,
  last_sync_at timestamptz,
  created_at timestamptz not null default now()
);

create table local_usage_records (
  device_id text not null references usage_sync_devices(id) on delete restrict,
  record_id text not null,
  revision bigint not null check (revision > 0),
  occurred_at timestamptz not null,
  session_id text,
  parent_session_id text,
  model text,
  reasoning_effort text,
  service_tier text,
  transport text,
  input_tokens bigint not null check (input_tokens >= 0),
  output_tokens bigint not null check (output_tokens >= 0),
  cached_tokens bigint check (cached_tokens >= 0 and cached_tokens <= input_tokens),
  reasoning_tokens bigint check (reasoning_tokens >= 0 and reasoning_tokens <= output_tokens),
  duration_ms bigint check (duration_ms >= 0),
  first_token_ms bigint check (first_token_ms >= 0),
  estimated_usd numeric(24,12) check (estimated_usd >= 0),
  excluded boolean not null default false,
  updated_at timestamptz not null default now(),
  primary key (device_id, record_id)
);
create index local_usage_records_time_idx on local_usage_records(occurred_at, device_id, record_id);
