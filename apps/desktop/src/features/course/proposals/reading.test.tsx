import { act, screen, waitFor, within } from "@testing-library/react";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GenEvent } from "@/api/ai";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { renderRoute } from "@/test/render";
import { openCourse } from "../testing";

// Courses of the "proposals" mock scenario (src/api/mock/proposals.ts); it routes AI to an
// OpenAI key (gpt-6-luna), so AI reading can run.
const FITTED = "canvas:canvas.demo.test/course/332"; // AI proposal already; outline readable
const UNLABELLED = "canvas:canvas.demo.test/course/240"; // scan proposal already
const LEGACY = "canvas:canvas.demo.test/course/205"; // accepted calendar gone stale
const DEMO101 = "folder:demo-courses/course/DEMO101"; // demo: a course syllabus

beforeEach(() => {
  toast.dismiss();
});

function mockApi() {
  return createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "proposals" });
}

async function openReading(courseId: string, api = mockApi()) {
  const result = await openCourse(courseId, { query: "tab=timeline", scenario: "proposals", api });
  const section = await screen.findByRole("region", { name: "Read with AI" });
  return { ...result, section };
}

describe("Read the syllabus with AI", () => {
  it("shows the cost first, runs, labels the proposal and asks question (b) once", async () => {
    const { user, section } = await openReading(UNLABELLED);
    const read = within(section).getByRole("button", { name: "Read the syllabus with AI" });
    // "≈ $x" before the run.
    await waitFor(() => expect(within(section).getByText(/^Up to|^Less than/)).toBeInTheDocument());

    await user.click(read);
    expect(await within(section).findByText(/^Dates read\./)).toBeInTheDocument();
    // The new AI proposal, labelled like every AI output.
    const proposals = await screen.findByRole("region", { name: "Proposed dates" });
    expect(
      within(proposals).getByText(/^AI-generated · OpenAI · gpt-6-luna · /),
    ).toBeInTheDocument();
    // The course's first cloud run with question (b) unanswered: the one-time reminder.
    const reminder = within(section).getByRole("region", {
      name: /^Check whether your instructor allows sharing course materials with AI services /,
    });
    await user.click(within(reminder).getByRole("button", { name: "Not sure" }));
    await waitFor(() => expect(within(section).queryByRole("region")).toBeNull());

    // A second run: no reminder again.
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(await within(section).findByText(/^Dates read\./)).toBeInTheDocument();
    expect(within(section).queryByRole("region")).toBeNull();
  });

  it("shows who reads it and the stage, and Stop ends it without changes", async () => {
    const api = mockApi();
    let finish: (error: unknown) => void = () => {};
    api.readCourseCalendar = vi.fn(async (_c, id, _o, onEvent: (e: GenEvent) => void) => {
      onEvent({
        type: "started",
        generation_id: id,
        backend_label: "OpenAI",
        model: "gpt-6-luna",
        on_device: false,
      });
      onEvent({ type: "stage", stage: "waiting_for_model" });
      return new Promise<never>((_, reject) => {
        finish = reject;
      });
    });
    const cancel = vi.spyOn(api, "cancelGeneration");
    const { user, section } = await openReading(FITTED, api);

    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(
      await within(section).findByText("Reading with OpenAI · gpt-6-luna"),
    ).toBeInTheDocument();
    expect(within(section).getByText("Waiting for the model…")).toBeInTheDocument();
    expect(within(section).queryByRole("button", { name: "Read the syllabus with AI" })).toBeNull();

    await user.click(within(section).getByRole("button", { name: "Stop" }));
    expect(cancel).toHaveBeenCalledWith(expect.any(String));
    expect(within(section).getByRole("button", { name: "Stopping…" })).toBeInTheDocument();
    await act(async () => finish(new ApiError("cancelled", "Stopped")));
    expect(await within(section).findByText("Stopped. Nothing was changed.")).toBeInTheDocument();
    expect(
      within(section).getByRole("button", { name: "Read the syllabus with AI" }),
    ).toBeVisible();
  });

  // Named by its title and its course.
  const REMINDER =
    /^Check whether your instructor allows sharing course materials with AI services /;
  /**
   * Runs that send their text and then end with `error`, in the facade's order: Started, a
   * stage, the failure, the reminder (only when `notice` says the run said it), Finished.
   */
  function endingWith(
    api: ReturnType<typeof mockApi>,
    error: () => Promise<ApiError>,
    notice: (run: number) => boolean = () => true,
  ) {
    let runs = 0;
    api.readCourseCalendar = vi.fn(async (_c, id, _o, onEvent: (e: GenEvent) => void) => {
      const run = runs++;
      onEvent({
        type: "started",
        generation_id: id,
        backend_label: "OpenAI",
        model: "gpt-6-luna",
        on_device: false,
      });
      onEvent({ type: "stage", stage: "waiting_for_model" });
      const reason = await error();
      if (notice(run)) onEvent({ type: "notice", code: "material_sharing_reminder" });
      onEvent({ type: "finished", ok: false });
      throw reason;
    });
  }
  const badOutput = async () =>
    new ApiError("model", "No dates it could check.", { model_error: "bad_output" });

  it("asks question (b) after a reading that failed once its text went out", async () => {
    const api = mockApi();
    endingWith(api, badOutput);
    const { user, section } = await openReading(UNLABELLED, api);
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(await within(section).findByRole("alert")).toBeInTheDocument();
    const reminder = within(section).getByRole("region", { name: REMINDER });
    expect(within(reminder).getByText(/materials to OpenAI\./)).toBeInTheDocument();
    await user.click(within(reminder).getByRole("button", { name: "Not sure" }));
    await waitFor(() =>
      expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull(),
    );
    // The failure stays said.
    expect(within(section).getByRole("alert")).toBeInTheDocument();
  });

  it("asks question (b) when the provider fails the reading (the mock's own failure)", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "ai-errors" });
    const { user, section } = await openReading(DEMO101, api);
    await user.click(await within(section).findByRole("button", { name: "Use it anyway" }));
    const read = within(section).getByRole("button", { name: "Read the syllabus with AI" });
    await waitFor(() => expect(read).not.toHaveAttribute("aria-disabled"));
    await user.click(read);
    expect(await within(section).findByRole("alert")).toHaveTextContent(/limiting requests/);
    expect(within(section).getByRole("region", { name: REMINDER })).toBeInTheDocument();
  });

  it("asks nothing after a failed or stopped reading that didn't say the reminder", async () => {
    const api = mockApi();
    let stop: () => void = () => {};
    let fail = true;
    endingWith(
      api,
      () =>
        fail
          ? badOutput()
          : new Promise((resolve) => {
              stop = () => resolve(new ApiError("cancelled", "Stopped"));
            }),
      () => false,
    );
    vi.spyOn(api, "cancelGeneration").mockImplementation(async () => stop());
    const { user, section } = await openReading(UNLABELLED, api);
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(await within(section).findByRole("alert")).toBeInTheDocument();
    expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull();

    fail = false;
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    await user.click(await within(section).findByRole("button", { name: "Stop" }));
    expect(await within(section).findByText("Stopped. Nothing was changed.")).toBeInTheDocument();
    expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull();
  });

  it("asks question (b) after a reading stopped once it started, until dismissed", async () => {
    const api = mockApi();
    let stop: () => void = () => {};
    endingWith(
      api,
      () =>
        new Promise((resolve) => {
          stop = () => resolve(new ApiError("cancelled", "Stopped"));
        }),
    );
    vi.spyOn(api, "cancelGeneration").mockImplementation(async () => stop());
    const { user, section } = await openReading(UNLABELLED, api);
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    await user.click(await within(section).findByRole("button", { name: "Stop" }));
    expect(await within(section).findByText("Stopped. Nothing was changed.")).toBeInTheDocument();
    const reminder = within(section).getByRole("region", { name: REMINDER });
    expect(
      within(reminder).getByRole("group", { name: /^Is sharing DEM240.*'s materials allowed\?$/ }),
    ).toBeInTheDocument();
    await user.click(within(reminder).getByRole("button", { name: /^Dismiss / }));
    expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull();
    expect(within(section).getByText("Stopped. Nothing was changed.")).toBeInTheDocument();
    // Focus goes back to the section, not to the page.
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 3, name: "Read with AI" })).toHaveFocus(),
    );
  });

  it("closes question (b) once it is answered elsewhere", async () => {
    const api = mockApi();
    endingWith(api, badOutput);
    const { user, section, queryClient } = await openReading(UNLABELLED, api);
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(await within(section).findByRole("region", { name: REMINDER })).toBeInTheDocument();
    // Answered in the course's Policy tab.
    await api.setCourseMaterialSharing(UNLABELLED, "allowed");
    await act(() => queryClient.invalidateQueries());
    await waitFor(() =>
      expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull(),
    );
    // Done with, not just hidden: a later "not sure" doesn't bring it back.
    await api.setCourseMaterialSharing(UNLABELLED, "not_sure");
    await act(() => queryClient.invalidateQueries());
    await screen.findByRole("tablist", { name: "Course sections" });
    expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull();
  });

  it("keeps question (b) through another reading until it is answered", async () => {
    const api = mockApi();
    // Said once (the facade claims it): the first run says it, the next one doesn't.
    endingWith(api, badOutput, (run) => run === 0);
    const { user, section } = await openReading(UNLABELLED, api);
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(await within(section).findByRole("region", { name: REMINDER })).toBeInTheDocument();
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    await waitFor(() => expect(api.readCourseCalendar).toHaveBeenCalledTimes(2));
    expect(await within(section).findByRole("alert")).toBeInTheDocument();
    expect(within(section).getByRole("region", { name: REMINDER })).toBeInTheDocument();
  });

  it("reads with a model a server here runs in the cloud: its note, its price, then the reminder", async () => {
    const api = mockApi();
    const ollama = { kind: "provider", provider_id: "ollama" } as const;
    await api.addModelProvider("ollama", "http://127.0.0.1:11434", null);
    const own = (await api.aiStatus()).backends.find((b) => b.label === "Ollama");
    if (!own) throw new Error("no Ollama");
    await api.acknowledgeAiDisclosure(ollama, own.disclosure.version);
    await api.setFeatureModel("course_calendar", {
      backend: ollama,
      model: "gpt-oss:120b-cloud",
      effort: "lowest",
    });
    const { user, section } = await openReading(UNLABELLED, api);
    await user.click(await within(section).findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    await user.click(within(sheet).getByRole("button", { name: "Use gpt-oss:120b-cloud" }));
    await user.click(await within(section).findByRole("button", { name: "Use it anyway" }));
    const read = await within(section).findByRole("button", { name: "Read the syllabus with AI" });
    await waitFor(() => expect(read).not.toHaveAttribute("aria-disabled"));
    await user.click(read);
    expect(await within(section).findByText(/^Dates read\./)).toBeInTheDocument();
    // It went to the cloud: the course's one-time question (b).
    expect(within(section).getByRole("region", { name: REMINDER })).toBeInTheDocument();
  });

  it("keeps the note open when the reading changes under it, and focus in the section", async () => {
    const api = mockApi();
    const ollama = { kind: "provider", provider_id: "ollama" } as const;
    await api.addModelProvider("ollama", "http://127.0.0.1:11434", null);
    const own = (await api.aiStatus()).backends.find((b) => b.label === "Ollama");
    if (!own) throw new Error("no Ollama");
    await api.acknowledgeAiDisclosure(ollama, own.disclosure.version);
    await api.setFeatureModel("course_calendar", {
      backend: ollama,
      model: "gpt-oss:120b-cloud",
      effort: "lowest",
    });
    const { user, section, queryClient } = await openReading(UNLABELLED, api);
    await user.click(await within(section).findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });

    // Asked again meanwhile, the estimate now refuses the course: Generate makes way.
    const estimate = api.estimateGeneration;
    vi.spyOn(api, "estimateGeneration").mockImplementation(async (request) => ({
      ...(await estimate(request)),
      would_block: "material_sharing_not_allowed",
    }));
    await act(() => queryClient.invalidateQueries());
    expect(
      await within(section).findByRole("region", { name: "Not sent to an AI service" }),
    ).toBeInTheDocument();
    expect(within(section).queryByRole("button", { name: "Read it now" })).toBeNull();
    // The note is still there to finish, and focus then stays in the section.
    await user.click(within(sheet).getByRole("button", { name: "Use gpt-oss:120b-cloud" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 3, name: "Read with AI" })).toHaveFocus(),
    );
  });

  it("keeps focus in the section when Generate goes after a step settled there", async () => {
    const api = mockApi();
    const openai = { kind: "provider", provider_id: "openai" } as const;
    await api.setFeatureModel("course_calendar", {
      backend: openai,
      model: "gpt-6-preview-0929",
      effort: "lowest",
    });
    // Once the price is accepted, the estimate asked again refuses the course.
    const estimate = api.estimateGeneration;
    let accepted = false;
    const acknowledge = api.acknowledgeUnpricedModel;
    vi.spyOn(api, "acknowledgeUnpricedModel").mockImplementation(async (backend, model) => {
      await acknowledge(backend, model);
      accepted = true;
    });
    vi.spyOn(api, "estimateGeneration").mockImplementation(async (request) => {
      const real = await estimate(request);
      return accepted ? { ...real, would_block: "material_sharing_not_allowed" } : real;
    });
    const { user, section } = await openReading(UNLABELLED, api);
    await user.click(await within(section).findByRole("button", { name: "Use it anyway" }));
    expect(
      await within(section).findByRole("region", { name: "Not sent to an AI service" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 3, name: "Read with AI" })).toHaveFocus(),
    );
  });

  it("says why it can't run for a No AI course, and respects question (b)", async () => {
    const api = mockApi();
    await api.setCoursePolicy(FITTED, "prohibited", null);
    const first = await openReading(FITTED, api);
    expect(within(first.section).getByText(/^This course is marked “No AI”/)).toBeInTheDocument();
    expect(within(first.section).queryByRole("button", { name: /Read the syllabus/ })).toBeNull();
    first.unmount();

    const other = mockApi();
    await other.setCourseMaterialSharing(UNLABELLED, "not_allowed");
    const second = await openReading(UNLABELLED, other);
    expect(
      await within(second.section).findByRole("region", { name: "Not sent to an AI service" }),
    ).toBeInTheDocument();
    expect(within(second.section).queryByRole("button", { name: /Read the syllabus/ })).toBeNull();
  });

  it("finds dates without AI, or says there's nothing new", async () => {
    const { user } = await openReading(FITTED);
    await user.click(screen.getByRole("button", { name: "Find dates without AI" }));
    expect(await screen.findByText(/^Found dates\./)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Find dates without AI" }));
    expect(await screen.findByText("No new dates found in these materials.")).toBeInTheDocument();
  });
});

describe("Read the syllabus with AI: setup and staleness", () => {
  it("without a model, offers the setup and nothing more (and no batch on the home screen)", async () => {
    const api = mockApi();
    await api.setFeatureModel("course_calendar", null);
    const { section, unmount } = await openReading(UNLABELLED, api);
    const setUp = await within(section).findByRole("link", { name: "Set up AI to read syllabi" });
    expect(setUp).toHaveAttribute("href", "/settings");
    expect(within(section).queryByRole("button", { name: /Read the syllabus/ })).toBeNull();
    unmount();

    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 2, name: "This week" });
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Read syllabi with AI" })).toBeNull(),
    );
  });

  it("says when the reading stays on this computer", async () => {
    const api = mockApi();
    api.readCourseCalendar = vi.fn(async (_c, id, _o, onEvent: (e: GenEvent) => void) => {
      onEvent({
        type: "started",
        generation_id: id,
        backend_label: "Ollama",
        model: "qwen3.5:9b",
        on_device: true,
      });
      return new Promise<never>(() => {});
    });
    const { user, section } = await openReading(FITTED, api);
    await user.click(within(section).getByRole("button", { name: "Read the syllabus with AI" }));
    expect(
      await within(section).findByText("Reading on this computer with Ollama · qwen3.5:9b"),
    ).toBeInTheDocument();
  });

  it('takes a stale calendar\'s "read again" to the reading section', async () => {
    const { user } = await openCourse(LEGACY, { query: "tab=timeline", scenario: "proposals" });
    await user.click(await screen.findByRole("button", { name: "Read the syllabus again" }));
    expect(screen.getByRole("heading", { level: 3, name: "Read with AI" })).toHaveFocus();
  });
});

describe("Read syllabi for N courses", () => {
  it("reads each offered course, lists the outcomes and accepts the passing ones", async () => {
    const api = mockApi();
    const offers = await api.syllabusReadingOffers();
    expect(offers.length).toBeGreaterThan(1);
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const read = await within(region).findByRole("button", {
      name: `Read syllabi for ${offers.length} courses`,
    });
    await waitFor(() => expect(within(region).getByText(/^Up to|^Less than/)).toBeInTheDocument());

    await user.click(read);
    expect(
      await within(region).findByText(
        new RegExp(`^Done: ${offers.length} courses have dates to check\\.$`),
      ),
    ).toBeInTheDocument();
    const rows = within(region).getAllByRole("listitem");
    expect(rows).toHaveLength(offers.length);
    expect(within(rows[0] as HTMLElement).getByRole("link")).toHaveAttribute(
      "href",
      expect.stringContaining("tab=timeline"),
    );

    const accept = within(region).getByRole("button", { name: /^Accept the \d+ that passed/ });
    await user.click(accept);
    expect(await within(region).findByText(/^Dates accepted for \d+ course/)).toBeInTheDocument();
  });

  it("goes away for two weeks with Not now, and focus moves to the heading", async () => {
    const api = mockApi();
    const snooze = vi.spyOn(api, "snoozeCalendarOffers");
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    await user.click(
      within(region).getByRole("button", { name: "Not now: reading syllabi with AI" }),
    );
    expect(await screen.findByText("We'll offer it again in two weeks.")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Read syllabi with AI" })).toBeNull(),
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveFocus();
    expect(snooze).toHaveBeenCalledTimes(1);
    // The facade offers none of them now, on the card and at startup alike.
    expect(await api.syllabusReadingOffers()).toEqual([]);
    expect((await api.startupTasks()).calendar_offers_total).toBe(0);
  });

  it("stops the rest when asked", async () => {
    const api = mockApi();
    const offers = await api.syllabusReadingOffers();
    const real = api.readCourseCalendars;
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    api.readCourseCalendars = vi.fn(async (ids, batchId, options, onEvent) => {
      onEvent({ type: "course_started", course_id: ids[0] as string, index: 0, total: ids.length });
      await gate;
      return real(ids, batchId, options, onEvent);
    });
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const read = await within(region).findByRole("button", { name: /^Read syllabi for/ });
    await waitFor(() => expect(read).not.toHaveAttribute("aria-disabled"));
    await user.click(read);
    expect(
      await within(region).findByText(new RegExp(`^Reading 1 of ${offers.length} · `)),
    ).toBeInTheDocument();

    await user.click(within(region).getByRole("button", { name: "Stop the rest" }));
    release();
    expect(await within(region).findByText(/^Stopped\. 0 courses have dates/)).toBeInTheDocument();
  });

  // Named by its title and its course.
  const REMINDER =
    /^Check whether your instructor allows sharing course materials with AI services /;

  it("asks question (b) for each course whose text went out, also after Done", async () => {
    const api = mockApi();
    const { user, queryClient } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const read = await within(region).findByRole("button", { name: /^Read syllabi for/ });
    await waitFor(() => expect(read).not.toHaveAttribute("aria-disabled"));
    await user.click(read);
    expect(await within(region).findByText(/^Done: /)).toBeInTheDocument();
    // The courses with question (b) unanswered, each named once, its answers named by it too.
    const reminders = within(region).getAllByRole("region", { name: REMINDER });
    expect(reminders.length).toBeGreaterThan(1);
    const named = reminders.map((r) => within(r).getByText(/sent text from/).textContent);
    expect(new Set(named).size).toBe(named.length);
    const groups = reminders.map((r) => within(r).getByRole("group").textContent);
    expect(new Set(groups).size).toBe(groups.length);
    // Each region and its Dismiss say its course.
    for (const group of groups) {
      const code = /^Is sharing (.+)'s materials allowed\?/.exec(group ?? "")?.[1] ?? "";
      const reminder = within(region).getByRole("region", { name: new RegExp(` ${code}$`) });
      expect(within(reminder).getByRole("button", { name: `Dismiss ${code}` })).toBeInTheDocument();
    }

    await user.click(
      within(reminders[0] as HTMLElement).getByRole("button", { name: /^Dismiss / }),
    );
    await waitFor(() =>
      expect(within(region).getAllByRole("region", { name: REMINDER })).toHaveLength(
        reminders.length - 1,
      ),
    );
    // Focus goes back to the card, from where the next one is a Tab away.
    await waitFor(() =>
      expect(within(region).getByRole("heading", { name: "Read syllabi with AI" })).toHaveFocus(),
    );
    // Putting the outcomes away keeps the questions still open.
    await user.click(within(region).getByRole("button", { name: "Done" }));
    const left = within(region).getAllByRole("region", { name: REMINDER });
    expect(left).toHaveLength(reminders.length - 1);
    // An answer given on a course's page closes its reminder here too.
    const legend = within(left[0] as HTMLElement).getByRole("group").textContent ?? "";
    const code = /^Is sharing (.+)'s materials allowed\?/.exec(legend)?.[1];
    const course = (await api.listCourses()).find((c) => (c.course.code ?? c.course.name) === code);
    if (!course) throw new Error(`no course ${code}`);
    await api.setCourseMaterialSharing(course.course.id, "not_allowed");
    await act(() => queryClient.invalidateQueries());
    await waitFor(() =>
      expect(within(region).queryAllByRole("region", { name: REMINDER })).toHaveLength(
        reminders.length - 2,
      ),
    );
  });

  it("keeps a course's reminder from a batch for its page, answered there once", async () => {
    const api = mockApi();
    const { user, router } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const read = await within(region).findByRole("button", { name: /^Read syllabi for/ });
    await waitFor(() => expect(read).not.toHaveAttribute("aria-disabled"));
    await user.click(read);
    expect(await within(region).findByText(/^Done: /)).toBeInTheDocument();
    const before = within(region).getAllByRole("region", { name: REMINDER }).length;
    // Off to the course's page through its outcome's link: the reminder is there.
    await user.click(within(region).getByRole("link", { name: /^DEM240/ }));
    const section = await screen.findByRole("region", { name: "Read with AI" });
    const reminder = await within(section).findByRole("region", { name: REMINDER });
    await user.click(within(reminder).getByRole("button", { name: "Yes, it's allowed" }));
    await waitFor(() =>
      expect(within(section).queryByRole("region", { name: REMINDER })).toBeNull(),
    );
    // Back on the Courses page: that one is done there too.
    await act(() => router.navigate("/courses"));
    const again = await screen.findByRole("region", { name: "Read syllabi with AI" });
    await waitFor(() =>
      expect(within(again).getAllByRole("region", { name: REMINDER })).toHaveLength(before - 1),
    );
  });

  it("puts focus on the page when the last reminder goes and the card with it", async () => {
    const api = mockApi();
    const offers = api.syllabusReadingOffers;
    let read = false;
    // Once read, no course is offered any more: the reminders alone keep the card.
    vi.spyOn(api, "syllabusReadingOffers").mockImplementation(async () => (read ? [] : offers()));
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const button = await within(region).findByRole("button", { name: /^Read syllabi for/ });
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    read = true;
    await user.click(button);
    await user.click(await within(region).findByRole("button", { name: "Done" }));
    // Nothing to snooze on a card its reminders alone keep.
    expect(within(region).queryByRole("button", { name: /^Not now/ })).toBeNull();
    for (const reminder of within(region).getAllByRole("region", { name: REMINDER })) {
      await user.click(within(reminder).getByRole("button", { name: /^Dismiss / }));
    }
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Read syllabi with AI" })).toBeNull(),
    );
    await waitFor(() => expect(screen.getByRole("main")).toHaveFocus());
  });

  it("keeps the card for its reminders after Not now, and focus on it", async () => {
    const api = mockApi();
    const offered = await api.syllabusReadingOffers();
    let snoozed = false;
    vi.spyOn(api, "syllabusReadingOffers").mockImplementation(async () => (snoozed ? [] : offered));
    const snooze = api.snoozeCalendarOffers;
    vi.spyOn(api, "snoozeCalendarOffers").mockImplementation(async () => {
      snoozed = true;
      return snooze();
    });
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const button = await within(region).findByRole("button", { name: /^Read syllabi for/ });
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    await user.click(button);
    await user.click(await within(region).findByRole("button", { name: "Done" }));
    expect(within(region).getAllByRole("region", { name: REMINDER }).length).toBeGreaterThan(0);
    await user.click(
      within(region).getByRole("button", { name: "Not now: reading syllabi with AI" }),
    );
    // The offers go; the reminders keep the card, and focus stays on it.
    await waitFor(() =>
      expect(within(region).queryByRole("button", { name: /^Not now/ })).toBeNull(),
    );
    await waitFor(() =>
      expect(within(region).getByRole("heading", { name: "Read syllabi with AI" })).toHaveFocus(),
    );
  });

  it("asks question (b) for a course whose text went out before the batch failed", async () => {
    const api = mockApi();
    api.readCourseCalendars = vi.fn(async (ids, _batchId, _options, onEvent) => {
      const course = ids[0] as string;
      onEvent({ type: "course_started", course_id: course, index: 0, total: ids.length });
      onEvent({
        type: "gen",
        course_id: course,
        event: {
          type: "started",
          generation_id: "g",
          backend_label: "OpenAI",
          model: "gpt-6-luna",
          on_device: false,
        },
      });
      onEvent({
        type: "gen",
        course_id: course,
        event: { type: "notice", code: "material_sharing_reminder" },
      });
      throw new ApiError("internal", "The store is busy.");
    });
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: "Read syllabi with AI" });
    const read = await within(region).findByRole("button", { name: /^Read syllabi for/ });
    await waitFor(() => expect(read).not.toHaveAttribute("aria-disabled"));
    await user.click(read);
    expect(await within(region).findByText("The syllabi couldn't be read")).toBeInTheDocument();
    const reminder = within(region).getByRole("region", { name: REMINDER });
    expect(within(reminder).getByText(/materials to OpenAI\./)).toBeInTheDocument();
  });
});
