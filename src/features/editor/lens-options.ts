/** 镜头弹窗的搜索与近似推荐；只排列配置文件，不猜自动校正。 */
import type { LensMatch, LensProfile } from "../../api/types.ts";

function isZoom(profile: LensProfile): boolean {
  return profile.focalMax > profile.focalMin + 0.1;
}

function targetZoom(match: LensMatch): boolean | null {
  if (match.detected !== null) return isZoom(match.detected);
  const name = match.lensName ?? "";
  const range = name.match(/(\d+(?:\.\d+)?)\s*[-–]\s*(\d+(?:\.\d+)?)/);
  return range === null ? null : Number(range[2]) > Number(range[1]);
}

/** 焦段相近且变焦/定焦类型相同的候选排在最前。 */
export function suggestedLensProfiles(match: LensMatch): LensProfile[] {
  const focal = match.focalMm;
  if (focal === null || !Number.isFinite(focal) || focal <= 0) {
    return match.detected === null ? [] : [match.detected];
  }
  const zoom = targetZoom(match);
  return match.candidates
    .filter((candidate) => zoom === null || isZoom(candidate) === zoom)
    .map((candidate) => ({
      candidate,
      distance: focal < candidate.focalMin ? candidate.focalMin - focal
        : focal > candidate.focalMax ? focal - candidate.focalMax : 0,
    }))
    .filter(({ distance }) => distance <= Math.max(8, focal * 0.25))
    .sort((a, b) => Number(b.candidate.key === match.detected?.key) - Number(a.candidate.key === match.detected?.key)
      || Number(b.candidate.maker === match.detected?.maker) - Number(a.candidate.maker === match.detected?.maker)
      || a.distance - b.distance || a.candidate.model.localeCompare(b.candidate.model))
    .slice(0, 8)
    .map(({ candidate }) => candidate);
}

export function searchLensProfiles(match: LensMatch, query: string): LensProfile[] {
  const normalized = normalizeLensText(query);
  const tokens = normalized.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return match.candidates;
  const numbers = tokens.filter((token) => /^\d+(?:\.\d+)?$/.test(token)).map(Number);
  return match.candidates
    .map((profile, index) => {
      const text = normalizeLensText(`${profile.maker} ${profile.model} ${profile.focalMin} ${profile.focalMax}`);
      const words = new Set(text.split(/\s+/));
      const matches = tokens.every((token) => /^\d+(?:\.\d+)?$/.test(token)
        ? words.has(token) : text.includes(token));
      const exactRange = numbers.length >= 2 && profile.focalMin === numbers[0] && profile.focalMax === numbers[1];
      return { profile, index, matches, exactRange };
    })
    .filter((item) => item.matches)
    .sort((a, b) => Number(b.exactRange) - Number(a.exactRange) || a.index - b.index)
    .map((item) => item.profile);
}

/** 仅规范搜索文本；不根据俗称自动套用另一支镜头的标定。 */
function normalizeLensText(value: string): string {
  let text = value.normalize("NFKC").toLowerCase();
  const aliases: readonly [string, RegExp][] = [
    ["panasonic", /松下|\b(?:panasonic|lumix)\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
    ["olympus", /奥林巴斯|奥巴|\b(?:olympus|om[\s-]*system|om[\s-]*digital)\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
    ["canon", /佳能|\bcanon\b/g], ["nikon", /尼康|尼克尔|\b(?:nikon|nikkor)\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
    ["sony", /索尼|\bsony\b/g], ["sigma", /适马|\bsigma\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
    ["tamron", /腾龙|\btamron\b/g], ["fujifilm", /富士|\b(?:fujifilm|fuji)\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
    ["pentax", /宾得|\bpentax\b/g], ["tokina", /图丽|\btokina\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
    ["samyang", /三阳|\bsamyang\b/g], ["leica", /徕卡|莱卡|\bleica\b/g], // i18n-exempt: 跨语言检索别名/单位数据，不是界面文案
  ];
  for (const [brand, expression] of aliases) text = text.replace(expression, ` ${brand} `);
  return text.replace(/(\d)\s*(?:mm|毫米)/g, "$1 ") // i18n-exempt: 检索单位数据，不是界面文案
    .replace(/(\d)([a-z])/g, "$1 $2").replace(/([a-z])(\d)/g, "$1 $2")
    .replace(/\d+(?:\.\d+)?/g, (number) => String(Number(number)))
    .replace(/[^\p{L}\p{N}.]+/gu, " ").replace(/\s+/g, " ").trim();
}

/** The saved key is a selection; automatic matching only proposes a key until explicitly applied. */
export function chosenLensProfile(match: LensMatch | null, chosen: string | null): LensProfile | null {
  if (chosen === null || chosen === "none") return null;
  const known = match?.candidates.find((item) => item.key === chosen);
  if (known) return known;
  // Show a saved selection before opening the library; the stable key already contains its name.
  const separator = chosen.indexOf("|");
  if (separator <= 0 || separator === chosen.length - 1) return null;
  return {key: chosen, maker: chosen.slice(0, separator), model: chosen.slice(separator + 1),
    rectilinear: true, focalMin: 0, focalMax: 0};
}
export function appliedLensProfile(match: LensMatch | null, chosen: string | null, enabled: boolean): {
  mode: "manual" | "none" | "disabled";
  profile: LensProfile | null;
} {
  if (!enabled) return { mode: "disabled", profile: null };
  if (match !== null && (match.focalMm === null || !Number.isFinite(match.focalMm) || match.focalMm <= 0)) {
    return { mode: "none", profile: null };
  }
  if (match !== null && !match.candidates.some(item => item.key === chosen)) return { mode: "none", profile: null };
  const profile = chosenLensProfile(match, chosen);
  return { mode: profile === null ? "none" : "manual", profile };
}
