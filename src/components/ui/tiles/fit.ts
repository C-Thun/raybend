import { createContext, useContext, type Accessor } from "solid-js";

/** TilesShell 发出的「把当前行铺满」请求；网格拥有宽度事实并负责计算。 */
export const TilesFitRequestContext = createContext<Accessor<number>>();

export function useTilesFitRequest(): Accessor<number> | undefined {
  return useContext(TilesFitRequestContext);
}
