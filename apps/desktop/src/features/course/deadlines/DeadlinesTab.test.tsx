import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { DEMO099, DEMO101, openCourse } from "../testing";

describe("Deadlines tab", () => {
  it("splits deadlines into coming up and recently past", async () => {
    await openCourse(DEMO101, { query: "tab=deadlines" });

    const upcoming = await screen.findByRole("region", { name: "Coming up" });
    expect(within(upcoming).getByRole("link", { name: "Problem Set 2" })).toBeInTheDocument();
    expect(within(upcoming).getByText("Quiz 3 — Sampling")).toBeInTheDocument();
    expect(within(upcoming).getByText("Midterm test")).toBeInTheDocument();
    expect(within(upcoming).queryByText("Reading response 3")).not.toBeInTheDocument();
    // Class meetings are not deadlines.
    expect(screen.queryByText("Lecture 9")).not.toBeInTheDocument();

    const past = screen.getByRole("region", { name: "Recently past" });
    expect(within(past).getByText("Reading response 3")).toBeInTheDocument();

    expect(screen.getByText(/never downloads assignment instructions/)).toBeInTheDocument();
  });

  it("asks for the next 21 days and the last 7", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const listDeadlines = vi.spyOn(api, "listDeadlines");
    await openCourse(DEMO101, { api, query: "tab=deadlines" });
    await screen.findByRole("region", { name: "Coming up" });
    expect(listDeadlines).toHaveBeenCalledWith(DEMO101, 21, 7);
  });

  it("shows a plain title for a deadline whose URL isn't http(s)", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const real = await api.listDeadlines(DEMO101, 21, 7);
    vi.spyOn(api, "listDeadlines").mockResolvedValue(
      real.map((d) =>
        d.title === "Problem Set 2" ? { ...d, url: "webcal://calendar.demo.test/ps2" } : d,
      ),
    );
    await openCourse(DEMO101, { api, query: "tab=deadlines" });

    const upcoming = await screen.findByRole("region", { name: "Coming up" });
    expect(within(upcoming).getByText("Problem Set 2")).toBeInTheDocument();
    expect(within(upcoming).queryByRole("link", { name: "Problem Set 2" })).not.toBeInTheDocument();
    expect(within(upcoming).getByRole("link", { name: "Quiz 3 — Sampling" })).toBeInTheDocument();
  });

  it("shows empty states when nothing is due", async () => {
    await openCourse(DEMO099, { query: "tab=deadlines" });
    expect(await screen.findByText("Nothing due in the next 21 days")).toBeInTheDocument();
    expect(screen.getByText("Nothing was due in the last 7 days.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Sources & sync" })).toBeInTheDocument();
  });

  it("shows an error with retry", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    vi.spyOn(api, "listDeadlines").mockRejectedValueOnce(new ApiError("internal", "Boom."));
    const { user } = await openCourse(DEMO101, { api, query: "tab=deadlines" });

    expect(await screen.findByText("Couldn't load deadlines")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("region", { name: "Coming up" })).toBeInTheDocument();
  });
});
