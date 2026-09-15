-- 规则只追加版本；请求固定准入时的版本，管理员改价不影响正在执行及历史请求。
create table portal_pricing_revisions (
    id bigserial primary key,
    policy jsonb not null,
    created_at timestamptz not null default now()
);
insert into portal_pricing_revisions(policy) values ('{"globalMultiplier":"1","modelMultipliers":{}}');
alter table portal_user_requests add column pricing_revision bigint not null default 1 references portal_pricing_revisions(id);
alter table portal_wallet_events add column base_cost_usd numeric(24,8);
alter table portal_wallet_events add column multiplier numeric(12,6);
alter table portal_wallet_events add column model_id text;
