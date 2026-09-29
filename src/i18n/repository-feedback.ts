import type { MessageKey } from "./index.ts";

export const repositoryErrorKeys = {
  multiple_locations: "repo.location_error.multiple_locations",
  identity_mismatch: "repo.location_error.identity_mismatch",
  access_denied: "repo.location_error.access_denied",
  read_only: "repo.location_error.read_only",
  catalog_invalid: "repo.location_error.catalog_invalid",
  io_failure: "repo.location_error.io_failure",
  storage_full: "repo.location_error.storage_full",
  connection_lost: "repo.location_error.connection_lost",
  schema_too_new: "repo.location_error.schema_too_new",
  migration_failed: "repo.location_error.migration_failed",
  timeout: "repo.location_error.timeout",
  unsupported_location: "repo.location_error.unsupported_location",
  busy: "repo.location_error.busy",
  invalid_path: "repo.location_error.invalid_path",
  active_location: "repo.location_error.active_location",
  not_found: "repo.location_error.not_found",
} as const satisfies Record<string, MessageKey>;

/** 库卡片、设置与位置操作使用同一原因文案；原始异常仅留诊断。 */
export function repositoryErrorKey(code: unknown): MessageKey {
  return typeof code === "string" && Object.prototype.hasOwnProperty.call(repositoryErrorKeys, code)
    ? repositoryErrorKeys[code as keyof typeof repositoryErrorKeys] : repositoryErrorKeys.io_failure;
}
export function locationErrorKey(error: unknown): MessageKey {
  return repositoryErrorKey(error && typeof error === "object" && "code" in error ? error.code : null);
}
