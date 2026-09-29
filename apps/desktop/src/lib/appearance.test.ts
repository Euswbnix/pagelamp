import { describe, expect, it } from "vitest";
import { platformFromUserAgent, staticAppearance } from "./appearance";

const WINDOWS =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36 Edg/129.0.0.0";
const MAC =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";

describe("appearance contexts", () => {
  it("tells the platform from the web view's user agent", () => {
    expect(platformFromUserAgent(WINDOWS)).toBe("windows");
    expect(platformFromUserAgent(MAC)).toBe("macos");
    expect(platformFromUserAgent(LINUX)).toBe("linux");
  });

  it("uses Mica only when the Rust side built a Windows window with it", () => {
    const base = { mock: false, search: "" };
    expect(staticAppearance({ ...base, userAgent: WINDOWS, windowBackdrop: "mica" })).toEqual({
      platform: "windows",
      backdrop: "mica",
    });
    expect(staticAppearance({ ...base, userAgent: WINDOWS })).toEqual({
      platform: "windows",
      backdrop: "none",
    });
    expect(staticAppearance({ ...base, userAgent: LINUX, windowBackdrop: "mica" }).backdrop).toBe(
      "none",
    );
  });

  it("lets mock mode simulate another system, and only mock mode", () => {
    const search = "?scenario=demo&platform=windows&backdrop=mica";
    expect(staticAppearance({ mock: true, search, userAgent: MAC })).toEqual({
      platform: "windows",
      backdrop: "mica",
    });
    expect(staticAppearance({ mock: false, search, userAgent: MAC })).toEqual({
      platform: "macos",
      backdrop: "none",
    });
  });
});
