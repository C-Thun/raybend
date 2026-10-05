import assert from "node:assert/strict";
import test from "node:test";
import { displayColorMode, displayPresentationMatches, type DisplayPresentation, type DisplaySnapshot } from "./display-color.ts";
const status = (fields: Partial<DisplayPresentation> = {}): DisplayPresentation => ({
  kind:"icc",displayId:"屏幕一",profilePath:"C:/色彩/摄影棚.icc",outputSpace:"srgb",sdrWhiteNits:null,reason:null,diagnostic:null,generation:7,...fields,
});

test("实际状态选择不依赖诊断文本或系统希望的输出空间",()=> {
  assert.equal(displayColorMode(status()),"icc");
  assert.equal(displayColorMode(status({kind:"systemManaged",outputSpace:"scRgb"})),"scRgb");
  assert.equal(displayColorMode(status({kind:"systemManaged",reason:"limitedOutput"})),"systemSrgb");
  assert.equal(displayColorMode(status({kind:"srgbFallback",reason:"missingProfile"})),"fallback");
  assert.equal(displayColorMode(status({kind:"srgbFallback",reason:"preparationFailed",diagnostic:"错误\nCLUT error"})),"preparationFailed");
  assert.equal(displayColorMode(status({kind:"unavailable"})),"unavailable");
  for(const phase of ["pending","inactive","error"] as const) assert.equal(displayColorMode(null,phase),phase);
});

test("跨屏、ICC替换、ACM切换和白电平变化不展示旧成功结论", () => {
  const icc: DisplaySnapshot = {displayId:"屏幕一",state:{kind:"icc",profile_path:"C:/色彩/摄影棚.icc"}};
  assert.equal(displayPresentationMatches(status(),icc),true);
  assert.equal(displayPresentationMatches(status(),{...icc,displayId:"屏幕二"}),false);
  assert.equal(displayPresentationMatches(status(),{...icc,state:{kind:"icc",profile_path:"C:/色彩/新.icc"}}),false);
  const managed: DisplaySnapshot = {displayId:"屏幕一",state:{kind:"systemManaged",output_space:"scRgb",sdr_white_nits:203}};
  assert.equal(displayPresentationMatches(status(),managed),false);
  const system=status({kind:"systemManaged",sdrWhiteNits:203,reason:"limitedOutput"});
  assert.equal(displayPresentationMatches(system,managed),true); // Actual sRGB fallback stays truthful.
  assert.equal(displayPresentationMatches(system,icc),false);
  assert.equal(displayPresentationMatches(system,{...managed,state:{...managed.state as {kind:"systemManaged";output_space:"scRgb";sdr_white_nits:number},sdr_white_nits:80}}),false);
  assert.equal(displayPresentationMatches(status({kind:"srgbFallback",reason:"missingProfile"}),icc),false);
  assert.equal(displayPresentationMatches(status({kind:"unavailable"}),icc),false);
  assert.equal(displayPresentationMatches(status({kind:"srgbFallback",reason:"preparationFailed",displayId:null}),icc),true);
});

