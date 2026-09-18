-- 请求策略默认维持原行为；老客户端不传此字段时不覆盖。
ALTER TABLE runtime_settings ADD COLUMN request_overrides_json jsonb NOT NULL DEFAULT '{}'::jsonb;
