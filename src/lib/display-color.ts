/** Device presentation facts only. No pixels, ICC parsing, UI or OS access. */
export type DisplayColorState =
  | { kind: "icc"; profile_path: string }
  | { kind: "systemManaged"; output_space: "srgb" | "scRgb"; sdr_white_nits: number | null }
  | { kind: "srgbFallback"; reason: string }
  | { kind: "unavailable"; reason: string };
export interface DisplaySnapshot { displayId: string | null; state: DisplayColorState }
export interface DisplayPresentation {
  kind: "icc" | "systemManaged" | "srgbFallback" | "unavailable";
  displayId: string | null;
  profilePath: string | null;
  outputSpace: "srgb" | "scRgb";
  sdrWhiteNits: number | null;
  reason: "missingProfile" | "preparationFailed" | "limitedOutput" | "systemUnavailable" | null;
  diagnostic: string | null;
  generation: number | null;
}

export type DisplayPhase = "pending" | "inactive" | "error";
export function displayColorMode(status: DisplayPresentation | null, phase: DisplayPhase = "pending") {
  if (status === null) return phase;
  switch (status.kind) {
    case "icc": return "icc";
    case "systemManaged": return status.outputSpace === "scRgb" ? "scRgb" : "systemSrgb";
    case "srgbFallback": return status.reason === "preparationFailed" ? "preparationFailed" : "fallback";
    case "unavailable": return "unavailable";
  }
}

/** Hide an old successful result while the window/profile policy is changing. */
export function displayPresentationMatches(status: DisplayPresentation, detected: DisplaySnapshot): boolean {
  if (status.displayId !== null && detected.displayId !== null && status.displayId !== detected.displayId) return false;
  if (status.kind === "icc") return detected.state.kind === "icc" && status.profilePath === detected.state.profile_path;
  if (status.kind === "systemManaged") return detected.state.kind === "systemManaged"
    && status.sdrWhiteNits === detected.state.sdr_white_nits;
  if (status.kind === "unavailable") return detected.state.kind === "unavailable";
  if (status.reason === "missingProfile") return detected.state.kind === "srgbFallback";
  return true;
}
