-- 保留配额变化，不保存账号凭据。触发器覆盖被动响应、主动刷新和重置卡后的更新。
create table account_quota_history (
    id bigserial primary key,
    account_id text not null,
    upstream_account_id text,
    upstream_user_id text,
    observed_at timestamptz not null,
    document jsonb not null,
    unique (account_id, observed_at)
);
create index account_quota_history_account_time on account_quota_history(account_id, observed_at desc);

create function capture_account_quota_history() returns trigger language plpgsql as $$
begin
    if new.provider_kind = 'openai' and new.provider_quota_json is not null
       and new.quota_observed_at is not null
       and (tg_op = 'INSERT' or old.provider_quota_json is distinct from new.provider_quota_json) then
        insert into account_quota_history(account_id, upstream_account_id, upstream_user_id, observed_at, document)
        values(new.id, new.upstream_account_id, new.upstream_user_id, new.quota_observed_at, new.provider_quota_json)
        on conflict do nothing;
    end if;
    return new;
end;
$$;
create trigger provider_account_quota_history
after insert or update of provider_quota_json on provider_accounts
for each row execute function capture_account_quota_history();

insert into account_quota_history(account_id, upstream_account_id, upstream_user_id, observed_at, document)
select id, upstream_account_id, upstream_user_id, quota_observed_at, provider_quota_json from provider_accounts
where provider_kind = 'openai' and provider_quota_json is not null and quota_observed_at is not null;
