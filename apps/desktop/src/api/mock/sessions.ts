// The mock's "same session", for the dates form's "also use these dates in". The facade works it
// out from the school's session hint; the mock reads UofT-style course codes the same way:
// "<course><H|Y><campus> <F|S|Y> <section> <session>", e.g. "DEM332H5 F LEC0101 20269".

/**
 * The session a course code belongs to, or null when the code says none. In Fall and Winter, F,
 * S and no letter are one group and Y (the full year) its own; in summer, F, S and Y are three.
 */
export function sessionKey(
  code: string | null,
): { id: string; session: string; campus: number } | null {
  const match = code?.match(/^[A-Z]{3}\d{3}[HY](\d)(?:\s+([FSY]))?\s+\S+\s+(\d{5})$/);
  if (!match) return null;
  const [, campus, section = "", session = ""] = match;
  const summer = session.endsWith("5");
  const group = summer ? section || "Y" : section === "Y" ? "Y" : "FS";
  return { id: `${session}/${group}/${campus}`, session, campus: Number(campus) };
}
