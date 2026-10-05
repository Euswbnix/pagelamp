# Privacy

*Last updated: 2026-10-18 · applies to PageLamp v0.1.0 (the latest stable release) and to
v0.3.0-alpha.1 (a pre-release). Items marked **(v0.3)** are true for the pre-release only; v0.1.0
doesn't have them.*

PageLamp is a local app. There is no PageLamp server, account, analytics or telemetry. The people
who build PageLamp never receive your data.

## What PageLamp stores, and where

| Data | Where | Why |
|---|---|---|
| Course list, syllabuses, modules, material titles and text, announcements, deadlines and other dated items from your Canvas planner (your own planner notes and calendar events included: title, date and link), your study plan, your per-course settings (AI policy, AI access, term dates, hidden, and **(v0.3)** that you are still taking a course PageLamp counts as past) | a SQLite database in your data folder (macOS `~/Library/Application Support/dev.PageLamp.PageLamp`, Windows `%APPDATA%\PageLamp\PageLamp\data`, Linux `$XDG_DATA_HOME/pagelamp`, or `PAGELAMP_HOME`) | so your AI app can answer questions about your courses |
| Your sources: the Canvas address and the name Canvas shows for your account, a course folder's full path, when each source last synced and its last error | the same database (and its backup); PageLamp doesn't give the name or the path to your AI app | to show "Connected as …" and each source's state |
| Files you ask PageLamp to download from Canvas | the `files/` folder next to the database | so their text can be indexed |
| Canvas access token, calendar-feed link | your operating system's keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service) — never in the database, logs or AI output | to sync on your behalf |
| **(v0.3)** Your update settings, the result of the last update check, and the version you last ran | the same database | to know when the next check is due and to show "What's new" once |
| **(v0.3)** Your automatic sync setting, and when PageLamp last synced or tried to | the same database | to know when the next sync is due |
| **(v0.3)** For each Canvas course, a list of what PageLamp didn't read and why: the title Canvas shows for the item (a tab, a module item, a page or a file), its link and a reason; and which pages and files the texts it read link to (addresses only, never a link's words) | the same database; deleted when you remove the source | to tell you and your AI app what isn't in PageLamp, and to avoid reading a page again just to follow its links |
| **(v0.3)** A backup of the database, made before an update changes its format | `pagelamp.db.v<N>.bak` next to the database (on macOS and Linux its permissions let only your account read it); only the newest is kept, and it is deleted when you remove your last source | so a failed update can be undone. It holds the same course data as the database |

PageLamp does **not** store assignment instructions or submissions — only assignment titles, due
dates and links; **(v0.3)** an assignment or a quiz can also appear, by title and link, in the
list of what wasn't read. Canvas sends each assignment's description along with the list of
assignments; PageLamp drops it and never stores it, and it never opens an assignment or a quiz. It
never reads your university password.

**(v0.3)** A link address in a Canvas page, announcement or syllabus can carry a parameter
(`verifier`, `sf_verifier`, `access_token`) that, as far as we can tell, opens a file for whoever
has the address. v0.1.0 stores such addresses as they are, and your AI app can read them with the
text. From v0.3 PageLamp removes these parameters from the link addresses in the text it stores,
and again from everything it gives your AI app. Two limits: a parameter whose name is written in
an encoded form can be missed, and a material's own link (for example a link your instructor put
in a module) is stored as it is and cleaned only on its way to your AI app. Text stored earlier
(materials, syllabuses, saved study plans) is cleaned once after the update: the app and the
`pagelamp` commands other than the MCP server don't open your data until that is done; the MCP
server starts in any case and cleans what it gives out. PageLamp also tries to clean it before it
makes the backup copy; if that try fails, the copy is still made and keeps the old addresses.

## What leaves your computer

- **Sync:** PageLamp connects to your LMS (read-only requests with your own token) and/or
  downloads your calendar feed. Course folders are read locally. A sync runs when you press Sync
  (or run `pagelamp sync`). **(v0.3)** What a sync reads of a Canvas course, and what Canvas may
  record of it, is in the [README](README.md#canvas-access-tokens).
- **(v0.3) Automatic sync:** while the PageLamp app is running, it also syncs by itself, twice a
  day unless you choose once a day or off (see below). A sync that fails is tried again later. It
  never downloads files. Course folders and calendar feeds are read in full by every automatic
  sync.
  - When nobody is at the app, PageLamp checks your token and asks Canvas only for your course
    list (with each course's syllabus), your deadlines and your courses' announcements. It makes
    no request for a course's modules, pages, file list or assignments. Canvas keeps its own
    records, and we can't promise that these requests leave none.
  - When you open PageLamp, come back to its window and click, type or scroll, close "What's new",
    or change this setting, and the last full sync is old enough or a newly found course hasn't
    been read yet, it runs the same sync as the Sync button. A window that only comes to the front
    doesn't count. Canvas may record a full sync as your activity in each course, as it would if
    you pressed Sync yourself.
  - PageLamp's MCP server gives your AI app no way to start a sync and tells it not to run one
    for you. PageLamp doesn't sync by itself while its app is closed.
- **When you ask your AI app a question:** your AI app (Claude, ChatGPT, Codex, …) reads the course
  information it needs from PageLamp on your computer and sends it to that AI provider **under your
  own account and that provider's terms**. What the provider stores or uses for training depends on
  your account settings with them — check them.
- **(v0.3) Update check:** once a day PageLamp downloads a small file from GitHub to see whether
  there's a new version. GitHub sees your IP address and your PageLamp version, as with any
  download; nothing about your courses is sent. The request carries only `User-Agent:
  PageLamp/<version>` and standard `Accept` headers: no cookies, and nothing about you or your
  computer in the address. PageLamp always asks before installing an update, which it then
  downloads from GitHub too. On Linux `.deb`/`.rpm` installs it only shows a download link. You can
  turn the daily check off (see below).
- **Downloading course files:** when you ask for a course's files, Canvas may send the download
  on to its file storage host; PageLamp follows it and doesn't send your token there.
- **Adding a source:** when you add a Canvas source or replace its token, PageLamp checks the
  token with Canvas right away; when you add a calendar feed or replace its link, it downloads the
  feed once.
- **A calendar feed that redirects:** PageLamp follows the redirect, to https addresses only, and
  so contacts the server the feed names.
- **Installing on Windows:** if your computer doesn't have Microsoft's WebView2 runtime (Windows
  11 comes with it), the installer is set to download it from Microsoft.
- Nothing else. The PageLamp app, the `pagelamp` command and the MCP server contact no other
  server.

## Your controls

- **Per course:** turn off *"Let my AI app read this course's materials"*, or mark the course's AI
  policy as "No AI" — PageLamp then shares no material text for that course (deadlines, structure
  and your study plan remain available for planning). CLI: `pagelamp course ai-access <course> off`.
- **Hide a course** to keep it out of your AI app entirely: `pagelamp course hide <course>`.
- **(v0.3) Automatic sync:** *Sources & sync → Automatic sync* (Off, Once a day, Twice a day).
  With it off, PageLamp syncs only when you press Sync. CLI: `pagelamp sync --auto off`.
- **(v0.3) Update checks:** *Settings → Updates → Check for updates automatically*. With it off,
  PageLamp never checks on its own; *Check now* still works.
- **Remove a source** (`pagelamp sources remove <id>` or *Sources & sync → Remove*) deletes what
  was synced from it — its courses with your AI-policy and term settings for them (course folder
  and Canvas), its deadlines and events (calendar feed), and for Canvas the course files you
  downloaded — plus its token or feed link in the keychain. Your own course folder and anything in
  Canvas or your LMS calendar are never changed.
- **Delete everything:** remove each source first (so its token or feed link is deleted from the
  keychain), then quit PageLamp and delete the data folder above. Uninstalling the app doesn't do
  this. The database, downloaded files, the backup, the logs and the list of course names stay
  until you delete the folder. Your Canvas token or feed link isn't in that folder: it stays in
  your system's keychain, under the service name `dev.pagelamp`, until you remove the source in
  PageLamp or delete the entry there yourself. The desktop app also keeps its display preferences
  (such as theme, language, transparency and contrast, which lists are open, and that the
  first-run setup is done) in its own app storage under `dev.pagelamp.desktop`; they contain no
  course data.

## AI disclosure

PageLamp does not generate content itself (v0.1.0 and v0.3.0-alpha.1). Text your AI app produces
is AI-generated by that app. PageLamp tells your AI app to cite course sources, tutor rather than produce graded work,
and respect each course's AI policy — but you are responsible for following your course and
university rules.

## Logs and diagnostic reports

PageLamp writes log files only on your computer, in the `logs/` folder of your data folder, and
keeps 7 days of them: each time it starts, it deletes the older ones. A short record of the last
crash (`logs/last-crash.json`: time, version, a redacted message and the place in the code) stays
until you dismiss the crash notice in the app or a later crash replaces it. Logs record what
PageLamp did (e.g. "synced 5 courses", request paths and status codes) — never tokens,
calendar-feed links, signed download links or course text; your home folder is shown as `~`. A
diagnostic report (*Copy diagnostic report* in the app, or `pagelamp report`) is created only when
you ask for it, shows you its full content first, and replaces course names with "Course 1",
"Course 2". From v0.3 it also lists the result of the last update check and of the last database
update and backup, as codes only, whether the file reader can run, and how many files couldn't be
read, by reason. Nothing is ever sent automatically — you decide whether to share a report, e.g.
in a GitHub issue.

To replace names of courses you have since renamed or removed, PageLamp keeps a small list of
course names, codes and folder names in `course-aliases.json` in your data folder (on macOS and
Linux its permissions let only your account read it; it is never included in a report). Each entry
is deleted 30 days after the course was last seen.

## Questions

Open an issue at https://github.com/Euswbnix/pagelamp/issues (don't include personal data), or see
[SECURITY.md](SECURITY.md) for security reports.
