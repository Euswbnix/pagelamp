# Mac strings (`mac.*`)

The Mac app reads the same i18next JSON as the Tauri app (`apps/desktop/src/i18n/locales/{en,zh-CN}/*.json`,
keys `<namespace>.<path>`) plus the fragments in this folder. Every fragment is merged under `mac.`,
so a key `{"nav": {"thisWeek": …}}` becomes `mac.nav.thisWeek`, whichever file it is in.

- **Fragments:** `<name>.en.json` + `<name>.zh-CN.json`, both required, with the same keys and the same
  `{{placeholders}}` per key. `mac.*.json` holds the shared shell keys and `shell.*.json` the
  skeleton's own (errors, capsule notices, sync details, the Debug menu); screens add their own
  (`thisweek.*`, `course.*`, `setup.*`). A key defined in two fragments is an error.
- **What goes here:** Mac-only strings, HIG title-case labels ("Sync Now", "Replace Token…"), and the
  keys spec §3 marks *new*. A new key that belongs to a shared namespace mirrors that namespace under
  `mac.` (`mac.common.week.compact`, `mac.courses.thisWeek.next7`, `mac.course.aiStatus.notSet`), so
  it can move to the shared files unchanged, apart from the prefix, once Tauri needs it. zh-CN is the
  same text in both apps; never write fragments (`"Due"` + `"today"`), only whole sentences.
- **Placeholders:** `{{name}}` becomes `%N$@`, numbered by first appearance in English and identical
  in Chinese. `{{product}}` is replaced by the brand's `productName`. Plurals use i18next suffixes
  (`key_one`, `key_other`; Chinese keeps `_one` for parity but only `other` is used) and go to
  `Localizable.stringsdict` with `count` as `%N$ld`.
- **Regenerate** after any change (CI runs `--check`):

  ```sh
  node apps/macos/scripts/gen-strings.mjs           # → Sources/PageLamp/Resources/{en,zh-Hans}.lproj, Generated/L10nKeys.swift
  node --test apps/macos/scripts/test/*.test.mjs
  ```

- **In Swift** use the L10n helper, never literals. `L10nKeys.all` lists every key, `L10nKeys.arguments[key]`
  gives the argument names in order (pass Strings), and `L10nKeys.plural` marks stringsdict keys (pass
  `count` as an Int). A key with no arguments is plain text: don't run it through `String(format:)`.
