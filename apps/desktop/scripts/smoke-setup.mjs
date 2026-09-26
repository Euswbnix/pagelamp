#!/usr/bin/env node
// Real-app smoke test with synthetic data, in one command:
//
//   pnpm run smoke            # (re)create the scratch data + course folder, then `pnpm tauri dev`
//   pnpm run smoke --no-launch  # only (re)create the scratch data and print the steps
//   pnpm run smoke:clean      # delete the scratch directory
//
// Everything lives in <OS temp dir>/studentos-smoke — never in the real StudentOS data folder:
//   home/     STUDENTOS_HOME for this run (database, file cache)
//   Courses/  a synthetic course folder: two DEMO courses with "Week 1".."Week 4" folders
// All content is made up (docs/ARCHITECTURE.md §3.7).

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(desktopDir, "../..");
const root = join(tmpdir(), "studentos-smoke");
const home = join(root, "home");
const courses = join(root, "Courses");
const args = new Set(process.argv.slice(2));

if (args.has("--clean")) {
  rmSync(root, { recursive: true, force: true });
  console.log(`Deleted ${root}`);
  process.exit(0);
}

// ---------------------------------------------------------------------------------------------
// Synthetic course folder
// ---------------------------------------------------------------------------------------------

const DAY = 24 * 60 * 60 * 1000;
const today = new Date();
today.setHours(12, 0, 0, 0);

/** Local calendar date `days` from today as YYYY-MM-DD. */
function isoDay(days) {
  const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() + days);
  const m = String(d.getMonth() + 1).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${String(d.getDate()).padStart(2, "0")}`;
}

/** A one-page PDF with real extractable text (offsets computed so the xref table is valid). */
function tinyPdf(lines) {
  const text = lines
    .map((line, i) => `BT /F1 14 Tf 72 ${720 - i * 22} Td (${line.replace(/[()\\]/g, "")}) Tj ET`)
    .join("\n");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    `<< /Length ${Buffer.byteLength(text)} >>\nstream\n${text}\nendstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  let body = "%PDF-1.4\n";
  const offsets = [];
  objects.forEach((obj, i) => {
    offsets.push(Buffer.byteLength(body));
    body += `${i + 1} 0 obj\n${obj}\nendobj\n`;
  });
  const xref = Buffer.byteLength(body);
  body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets) body += `${String(offset).padStart(10, "0")} 00000 n \n`;
  body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return body;
}

function write(path, content, publishedDaysAgo) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
  // published_at = file mtime, so each week's files look published in that week.
  const when = new Date(today.getTime() - publishedDaysAgo * DAY);
  utimesSync(path, when, when);
}

const COURSES = [
  {
    dir: "DEMO101 Intro to Demo Studies",
    name: "Intro to Demo Studies",
    termStartDaysAgo: 23, // → week 4 today
    weeks: [
      ["What Is a Demo?", "A demo is a small, made-up example used to show how something works."],
      ["Placeholder Data", "Placeholder data looks realistic but describes nothing real."],
      [
        "Measuring Nothing Carefully",
        "Careful measurement starts with deciding what not to count.",
      ],
      [
        "Sampling and Surveys",
        "A sampling frame lists everyone who could be asked a survey question.",
      ],
    ],
  },
  {
    dir: "DEMO205 Foundations of Sample Data",
    name: "Foundations of Sample Data",
    termStartDaysAgo: 24,
    weeks: [
      ["Tables", "Tables arrange values into rows and columns."],
      ["Columns", "Each column holds one kind of value."],
      ["Rows", "Each row describes one example."],
      ["Joining Tables", "A join matches rows from two tables by a shared key."],
    ],
  },
];

function createCourses() {
  rmSync(courses, { recursive: true, force: true });
  for (const course of COURSES) {
    const base = join(courses, course.dir);
    write(
      join(base, "course.toml"),
      `name = "${course.name}"\nterm_start = ${isoDay(-course.termStartDaysAgo)}\n`,
      course.termStartDaysAgo,
    );
    course.weeks.forEach(([topic, sentence], i) => {
      const week = i + 1;
      const folder = join(base, `Week ${week}`);
      const daysAgo = course.termStartDaysAgo - i * 7;
      write(
        join(folder, `Week ${week} notes.md`),
        `# Week ${week}: ${topic}\n\n${sentence}\n\n## Key ideas\n\n- ${topic} is this week's topic.\n- These notes are synthetic test data.\n`,
        daysAgo,
      );
      write(
        join(folder, "Reading questions.txt"),
        `Week ${week} reading questions\n1. In your own words, what is ${topic.toLowerCase()}?\n2. Give one made-up example.\n`,
        daysAgo,
      );
      if (week === 4) {
        write(
          join(folder, `Week ${week} slides.pdf`),
          tinyPdf([`Week ${week}: ${topic}`, sentence, "Synthetic test slides."]),
          daysAgo - 1,
        );
        // Unsupported on purpose: shows the "can't be read" status.
        write(join(folder, "Lecture recording.mp4"), "not a real video", daysAgo - 1);
      }
    });
  }
}

mkdirSync(home, { recursive: true });
createCourses();

// ---------------------------------------------------------------------------------------------
// Steps for the person at the window
// ---------------------------------------------------------------------------------------------

const cli = join(repoRoot, "target/debug/studentos");
const steps = `
StudentOS smoke test — synthetic data only
  data folder (STUDENTOS_HOME): ${home}
  course folder to add:         ${courses}

 1. The window opens on "Welcome to StudentOS" (if it opens on Courses instead, go to
    Sources & sync → Add source and continue at step 3).
    Expect: the AI disclosure and an unticked "I understand"; "Get started" explains that you
    must tick it first.
 2. Tick "I understand", click "Get started".
    Expect: "Where are your courses?" with "Course folder + calendar feed" selected.
 3. Paste this path into "Folder path" (or use "Choose folder…"):
      ${courses}
    Type "Smoke test courses" into "Name (optional)". Leave the calendar feed empty (don't
    enter a real feed or Canvas token in this test). Click "Add and continue".
    Expect: "First sync" runs, then "Your courses are ready" with Courses 2 and 20
    materials; "Lecture recording.mp4" produces no error (it's just unreadable).
 4. Click "Go to my courses".
    Expect: DEMO101 and DEMO205 cards, each "Week 4" with a confidence, "No upcoming deadlines",
    "N of M materials readable by your AI app" (the .mp4 files don't count as readable),
    policy "Not set". "This week" says nothing is due (no calendar feed in this test).
 5. Open DEMO101.
    Expect: "Data from Smoke test courses · synced …"; This week shows Week 4 with notes, questions,
    slides (readable, with sections) and the recording ("Can't be read (e.g. video)").
    No "Download files…" note (folder courses are local).
 6. Timeline tab.
    Expect: Week 4 and the backend's evidence (term start from course.toml).
 7. AI policy tab → switch "Let my AI app read this course's materials" OFF.
    Expect: note "Your AI app can still see deadlines and course structure, but not the
    materials." Back on Courses, DEMO101 reads "AI access to materials is off".
 8. Back in AI policy: choose "No AI", Save.
    Expect: the switch shows off and is greyed out, with the "You marked this course 'No AI'…"
    note; the card reads "Materials not shared (No AI course)". Set the policy back and the
    switch returns to its stored value.
 9. Connect your AI app.
    Expect: Claude Desktop first; its JSON snippet runs
      ${cli}
    with STUDENTOS_HOME set to the data folder above (because it isn't the default).
    ${existsSync(cli) ? "" : "(That binary isn't built yet; `cargo build -p studentos-cli` makes it — only needed to actually connect an AI app.)\n    "}Nothing needs to be pasted anywhere for this test.
10. Settings.
    Expect: Data folder = the data folder above; "Show folder" opens it in Finder; the privacy
    section says when you confirmed the disclosure. Switch to 简体中文 and dark mode and glance
    back through Courses and a course page.

Clean up afterwards (closes nothing; quit the window first):
  cd ${desktopDir} && pnpm run smoke:clean
`;
console.log(steps);

if (args.has("--no-launch")) process.exit(0);

// ---------------------------------------------------------------------------------------------
// Launch the real desktop app against the scratch data
// ---------------------------------------------------------------------------------------------

const env = {
  ...process.env,
  STUDENTOS_HOME: home,
  // cargo is often not on PATH in GUI-launched shells; rustup installs it here.
  PATH: `${join(homedir(), ".cargo/bin")}:${process.env.PATH}`,
};
const child = spawn("pnpm", ["tauri", "dev"], { cwd: desktopDir, env, stdio: "inherit" });
child.on("exit", (code) => process.exit(code ?? 0));
