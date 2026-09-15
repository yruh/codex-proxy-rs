-- 删除登录身份但保留钱包、账单与调用归属，用户名可以重新注册。
alter table portal_users add column deleted_at timestamptz;
alter table portal_users drop constraint portal_users_username_key;
create unique index portal_users_active_username on portal_users(username) where deleted_at is null;
-- 密钥删除后仍通过不可变的 key ref 追溯原用户；新绑定由事务检查密钥存在。
alter table portal_key_owners drop constraint portal_key_owners_client_api_key_id_fkey;
