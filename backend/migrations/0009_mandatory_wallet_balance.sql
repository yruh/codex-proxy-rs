-- 普通用户必须先充值才能调用，余额检查不再提供关闭开关。
alter table portal_wallets drop column balance_enforced;
