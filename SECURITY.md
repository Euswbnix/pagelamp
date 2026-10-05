# Security policy

PageLamp handles students' course data and Canvas access tokens on their own computers, so we take
security reports seriously.

## Reporting a vulnerability

**Please don't open a public issue.** Use GitHub's private reporting instead:
[Security → Report a vulnerability](https://github.com/Euswbnix/pagelamp/security/advisories/new).
Include steps to reproduce with **synthetic** data only — never real tokens, calendar links or
course materials.

We aim to acknowledge reports within 7 days. Only the latest release is supported during the 0.x
series.

## In scope

- Anything that could leak a Canvas token or calendar-feed link (logs, database, MCP output, errors).
- Any way for course content (e.g. a malicious page or PDF) to make PageLamp write to Canvas, run
  commands, reach the network from the MCP server, or escape the `<course_material>` wrapper.
- Material text reaching an AI app for a course whose AI access is off or policy is "No AI".
- Crashes or resource exhaustion from malformed files during sync.

## Out of scope

What your AI app or AI provider does with the data you choose to show it, and vulnerabilities in
third-party AI apps.

## Verifying downloads

Every release on the [releases page](https://github.com/Euswbnix/pagelamp/releases) has a
`SHA256SUMS` file listing every file in it. From v0.3 on, GitHub also **attests** each file in that
list: a signed statement that this repository's Release workflow built it. v0.1.0 has checksums
and code signatures, but no attestations. If a check below fails for a file from our releases
page, don't run it, and tell us privately (see above).

**Checksums** (any file). Put `SHA256SUMS` next to the download, then:

- macOS and Linux: `shasum -a 256 -c SHA256SUMS --ignore-missing` (or
  `sha256sum -c SHA256SUMS --ignore-missing`); it prints `OK` for each file you have.
- Windows (PowerShell): `(Get-FileHash .\PageLamp_<version>_x64-setup.exe -Algorithm SHA256).Hash`
  must equal that file's line in `SHA256SUMS` (upper or lower case doesn't matter).

A matching checksum shows that you have the file the release lists; the attestation and the code
signatures show who built it.

**Attestations** (from v0.3), with the [GitHub CLI](https://cli.github.com/):

```sh
gh attestation verify PageLamp_<version>_universal.dmg --repo Euswbnix/pagelamp \
  --signer-workflow Euswbnix/pagelamp/.github/workflows/release.yml
```

This works for every file listed in `SHA256SUMS`, including the Linux packages, which aren't
code-signed.

**macOS.** The app, the `.dmg` and the command-line `pagelamp` are signed with a Developer ID of
team `CBU69BX7M8` and notarized by Apple; the ticket is stapled to the app and the `.dmg`.

```sh
codesign --verify --deep --strict --verbose=2 /Applications/PageLamp.app
codesign --display --verbose=2 /Applications/PageLamp.app 2>&1 | grep TeamIdentifier   # TeamIdentifier=CBU69BX7M8
spctl --assess --type execute -vv /Applications/PageLamp.app      # source=Notarized Developer ID
spctl --assess --type open --context context:primary-signature -vv PageLamp_<version>_universal.dmg
xcrun stapler validate /Applications/PageLamp.app                  # needs the Xcode command line tools
codesign --verify --strict --verbose=2 ./pagelamp                  # the command-line tool
codesign --display --verbose=2 ./pagelamp 2>&1 | grep TeamIdentifier
```

A bare command-line binary can't carry a stapled ticket; macOS looks its notarization up online
the first time it runs.

**Windows.** The installers, the app and `pagelamp.exe` (in the installer and in the CLI zip) are
signed with Azure Artifact Signing and timestamped. In PowerShell:

```powershell
Get-AuthenticodeSignature .\PageLamp_<version>_x64-setup.exe |
  Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

`Status` must be `Valid`, with a timestamp, and the signer must be the same publisher for every
PageLamp release (the name Windows shows as the verified publisher when you run the installer).
The certificate chains to Microsoft's "Microsoft Identity Verification Root Certificate Authority
2020".

**Automatic updates** (from v0.3). The app installs an update only after checking its signature
against the updater public key built into the app, whichever channel it came from; a changed or
swapped file is refused. The stable and beta channels (`updates/stable.json` and
`updates/beta.json` on this repository's `gh-pages` branch, which the app reads from
`raw.githubusercontent.com`) point only at files of this repository's releases, and each update
file is also in that release's `SHA256SUMS` and attestation. `updates/test.json` on the same
branch serves rehearsal builds from `updates/test/`, which belong to no release and have no
`SHA256SUMS` or attestation; only rehearsal builds read it, never a released app.
