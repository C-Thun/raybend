-- Missing color_state preserves legacy rendering and canonical JSON/hash.
-- Named issues retain their immutable profile JSON; new schema v2 snapshots
-- explicitly carry resolved input identity and processing version.
ALTER TABLE develop_stacks ADD COLUMN color_state TEXT;
