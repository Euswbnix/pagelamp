# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/)
(0.x: anything may change between minor versions).

## [Unreleased]

What will be the first pre-release of v0.3.

### Added
- Automatic updates: while it is open, PageLamp looks for a new version on GitHub once a day and
  shows a notice when there is one. It always asks before installing (*Install and restart*), and
  it installs only an update whose signature it has checked. An update isn't installed while a
  sync is running. The check sends nothing about your courses; GitHub sees your IP address and
  your PageLamp version, as with any download (see [PRIVACY.md](PRIVACY.md)). A copy installed
  from the `.deb` or `.rpm` gets a link to the new version's release page instead.
- *Settings → Updates*: the version you run, a switch for the daily check, *Check now*, and the
  update channel (Stable or Beta; a pre-release like this one starts on Beta). The first-run
  setup (or, after an upgrade, *What's new*) explains the check and has the same switch;
  PageLamp doesn't check on its own before you have seen it.
- Automatic sync: while the PageLamp app is running it syncs by itself, twice a day unless you
  choose once a day or off (in the app, or `pagelamp sync --auto <off|daily|twice-daily>`). It
  never downloads files. With nobody at the app it checks your token and asks Canvas only for
  your course list (with each course's syllabus), deadlines and announcements; modules, pages,
  the file list and assignments are read when you start a sync, or when you open PageLamp, or
  come back to its window and click, type or scroll there, and the last full sync is old enough
  (or a newly found course is waiting for its first full sync). Closing *What's new* or changing
  this setting counts as being at the app too. Canvas may record such a full sync as your
  activity in each course, as when you press Sync. A course found in between is listed at once
  and says that its materials haven't been read yet. After you stop a sync, nothing starts by
  itself for an hour. A sync PageLamp starts by itself says so in the `User-Agent` it sends to
  Canvas.
- Course weeks that follow the real teaching dates: a Canvas term that is really an enrollment
  window (for example a "Fall" term that runs from May to January) is no longer used to count
  weeks. PageLamp uses the dates you set first; then the Canvas course's own dates or the Canvas
  term (for a course folder, the folder's dates), when they fit a teaching term; and otherwise the
  week numbers in the materials your instructor posts. Each course shows whether it hasn't
  started, which teaching week it is in, or that it is in exams or has ended.
- "Where this course is" on a course's **Timeline** tab: the week or phase and how sure PageLamp
  is, the dates it used and where they come from, the dates it didn't use and why, and the
  course's status.
- Past courses: finished courses, and courses that have no dates and nothing new for about four
  months, move to a collapsed "Past courses" group in the app ("Past" in `pagelamp courses`),
  and courses that haven't started are listed under "Upcoming". Past courses keep their
  deadlines in the list; "I'm still taking this" moves a course back.
- Canvas: PageLamp reads the page a course's Home shows, and the pages of the same course that
  the Home page, the syllabus, other pages and announcements link to (one step, within limits).
  Files they link to are listed, not downloaded. This is how notes and files are found in a
  course that hides its Pages or Files list; PageLamp still never asks for either of these lists
  when the course hides it. (It asks for a course's modules, assignments and announcements
  whatever the course menu shows, as v0.1 did.) A page that no list of the course gives a change
  date for is read again by every sync you start, and by an automatic sync at most once a day;
  Canvas may record each read as a view.
- What PageLamp didn't read of a Canvas course is recorded with a reason. `pagelamp sync` prints
  a line per course with what it found through links and how many things it couldn't read; your
  AI app gets the list with the reasons and is told not to guess at it.
- In the app, after a sync the row of a Canvas source has a line for each course that doesn't
  show its Pages or Files list in Canvas, or where pages and files were found through links;
  the course's page says in its header which lists aren't shown.
- In the app, a Canvas course's page lists what PageLamp didn't read of the course and why
  ("What PageLamp didn't read", at the end of This week): what waits for you, what couldn't be
  read last time, what PageLamp can't read, and what it never reads. When a sync left something
  unread that went wrong or waits for you, the course's line on the sync row leads there.
- MCP: a new read-only tool, `list_materials`, lists every material of a course, 50 at a time.
- Stop a running sync or file download: *Stop* in the sync capsule's details, in *Sources & sync*
  and in the first-sync step. It stops at the next file, course or download; what was synced so
  far is kept, and the source isn't marked as failed. A sync started from the command line can't
  be stopped from the app.
- Files are read in a separate, resource-limited process, so one bad PDF can't stop a sync; the
  diagnostic report counts the files that can't be read, by reason. *Settings → Help & feedback*
  shows the same counts and says when this file reader can't start.
- A backup of the database before an update changes its format: `pagelamp.db.v2.bak`, next to
  the database. Only the newest backup is kept, and it is deleted when you remove your last
  source.
- When your data was saved by a newer PageLamp, the start screen says so and offers *Check for
  updates*; your data isn't changed.
- A new look for the desktop app: course pages read like paper with thin dividers, the toolbar
  turns to glass as you scroll, sync status sits in a small capsule, and a warm band marks this
  week. On Windows 11 22H2 and later the window uses Mica; "Reduce transparency" in Settings
  makes every surface solid, and on Linux "Increase contrast" gives darker text and borders. On
  Windows and Linux the typeface is now Inter; macOS uses the system font.
- Command line: `pagelamp course timeline <course>` prints where a course is, with the dates used
  and not used and why. `pagelamp course keep <course>` is "I'm still taking this" (`--until
  <date>`, or `--clear` to undo). `pagelamp courses -v` lists the reasons under each course.
  `pagelamp status` shows the automatic sync setting. `pagelamp doctor` gains lines for the file
  reader, the files that can't be read and the last database update, and the diagnostic report
  lists the result of the last update check (time, channel and a result code; no addresses).
- Releases: GitHub attests every file listed in `SHA256SUMS`; check one with
  `gh attestation verify <file> --repo Euswbnix/pagelamp` (see [SECURITY.md](SECURITY.md)).
- Releases: every installer and command-line archive carries `THIRD-PARTY-NOTICES.txt`, the
  licences of the software and data from other projects that PageLamp contains.

### Changed
- Canvas: a page that a module asks you to view is no longer read until you have viewed it in
  Canvas; as far as we can tell, reading it for you could mark it as viewed. Such pages are
  listed as not read. In two cases PageLamp can't know beforehand: when it reads a course's Home
  page for the first time, or again after the course got another Home page; and when a link uses
  another address of such a page. If it opened one, the sync says so.
- Canvas: a sync asks for a course's announcements of the last 365 days (was 120). Older ones
  PageLamp already has are kept, as before.
- A Canvas sync of only some courses (`pagelamp sync --course`, or downloading one course's
  files) no longer counts as a sync of the whole source: "Last synced", `pagelamp status` (now
  "Last full sync") and `sync_status` keep the time of the last full sync.
- Canvas: the request that lists your courses also asks whether each course is concluded, and
  PageLamp keeps each course's time zone and the dates Canvas reports for it.
- On a Canvas site whose session codes PageLamp knows how to read (one site so far), it takes the
  session code at the end of a course's code or name (for example 20269) as a rough range of
  months for the course. The range is only a bound: it limits which dates are accepted, and it
  helps tell a course that hasn't started from one that is long over. It never counts weeks, and
  on its own it never marks a course as ended.
- The dates form on a course's **Timeline** tab is now "Course dates": the first and last day of
  classes, filled in with the dates PageLamp is using. The three weeks after the last day count
  as the exam period, and your dates come before every other source.
- In the exam period a course has no teaching week: its **This week** tab and the MCP tool
  `week_materials` show the materials of the last 14 days and say why.
- A course file whose text can't be read now says why, in the app's language: no text found (for
  example a scanned PDF), too large, password-protected, damaged or an old format, or it hit the
  file reader's time or memory limit; the week's count of files your AI app can read leaves them
  out.
- Sync steps are shown in the app's language. A source's raw error text and its sync warnings sit
  behind "Technical details", which you can copy.
- In the app, a Canvas file you can still download is labelled "Not downloaded yet" (was "Not
  downloaded"); locked and too-large files keep their own labels.
- MCP: tools declare all four annotation hints (read-only, destructive, idempotent, open-world).
- MCP: `course_overview` and `week_materials` report `course.term_start`/`term_end` as the dates
  that count weeks (the first and last day of classes), or null when none are known — never an
  enrollment-window term — with the new `course.term_dates_source`. `week_materials` gains
  `phase`; `course_overview` gains `lifecycle`, and its timeline gains `phase`, `default_week`,
  the teaching dates (`teaching`) and their source (`anchor`, plus `anchor_origin` when the dates
  are yours; `calendar_status` is always `none` in this version). `list_courses` gains `phase`,
  `lifecycle` and `outside_term`; ended courses stay listed.
- MCP: `sync_status` says whether PageLamp syncs by itself (`auto_sync`,
  `last_automatic_sync_at`), gives each source a `state` (fresh, old, never synced, failed, or
  needs the student, or `materials_old` when only its deadlines and announcements are current)
  and, when it is later than the last full sync, when those were read (`deadlines_synced_at`;
  `list_courses` gives it for each course too). `course_overview`, `week_materials`,
  `list_materials` and `read_material` carry a "data as of" line; `list_courses`,
  `course_overview`, `week_materials` and `list_materials` say when a course's modules and
  materials haven't been read yet (`structure_pending`), and so does `search_materials` when a
  search in that one course finds nothing. The hints no longer suggest running `pagelamp sync`:
  an AI app must not sync for the student.
- MCP: `course_overview` names the course's Home page and syllabus (`home`, `syllabus`), counts
  its announcements and lists what PageLamp didn't read (`not_readable`, with reasons).
  `get_announcements` takes an `offset` and says which announcements it shows of how many.
  `read_material` says when a file isn't downloaded, and why; in `course_overview` and
  `week_materials` a file that can't be downloaded carries `download_blocked`.
- `pagelamp courses` groups courses into Current, Upcoming and Past; past courses (also in
  `--json`) are listed with `--past` or `--all`. `pagelamp course term` dates mean the first and
  last day of classes.
- Releases: one universal macOS disk image for Apple silicon and Intel; Windows ships only the
  per-user installer (no MSI). Each release also carries the files the updater uses
  (`latest.json` and signed update packages).
- Building from source needs Rust 1.90 or newer.

### Fixed
- A file that became locked or was moved, and hasn't been read again, no longer shows its old
  text anywhere: not in search and not to your AI app.
- A course that PageLamp lists as ended, inactive or not started no longer reports a current
  week. Without usable dates, the week number of the last material posted (even years ago)
  was shown as the current week in the app, `pagelamp courses` and to your AI app
  (`list_courses`, `course_overview`, `week_materials`); these now say "Ended", "Inactive" or
  "Starts …" instead. Asking for a week by its number still works, and "I'm still taking this"
  brings the week back. A course PageLamp can't place yet (no dates, but something happened
  in it recently) can still show a week from its latest week-numbered material, marked low
  confidence.
- Canvas dates are counted in the course's own time zone (v0.1 used UTC, so something posted
  late in the evening could count for the next day). "Today" comes from your computer's time
  zone setting, so the app and the `pagelamp mcp` your AI app starts agree on the week.
- In the app, a course, material or deadline title that contains template text such as
  `{{when}}` is shown as written; in v0.1 it could take the place of a date in the sentence
  around it.
- In the app's sidebar, "Synced … ago" keeps counting while PageLamp stays open, and an old sync
  no longer shows a check mark.
- On macOS, *Connect your AI app* no longer says that this build isn't notarized by Apple.
- Spinners fade instead of turning when your system asks for reduced motion, and long source
  names wrap instead of pushing the status off the card.

### Security
- Link addresses in Canvas pages, announcements and syllabuses can carry access parameters.
  PageLamp now removes them from the link addresses in the text it stores, whatever the text
  came from, and before it gives text out (search, your AI app). A link your instructor put in a
  module as an item of its own keeps its address as written; that address is cleaned when it is
  given to your AI app. Files you downloaded are not changed. Text stored earlier is cleaned the
  next time PageLamp, the `pagelamp` command or your AI app's connection opens your data.

### Upgrading from 0.1
- v0.1.0 doesn't update itself: download this version from the releases page and install it by
  hand. This version checks for updates itself (see *Added*).
- Quit your AI app before you install. Then open PageLamp once, and open your AI app again. An AI
  app that still runs the old `pagelamp` can't read the upgraded database and asks you to update
  PageLamp.
- Windows, if you installed v0.1.0 from the `.msi`: this version has no `.msi`. The `-setup.exe`
  is made to remove that installation first (Windows may ask for an administrator's permission)
  and installs for your account only, so `pagelamp.exe` is in another folder afterwards: copy
  the setup from *Connect your AI app* into your AI app again. Nobody has tried this path by
  hand yet.
- If your AI app starts a `pagelamp` you installed on its own (from an archive, or with
  `cargo`), replace that file with this version's too.
- The first start of this version (the app, the `pagelamp` command or your AI app's connection,
  whichever comes first) upgrades the database. It saves a copy as `pagelamp.db.v2.bak` next to
  it first; if the copy can't be written, the upgrade still goes ahead, and `pagelamp doctor`
  says whether a backup was made. v0.1.0 can't open the upgraded database.
- The same start cleans what v0.1 stored: it removes access parameters from the link addresses in
  the stored text, and it does that before it makes the backup copy; and it deletes the old text
  of files that became locked or were moved.
- Dates you set in v0.1 that match the synced term are cleared (they were the old form's
  prefill). The remaining ones are kept, and the course's Timeline tab asks you to check them
  once.
- The app shows *What's new* once. Automatic sync and the daily update check are on unless you
  turn them off there, and neither runs before you close it. When you close it, a full sync
  may start at once if your last one is 12 hours old or more.
- From this version on, a full sync reads more of each Canvas course than v0.1 did (the Home
  page and linked pages; see *Added*). In a course that hides its Files list, a page that hasn't
  changed is also read once more, to find the files it links to. Canvas may record these reads
  as views.
- Finished courses move to "Past courses", and the week shown for a course can change, because
  weeks are counted from other dates than in v0.1.

### Known limitations
- Reading weeks, other breaks and exam dates can't be set yet. After a break the week shown can
  run ahead, and the three weeks after the last day of classes count as the exam period.
- A past course that Canvas still lists keeps syncing, and a course can't be removed yet. To
  keep one away from your AI app, hide it (course → *Settings* → *Hide this course*).
- Automatic sync and the update check run only while the PageLamp app is open.
- Updating inside the app is new. Nobody has tried it by hand on Windows or with the Linux
  `.AppImage` yet, and nobody has installed the Linux packages and gone through a sync; they are
  built by the same automation as the other downloads. If an update fails, install the new
  version by hand from the releases page.
- Reading the Home page and linked pages can't be turned off: it is part of every full sync of a
  Canvas course. With automatic sync off, a full sync runs only when you start one.
- Keep the Beta update channel (the default) while you run a 0.3 pre-release: the Stable channel
  has no 0.3 release.
- Installer file names and the OS-level app version show `0.3.0` for every 0.3.0 pre-release;
  the real version is in *Settings → Updates*, *Settings → About* or `pagelamp --version`.
- As in 0.1: no reminders or notifications; Canvas access uses a personal token; the desktop app
  doesn't put `pagelamp` on your PATH on macOS and Windows; the Linux `.AppImage` can't serve
  your AI app (use the `.deb`/`.rpm`); Linux builds are not code-signed; older `.ppt`/`.doc`
  files and scanned PDFs aren't searchable; Windows and Linux builds are x86_64 only.

## [0.1.0] — 2026-09-28

First public release.

### Added
- Local course knowledge base (SQLite + full-text search) with text extraction for PDF, PowerPoint
  (.pptx), Word (.docx), Jupyter notebooks, Markdown, HTML, plain text and source code; citations point to page,
  slide, cell or section.
- Sources: course folder (one sub-folder per course, week folders, optional `course.toml`),
  calendar feed (iCal/webcal, e.g. Canvas Calendar Feed), and Canvas with a personal access token
  (read-only; personal use only; files downloaded only on request).
- "Which week is this course in" inference with confidence and evidence.
- MCP server (`pagelamp mcp`) with 9 read-only tools plus `save_study_plan` (saves only to your local database), and the
  `weekly_review`, `catch_up` and `study_plan` prompts, for Claude Desktop, ChatGPT desktop /
  Codex and Claude Code.
- Per-course AI policy and AI-access switch; material text is withheld for "No AI" courses.
- `pagelamp` CLI and the desktop app (onboarding, sources & sync, courses, course detail,
  "Connect your AI app", settings), in English and 简体中文, light and dark.
- Signed and notarized macOS builds: the app (`.dmg`) and the standalone `pagelamp` binary are
  signed with a Developer ID and notarized by Apple (the ticket is stapled to the `.dmg` and the
  app), so they open without the System Settings → Privacy & Security steps.
- Signed Windows builds: the installers (`-setup.exe` and `.msi`), the app with its `pagelamp.exe`,
  the uninstaller and the standalone `pagelamp.exe` are signed with Azure Artifact Signing and
  timestamped, so Windows shows a verified publisher instead of "Unknown publisher".

### Upgrading from an earlier build
- After installing a new version, quit your AI app completely and open it again so it starts the new
  `pagelamp` (on macOS, open PageLamp once first so macOS lets the new build run).

### Known limitations
- On Windows, SmartScreen may still warn about a new release until the signing certificate has
  built up reputation (see the README). Linux builds are not code-signed; check downloads against
  `SHA256SUMS`.
- Canvas can report a course's term much wider than its classes (University of Toronto's Fall term,
  for example, runs from May to January), so PageLamp may show the wrong week, such as "Week 22"
  in late September. Fix it in the course's **Timeline** tab (*Wrong week? Set this course's term
  dates*). v0.3 will work the week out from the syllabus, the schedule and the published notes.
- Courses from past terms keep syncing when their instructor never closed them in Canvas. Hide them
  (course → *Settings* → *Hide this course*): hidden courses stay hidden after each sync and are never
  shown to your AI app. Removing finished courses is planned for v0.3.
- No reminders/notifications yet (planned for v0.3).
- Canvas access uses a personal token until an institution-approved sign-in is available.
- On macOS and Windows the desktop app doesn't put `pagelamp` on your PATH (use the full path shown
  in *Connect your AI app*); the Linux `.AppImage` can't serve your AI app — use the `.deb`/`.rpm`.
- Older `.ppt`/`.doc` files and scanned PDFs (no text layer) are listed but not searchable.
- Windows and Linux builds are x86_64 only.
- Text extraction runs in the sync process. PDFs are checked for decompression bombs and their image
  data is never decoded, but page content is still parsed in memory and LZW-only streams aren't
  measured, so a very dense or deliberately crafted PDF can use a lot of memory (an isolated
  extraction worker is planned for v0.2).
- Installer file names and the OS-level app version show `0.1.0` for every 0.1.0 beta; the real
  version is in *Settings → About* or `pagelamp --version`.
