import { describe, expect, it } from "vitest";
import { isHttpUrl } from "./url";

describe("isHttpUrl", () => {
  it("allows only http(s) links", () => {
    expect(isHttpUrl("https://canvas.example.edu/courses/1")).toBe(true);
    expect(isHttpUrl("http://example.edu")).toBe(true);
    expect(isHttpUrl("file:///Users/demo/notes.pdf")).toBe(false);
    expect(isHttpUrl("webcal://example.edu/feed.ics")).toBe(false);
    expect(isHttpUrl("javascript:alert(1)")).toBe(false);
    expect(isHttpUrl("not a url")).toBe(false);
    expect(isHttpUrl(null)).toBe(false);
  });
});
