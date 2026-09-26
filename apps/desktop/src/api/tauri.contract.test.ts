// IPC contract, TypeScript half. Calls every WeekmarkApi method of the real Tauri client with
// a mocked IPC and records exactly what would cross the boundary (command + arguments) in
// src-tauri/tests/fixtures/ipc-calls.json. The Rust half (src-tauri/tests/ipc_contract.rs)
// replays that file against the real commands, so a renamed argument or an unregistered
// command fails a test on one side or the other.
//
// After changing tauri.ts on purpose: `pnpm exec vitest run -u src/api/tauri.contract.test.ts`
// and commit the updated fixture.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { createTauriApi } from "./tauri";

afterEach(() => clearMocks());

const COURSE = "folder:contract-test/course/DEMO101";
const SOURCE = "folder:contract-test";
// Values the facade rejects before any network access, so the Rust replay stays offline and
// deterministic (the contract is about names and shapes, not about succeeding).
const OFFLINE_CANVAS_URL = "contract-test-not-a-url";
const OFFLINE_FEED_URL = "http://calendar.example.edu/feed.ics"; // plain http → invalid

it("sends the commands and arguments the Rust side expects", async () => {
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    // Plugin calls (folder picker, opener) aren't ours; the contract covers our commands.
    if (!cmd.startsWith("plugin:")) calls.push({ cmd, args });
    return null;
  });
  const api = createTauriApi();
  const onEvent = () => {};

  await api.status();
  await api.listSources();
  await api.addCanvasSource(OFFLINE_CANVAS_URL, "contract-test-token");
  await api.addFolderSource("/tmp/contract-test-courses", "2026-09-08", "Courses");
  await api.addFolderSource("/tmp/contract-test-courses");
  await api.addIcalSource(OFFLINE_FEED_URL, null);
  await api.updateSourceSecret(SOURCE, "contract-test-secret");
  await api.removeSource(SOURCE);
  await api.syncAll({}, onEvent);
  await api.syncSource(SOURCE, { download_files: false }, onEvent);
  await api.downloadCourseFiles(COURSE, onEvent);
  await api.listCourses();
  await api.courseOverview(COURSE);
  await api.weekMaterials(COURSE, 3);
  await api.weekMaterials(COURSE);
  await api.listDeadlines(null, 7, 0);
  await api.listDeadlines(COURSE, 21, 7);
  await api.search("sampling", null, 20);
  await api.latestStudyPlan();
  await api.setCoursePolicy(COURSE, "learning_aid", "Syllabus §5");
  await api.setCourseTerm(COURSE, "2026-09-08", null);
  await api.setCourseAiAccess(COURSE, false);
  await api.setCourseHidden(COURSE, true);
  await api.mcpClientConfigs();
  await api.revealDataDir();

  // Channels serialise as "__CHANNEL__:<callback id>"; the id is irrelevant to the contract.
  const json = JSON.stringify(calls, null, 2).replace(/"__CHANNEL__:\d+"/g, '"__CHANNEL__:0"');
  await expect(`${json}\n`).toMatchFileSnapshot("../../src-tauri/tests/fixtures/ipc-calls.json");
});
