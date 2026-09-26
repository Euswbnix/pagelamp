# PageLamp design tokens

One token source for the native Mac app (SwiftUI) and the Tauri app (Windows, Linux, legacy macOS).
The values are those of the design spec, `docs/design/macos-shell.md` §10. The files use the
[W3C DTCG 2025.10](https://www.designtokens.org/TR/2025.10/) format (tokens and
[resolver](https://www.designtokens.org/TR/2025.10/resolver/)).

```text
design/tokens/
├── pagelamp.resolver.json          sets + modifiers + resolutionOrder (DTCG resolver)
├── pagelamp.tokens.json            base set: light · normal contrast · macOS · no backdrop · transparency auto
├── theme/dark.tokens.json          modifier theme = dark
├── platform/{windows,linux}.tokens.json
├── backdrop/mica.tokens.json       Windows 11 Mica window
├── contrast/more.tokens.json       Increase Contrast
├── transparency/reduced.tokens.json
├── gen-tokens.mjs                  generator (Node ≥ 24, node: built-ins only)
└── test/                           node --test suite + SwiftUI spring fixture
```

## Pipeline

```sh
node design/tokens/gen-tokens.mjs           # write the three generated files
node design/tokens/gen-tokens.mjs --check   # CI: exit 1 if any is missing or stale
node --test design/tokens/test/*.test.mjs   # tests (the swiftc test is skipped without Xcode)
```

| Output | What it holds |
|---|---|
| `apps/macos/Sources/PageLamp/Generated/PLTokens.swift` | `PLColor`, `PLRadius`, `PLSpace`, `PLLayout`, `PLSize`, `PLType`, `PLMotion` |
| `apps/desktop/src/styles/tokens.css` | `--pl-*` custom properties for every resolver context (not imported by the app yet) |
| `apps/desktop/src/styles/glass.css` | `.pl-glass`, `.pl-glass-interactive`, `.pl-glass-prominent`, `.pl-lamp-band`, `.pl-accessory`, `.pl-concentric`, `.pl-prose` (spec §8, §10.5) |

Never edit the outputs by hand. Change the JSON, run the generator and commit the JSON and the outputs together.
The generated CSS already follows `apps/desktop/biome.json`, so `pnpm lint` passes on it.

## Resolver and modifiers

DTCG modifiers are orthogonal: a context cannot depend on another modifier. The increased-contrast
colours differ between light and dark, so they live in a private `palette.ic.*` group. The base set
holds the light values and `theme/dark` overrides them, while `contrast/more` only sets aliases
(`"color.text.primary": "{palette.ic.text.primary}"`). The Mica glass tint works the same way
(`palette.mica.*`). Groups marked `dev.pagelamp.private` are never emitted.

`resolutionOrder` is base → theme → platform → backdrop → contrast → transparency. Later entries
win, so increased contrast and reduced transparency override the Mica tint.

| Modifier | Contexts (default first) | Tauri (`<html>`) | Mac |
|---|---|---|---|
| theme | light, dark | `.dark` class | aqua / darkAqua |
| platform | macos, windows, linux | `data-platform` | always macos |
| backdrop | none, mica | `data-backdrop="mica"` | always none |
| contrast | normal, more | `data-contrast="more"` **or** `@media (prefers-contrast: more)` | accessibilityHighContrast(Dark)Aqua |
| transparency | auto, reduced | `data-transparency="reduced"` **or** `@media (prefers-reduced-transparency: reduce)` | handled in views (§7.2) |

## `$value` is the web value; `$extensions["dev.pagelamp"]` handles the Mac

| Key | Meaning |
|---|---|
| `macos` | Native Mac value, when it differs from `$value`: a number (pt) for dimensions, `{ textStyle, size, lineHeight, weight, design? }` for typography, `{ design }` for a font design, `{ lineSpacing }` for the CJK paragraph, or `{ "opacity": "separate" }` (opaque colour plus `…OpacityLight/Dark`). `"system"`: the Mac uses the platform's own value (system colours, radii, glass), so no Swift is emitted. `"asset"`: the `AccentColor` asset. |
| `swift` | `{ enum?, name? }` to rename a Swift member (e.g. `color.status.success` → `PLColor.success`), or `false` |
| `css` | `false` to skip CSS (review limits such as `glass.budget`) |
| `cssUnit` | `"%"` prints a 0…1 number as a percentage (`glass.prominent.accent-mix`) |
| `spring` | `{ preset, duration, bounce, stiffness, damping, settleMs }` for a `transition` token |
| `curve` | `easeOut` / `easeInOut` for a non-spring `transition` token |
| `private` | Group only: not emitted (the `palette`) |

`css`, `macos` and `private` set on a group apply to every token in it.

## What the generator checks (and fails on)

- Every colour's `hex` matches its OKLCH value within 1/255 per channel. The conversion follows CSS Color 4 (OKLab → linear sRGB), and a colour outside sRGB gets Display P3 in Swift.
- Springs: `duration` and `bounce` are the source of truth, and `bounce` must equal the SwiftUI preset's (`.smooth` 0, `.snappy` 0.15, `.bouncy` 0.3). The stored `stiffness = (2π/d)²`, `damping = 4π(1 − bounce)/d` (2 decimals) and `settleMs` must match the formula. `$value.duration` must equal `settleMs`. `$value.timingFunction` is a fitted cubic-bezier fallback for other DTCG tools and must stay within 0.08 of the spring. CSS uses a 25-point `linear()` sampled from the spring itself.
- Aliases resolve, have the right type and are not circular. Every permutation has exactly the base set's tokens, because contexts may only override.
- **The CSS cascade reproduces every permutation.** Each permutation becomes one `:root…` block. Blocks with more selectors come later and are more specific, and each holds only the declarations that the blocks before it would get wrong. Contexts with a media query are copied into `@media`, with the attribute selector replaced by `:root` so the specificity stays the same. The generator then simulates all 48 attribute states × 4 media states against the resolver.
- Swift: nothing named `Glass` (it would shadow `SwiftUI.Glass`) and no duplicate member. `glass.css` may only use variables that `tokens.css` defines.

## Swift conventions

- `PLColor` is built from `NSColor(name:dynamicProvider:)` and picks light, dark, increased-contrast light or increased-contrast dark through `bestMatch(from:)`. The high-contrast names come first in that list, so no asset catalog is needed.
- Everything is `nonisolated`, and the colour provider is a `@Sendable` closure. AppKit may resolve colours off the main thread, and under MainActor default isolation a closure inferred as MainActor-isolated traps there (verified).
- `PLType.largeTitle.font` is `.system(.largeTitle, design:, weight:)`, so it follows the HIG text style. `size` and `lineHeight` are for layout maths only.
- `PLMotion.calm` and the others are SwiftUI presets, e.g. `.smooth(duration: 0.45)`. `PLRadius.concentric(outer:padding:)` implements `max(innerMin, outer − padding)`.

## Differences from spec §10

- **Location and names:** `design/tokens/` instead of `apps/desktop/tokens/`. The CSS goes to `src/styles/` and the Swift to `Sources/PageLamp/Generated/`, and Swift uses top-level `PLColor`, `PLType` and so on instead of `PLTokens.Color`. Both follow the M1 task brief.
- **Settle times** are 717 / 446 / 869 / 557 / 398 / 637 ms instead of the table's 716 / 445 / 868 / 556 / 398 / 636. The generator uses a single definition: the first whole millisecond from which |x − 1| < 5 × 10⁻⁴ holds for good, sampled every 0.1 ms. The spec's numbers mix floor and ceil.
- **CSS variable names** follow the token paths (`--pl-color-glass-tint`, `--pl-glass-regular-shadow`), not the §8 sketch's `--pl-glass-tint`.
- **No brand modifier yet.** A DTCG modifier needs at least two contexts and there is one brand, so the brand accent is `color.accent`. Add `brand/<id>.tokens.json` and a `brand` modifier when a second brand exists.
- `radius.window` has no token (always the system's). `glass.clear` is not a token (never used).

## Adding or changing a token

1. Put the light/macOS/web value in `pagelamp.tokens.json` (colours with `hex`, springs with every `spring` field).
2. Override it in the context files that change it. For a contrast colour, add `palette.ic.*` in the base and dark files and alias it from `contrast/more`.
3. Add `dev.pagelamp.macos` if the Mac differs, or `"system"` if the Mac uses the platform's own value.
4. Run the generator. If a derived number is wrong, its error message gives the correct value.
