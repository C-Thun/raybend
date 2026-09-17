/**
 * `public/` 里静态资源的 base 感知 URL。
 *
 * 站点部署在自定义域名的根路径（`base = '/'`），所以平时就是 `/logo.webp`；
 * 但在自定义域名生效前想用 `https://c-thun.github.io/raybend/` 预览时，
 * 可用 `RAYBEND_BASE=/raybend/` 构建，此时必须走这里拼路径，否则图会 404。
 */
export function asset(path: string): string {
  const base = import.meta.env.BASE_URL || '/';
  return `${base.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`;
}
