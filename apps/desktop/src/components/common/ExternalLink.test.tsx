import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { Providers } from "@/app/Providers";
import { ExternalLink } from "./ExternalLink";

function renderLink(href: string, openExternal = vi.fn().mockResolvedValue(undefined)) {
  const api = { ...createMockApi({ latencyMs: 0 }), openExternal };
  render(
    <Providers api={api}>
      <ExternalLink href={href}>Course website</ExternalLink>
    </Providers>,
  );
  return openExternal;
}

describe("ExternalLink", () => {
  it("opens http(s) links through the desktop helper", async () => {
    const open = renderLink("https://canvas.example.edu/courses/1");
    await userEvent.click(screen.getByRole("link", { name: "Course website" }));
    expect(open).toHaveBeenCalledWith("https://canvas.example.edu/courses/1");
  });

  it("says so when the link can't be opened", async () => {
    renderLink(
      "https://canvas.example.edu/courses/1",
      vi.fn().mockRejectedValue(new ApiError("internal", "no default browser")),
    );
    await userEvent.click(screen.getByRole("link", { name: "Course website" }));
    expect(await screen.findByText("Something unexpected went wrong.")).toBeInTheDocument();
  });

  it("never links anything that isn't http(s)", () => {
    renderLink("file:///Users/demo/secret.pdf");
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.getByText("Course website")).toBeInTheDocument();
  });
});
