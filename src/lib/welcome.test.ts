import { test } from "node:test";
import assert from "node:assert/strict";
import { shouldWelcome, welcomeAcknowledged, acknowledgeWelcome } from "./welcome.ts";
test("仅原生空库且未确认时显示；未知/损坏值重新提示",()=>{
  assert.equal(shouldWelcome(true,0,null),true);
  assert.equal(shouldWelcome(true,0,"bad"),true);
  for(const args of [[false,0,null],[true,1,null],[true,0,"done"]] as const) assert.equal(shouldWelcome(args[0],args[1],args[2]),false);
});
test("设备存储拒绝访问不会阻断启动",()=>{
  const storage={getItem(){throw new Error("denied");},setItem(){throw new Error("denied");}};
  assert.equal(welcomeAcknowledged(storage),null);assert.doesNotThrow(()=>acknowledgeWelcome(storage));
});
