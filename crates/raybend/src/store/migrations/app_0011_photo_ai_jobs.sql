-- 复用 jobs；父任务准备完成后才允许领取其冻结 UID 子任务。
CREATE UNIQUE INDEX idx_photo_ai_job_identity ON jobs (
    json_extract(payload, '$.runId'),
    json_extract(payload, '$.photo.repositoryId'),
    json_extract(payload, '$.photo.photoUid')
) WHERE kind='photo_ai_image' AND json_valid(payload);
