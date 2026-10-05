import { test } from "node:test";
import assert from "node:assert/strict";
import { aiBuildEnabled, aiCapability } from "./ai-capability.ts";
test("capability requires both build and native confirmation", () => {
 assert.equal(aiBuildEnabled(),false);
 for (const value of [undefined,null,false,0,1,"true",{}]) assert.equal(aiCapability(true,value),false);
 assert.equal(aiCapability(false,true),false);assert.equal(aiCapability(true,true),true);
});
