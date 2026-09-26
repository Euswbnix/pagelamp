import { describe, expect, it } from "vitest";
import { aiMaterialsState } from "./types";

describe("aiMaterialsState (§3 rule 8 precedence)", () => {
  it("'No AI' withholds materials whatever the switch says", () => {
    expect(aiMaterialsState({ ai_policy: "prohibited", ai_access: true })).toBe(
      "withheld_by_policy",
    );
    expect(aiMaterialsState({ ai_policy: "prohibited", ai_access: false })).toBe(
      "withheld_by_policy",
    );
  });

  it("otherwise the switch decides, and 'Not set' counts as readable", () => {
    expect(aiMaterialsState({ ai_policy: "learning_aid", ai_access: false })).toBe("turned_off");
    expect(aiMaterialsState({ ai_policy: "unknown", ai_access: true })).toBe("readable");
    expect(aiMaterialsState({ ai_policy: "unrestricted", ai_access: true })).toBe("readable");
  });
});
