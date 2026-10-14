/**
 * 发布公开配置：**可以进仓库**的非机密信息（更新公钥、渠道基址…）。
 *
 * 私钥与密码永远只走环境变量（`TAURI_SIGNING_PRIVATE_KEY` /
 * `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`），本模块不读、不写、不打印它们。
 * 公钥是公开身份（要进安装包、也会写进发布页面的更新清单），放仓库里
 * 可以免掉「每次发版都记得 export」这个易错步骤；环境变量仍可覆盖它。
 */
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { parseJson } from './json-text.mjs';

export const RELEASE_CONFIG_FILE = 'release.config.json';

/**
 * 取更新签名公钥：`RAYBEND_UPDATER_PUBLIC_KEY` 环境变量优先，
 * 其次仓内 `release.config.json` 的 `updaterPublicKey`；都没有返回空串（= 不带更新构建）。
 */
export function updaterPublicKey(root, env = process.env) {
  const fromEnv = typeof env.RAYBEND_UPDATER_PUBLIC_KEY === 'string' ? env.RAYBEND_UPDATER_PUBLIC_KEY.trim() : '';
  if (fromEnv) return fromEnv;
  const path = join(root, RELEASE_CONFIG_FILE);
  if (!existsSync(path)) return '';
  const config = parseJson(readFileSync(path, 'utf8'), RELEASE_CONFIG_FILE);
  if (config === null || typeof config !== 'object' || Array.isArray(config)) throw new Error(`${RELEASE_CONFIG_FILE} 必须是 JSON 对象`);
  if (config.schema !== 1) throw new Error(`${RELEASE_CONFIG_FILE} schema 必须是 1`);
  const key = config.updaterPublicKey;
  if (key === undefined || key === null || key === '') return '';
  if (typeof key !== 'string' || !key.trim() || key.length > 8192) throw new Error(`${RELEASE_CONFIG_FILE} 的 updaterPublicKey 无效`);
  return key.trim();
}
