import { describe, expect, it } from "vitest";
import { isHttpUrl, isLoopbackUrl } from "./url";

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

describe("isLoopbackUrl", () => {
  it("knows this computer as the facade does: localhost, 127.0.0.0/8 and [::1]", () => {
    for (const here of [
      "http://localhost:8080/v1",
      "http://LOCALHOST:1234",
      "http://127.0.0.1:11434",
      "http://127.4.5.6/v1",
      "http://[::1]:8080/v1",
    ]) {
      expect(isLoopbackUrl(here)).toBe(true);
    }
    for (const elsewhere of [
      "https://llm.example.edu/v1",
      "http://128.0.0.1",
      "http://localhost.example.edu",
      "http://192.168.1.10:11434",
      "localhost:8080",
      "",
    ]) {
      expect(isLoopbackUrl(elsewhere)).toBe(false);
    }
  });
});
