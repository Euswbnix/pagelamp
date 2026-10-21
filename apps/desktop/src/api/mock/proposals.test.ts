import { describe, expect, it } from "vitest";
import type { GenEvent } from "../ai";
import { createMockApi } from ".";

const NOW = new Date(2026, 8, 28, 10, 0); // Monday 2026-09-28
const fast = { latencyMs: 0, syncStepMs: 0, now: () => NOW, scenario: "proposals" as const };
const FITTED = "canvas:canvas.demo.test/course/332";
const UNLABELLED = "canvas:canvas.demo.test/course/240";
const LEGACY = "canvas:canvas.demo.test/course/205";
const DEMO101 = "folder:demo-courses/course/DEMO101";

describe("mock calendar proposals", () => {
  it("shows an AI proposal with a conflict and alternatives, and the candidates", async () => {
    const api = createMockApi(fast);
    const view = await api.courseCalendar(FITTED);
    expect(view.status).toBe("proposed");
    const [proposal] = view.proposals;
    expect(proposal).toMatchObject({ origin: "ai", passing: false, resulting_week_today: 4 });
    expect(proposal?.conflicts[0]?.options).toHaveLength(2);
    // The schedule page's date, and PageLamp's own (no label, no quote).
    expect(proposal?.dates[0]?.alternatives).toHaveLength(2);
    expect(proposal?.conflicts.map((c) => c.options.length)).toEqual([2, 0]);
    expect(view.candidates.map((c) => c.reason).sort()).toEqual(["syllabus", "title_schedule"]);
    expect(view.blocked ?? null).toBeNull();
  });

  it("keeps the notes' week and the last activity when a calendar goes in force", async () => {
    const api = createMockApi(fast);
    // The stale calendar the scenario starts with, in force on its timeline.
    const legacy = await api.courseOverview(LEGACY);
    expect(legacy.timeline.term).toMatchObject({
      own_calendar: true,
      student_dates_confirmed: true,
      anchor_origin: "ai",
      anchor_confidence: "medium",
    });
    expect(legacy.timeline.notes_week).toBe(4);
    expect(legacy.lifecycle.last_activity).toBeTruthy();
    // Accepting another: what the materials and the activity say stays.
    const before = await api.courseOverview(FITTED);
    const [proposal] = (await api.courseCalendar(FITTED)).proposals;
    if (!proposal) throw new Error("the scenario has a proposal for DEM332");
    await api.acceptCalendarProposal(proposal.id, null);
    const after = await api.courseOverview(FITTED);
    expect(after.timeline.notes_week).toBe(before.timeline.notes_week);
    expect(after.lifecycle.last_activity).toBe(before.lifecycle.last_activity);
  });

  it("accepts with edits, which puts the calendar in force", async () => {
    const api = createMockApi(fast);
    const { proposals } = await api.courseCalendar(UNLABELLED);
    const scan = proposals[0];
    expect(scan?.origin).toBe("scan");
    const view = await api.acceptCalendarProposal(scan?.id ?? 0, null);
    expect(view.status).toBe("accepted");
    expect(view.proposals).toEqual([]);
    const { timeline } = await api.courseOverview(UNLABELLED);
    expect(timeline.current_week).toBe(4);
    expect(timeline.term.anchor).toBe("student_confirmed");
    expect(timeline.term.anchor_origin).toBe("scan");
    expect(timeline.calendar).toBe("accepted");
  });

  it("refuses a batch accept of a proposal with conflicts, and dismisses", async () => {
    const api = createMockApi(fast);
    const id = (await api.courseCalendar(FITTED)).proposals[0]?.id ?? 0;
    const error = await api.acceptPassingProposals([id]).catch((e) => e);
    expect(error.kind).toBe("invalid");
    await api.dismissCalendarProposal(id);
    expect((await api.courseCalendar(FITTED)).status).toBe("none");
  });

  it("marks a stale accepted calendar and offers only courses without one", async () => {
    const api = createMockApi(fast);
    const view = await api.courseCalendar(LEGACY);
    expect(view.status).toBe("accepted_stale");
    expect(view.accepted).toMatchObject({
      stale: true,
      changed_materials: ["Course outline (updated)"],
    });
    const offers = (await api.syllabusReadingOffers()).map((o) => o.course_id).sort();
    expect(offers).toEqual([UNLABELLED, FITTED].sort());
  });

  it("downloads one outline file, and lets the student exclude a candidate", async () => {
    const api = createMockApi(fast);
    const outline = (await api.courseCalendar(UNLABELLED)).candidates.find((c) => c.downloadable);
    expect(outline?.title).toBe("Syllabus.pdf");
    await api.downloadMaterialFiles(UNLABELLED, [outline?.material_id ?? ""], () => {});
    let candidates = (await api.courseCalendar(UNLABELLED)).candidates;
    expect(candidates.find((c) => c.title === "Syllabus.pdf")).toMatchObject({
      has_text: true,
      included: true,
    });

    candidates = await api.setCalendarSources(UNLABELLED, [], [outline?.material_id ?? ""]);
    expect(candidates.find((c) => c.title === "Syllabus.pdf")).toMatchObject({
      included: false,
      left_out: "excluded_by_student",
    });
  });

  it("scans a course once", async () => {
    const api = createMockApi({ ...fast, scenario: "uoft-fall" });
    // No outline in plain uoft-fall: nothing to scan.
    expect(await api.scanCourseCalendar(FITTED)).toBeNull();
  });
});

describe("mock syllabus reading: the question (b) reminder", () => {
  const options = { override_budget: false };
  /** Each event's type, a notice by its code. */
  const kinds = (events: GenEvent[]) => events.map((e) => (e.type === "notice" ? e.code : e.type));

  /** Resolves once the run's events include `type`. */
  function once(events: GenEvent[], type: GenEvent["type"]) {
    return new Promise<void>((resolve) => {
      const check = () => (events.some((e) => e.type === type) ? resolve() : setTimeout(check, 1));
      check();
    });
  }

  it("says it before a reading stopped after it started ends, and once a course", async () => {
    // Steps long enough for the stop to land while the run goes.
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    const events: GenEvent[] = [];
    const run = api.readCourseCalendar(UNLABELLED, "gen-1", options, (e) => events.push(e));
    await once(events, "started");
    await api.cancelGeneration("gen-1");
    await expect(run).rejects.toMatchObject({ kind: "cancelled" });
    // Like the facade: its first stage, then Started, and the notice before Finished.
    expect(kinds(events).slice(0, 2)).toEqual(["stage", "started"]);
    expect(kinds(events).slice(-2)).toEqual(["material_sharing_reminder", "finished"]);
    expect(events.at(-1)).toEqual({ type: "finished", ok: false });

    // Claimed by the stopped run: the next one doesn't say it again.
    const again: GenEvent[] = [];
    await api.readCourseCalendar(UNLABELLED, "gen-2", options, (e) => again.push(e));
    expect(kinds(again)).not.toContain("material_sharing_reminder");
  });

  it("says nothing of a reading stopped before it started, and claims nothing", async () => {
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    const events: GenEvent[] = [];
    const run = api.readCourseCalendar(UNLABELLED, "gen-1", options, (e) => events.push(e));
    await api.cancelGeneration("gen-1");
    await expect(run).rejects.toMatchObject({ kind: "cancelled" });
    expect(kinds(events)).toEqual(["stage", "finished"]);
    // Nothing was sent: the next reading still reminds.
    const again: GenEvent[] = [];
    await api.readCourseCalendar(UNLABELLED, "gen-2", options, (e) => again.push(e));
    expect(kinds(again)).toContain("material_sharing_reminder");
  });

  it("says nothing of a reading on this computer, and ends a blocked one too", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-local" });
    const events: GenEvent[] = [];
    await api.readCourseCalendar(DEMO101, "gen-1", options, (e) => events.push(e));
    expect(kinds(events)).not.toContain("material_sharing_reminder");
    // Blocked before it started (no model): its first stage, then Finished, no reminder.
    await api.setFeatureModel("course_calendar", null);
    const blocked: GenEvent[] = [];
    await expect(
      api.readCourseCalendar(DEMO101, "gen-2", options, (e) => blocked.push(e)),
    ).rejects.toMatchObject({ kind: "blocked" });
    expect(kinds(blocked)).toEqual(["stage", "finished"]);
  });

  it("forgets the reminders' claims and the AI calendars not accepted with all AI data", async () => {
    const api = createMockApi(fast);
    await api.readCourseCalendar(UNLABELLED, "gen-1", options, () => {});
    const ai = async (id: string) =>
      (await api.courseCalendar(id)).proposals.filter((p) => p.origin === "ai").length;
    expect(await ai(UNLABELLED)).toBe(1);
    expect(await ai(FITTED)).toBe(1);
    const report = await api.removeAllAiData();
    expect(report.generations_removed).toBe(2);
    expect(await ai(UNLABELLED)).toBe(0);
    expect(await ai(FITTED)).toBe(0);
    // The scan's proposal and the accepted calendar stay.
    expect((await api.courseCalendar(UNLABELLED)).proposals.map((p) => p.origin)).toEqual(["scan"]);
    expect((await api.courseCalendar(LEGACY)).status).toBe("accepted_stale");
    // Set up again, the course's next cloud reading asks question (b) again.
    await api.addModelProvider("openai", null, "sk-demo-000000000000007Qx2");
    const [facts] = await api.modelDisclosure({ kind: "provider", provider_id: "openai" }, false);
    await api.acknowledgeModelDisclosure(
      { kind: "provider", provider_id: "openai" },
      false,
      facts.version,
    );
    await api.setFeatureModel("course_calendar", {
      backend: { kind: "provider", provider_id: "openai" },
      model: "gpt-6-luna",
      effort: "lowest",
    });
    const again: GenEvent[] = [];
    await api.readCourseCalendar(UNLABELLED, "gen-2", options, (e) => again.push(e));
    expect(kinds(again)).toContain("material_sharing_reminder");
  });

  it("forgets them also when a key stays in the keychain", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-keychain-locked" });
    await api.readCourseCalendar(DEMO101, "gen-1", options, () => {});
    expect((await api.courseCalendar(DEMO101)).proposals).toHaveLength(1);
    await expect(api.removeAllAiData()).rejects.toMatchObject({
      kind: "internal",
      message: expect.stringContaining("and 1 generated text(s)"),
    });
    expect((await api.courseCalendar(DEMO101)).proposals).toEqual([]);
  });

  it("says it after a reading the provider failed, before it ends", async () => {
    // The "ai-errors" endpoint fails every request, a reading's too, once it started.
    const api = createMockApi({ ...fast, scenario: "ai-errors" });
    // Its model has no price: accepted first, so the run starts.
    const choice = (await api.aiStatus()).features.find((f) => f.feature === "course_calendar");
    if (!choice?.choice) throw new Error("no model for reading calendars");
    await api.acknowledgeUnpricedModel(choice.choice.backend, choice.choice.model);
    const events: GenEvent[] = [];
    await expect(
      api.readCourseCalendar(DEMO101, "gen-1", options, (e) => events.push(e)),
    ).rejects.toMatchObject({ kind: "model", model_error: "rate_limited" });
    expect(kinds(events).slice(0, 2)).toEqual(["stage", "started"]);
    expect(kinds(events).slice(-2)).toEqual(["material_sharing_reminder", "finished"]);
    expect(events.at(-1)).toEqual({ type: "finished", ok: false });
    expect((await api.courseCalendar(DEMO101)).proposals).toEqual([]);
  });

  it("follows the answer stored when the reading ends, like the facade", async () => {
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    // Answered "allowed" while the reading runs: nothing to remind of.
    const events: GenEvent[] = [];
    const run = api.readCourseCalendar(UNLABELLED, "gen-1", options, (e) => events.push(e));
    await api.setCourseMaterialSharing(UNLABELLED, "allowed");
    const proposal = await run;
    expect(kinds(events)).not.toContain("material_sharing_reminder");
    expect(proposal.sharing_reminder).toBe(false);
    // Back to "not sure" while another runs: reminded then, once.
    const again: GenEvent[] = [];
    const second = api.readCourseCalendar(UNLABELLED, "gen-2", options, (e) => again.push(e));
    await api.setCourseMaterialSharing(UNLABELLED, "not_sure");
    expect((await second).sharing_reminder).toBe(true);
    expect(kinds(again)).toContain("material_sharing_reminder");
  });

  it("says it in a batch inside the course's own events", async () => {
    const api = createMockApi(fast);
    const said: string[] = [];
    await api.readCourseCalendars([UNLABELLED, FITTED], "batch-1", options, (e) => {
      if (e.type === "gen" && e.event.type === "notice") said.push(e.course_id);
    });
    expect(said).toContain(UNLABELLED);
    expect(new Set(said).size).toBe(said.length);
  });
});
