-- 自动调整产生的可恢复基线；旧栈没有来源信息，保持 NULL。
ALTER TABLE develop_stacks ADD COLUMN auto_adjust TEXT;
