# Documentation site

Static site for this extension's documentation, built with
[Docusaurus](https://docusaurus.io/) and deployed by
[`.github/workflows/DeployDocs.yml`](../.github/workflows/DeployDocs.yml) to GitHub Pages.

The pages under `docs/docs/` are the extension's user guide and development notes; their Chinese
translations live under `i18n/zh-Hans/docusaurus-plugin-content-docs/current/`.

## Layout

| Path | Description |
| --- | --- |
| `docs/intro.md` | Introduction. The only page with a `slug`, so `/docs/intro` stays stable. |
| `docs/getting-started/` | Installing the extension and producing the first reports. |
| `docs/guide/` | The two functions, the options, the price branch, where the reports go, the error paths. |
| `docs/development-guide/` | Everything a *contributor* needs, and deliberately the last top-level entry in the sidebar: `architecture/` (layout, design notes, dependencies), `build/` (build and release, testing), `publishing/` (function descriptions, community-extension registration), `demo-data/` (the snapshot the examples run on). |
| `i18n/zh-Hans/` | Simplified Chinese translations of all of the above, plus the UI strings. |
| `static/demo/` | The demo snapshot (`prices.csv`) that every example reads, served at `/demo/prices.csv`, plus a pre-generated report the home page shows in an iframe. |
| `src/pages/index.tsx` | Home page: hero, feature cards, the Rust/SQL showcase and the "where to go next" cards. The hero, the feature grid and the next-step cards are `<dfk-*>` custom elements from [`duckfn-docs-kit`](https://www.npmjs.com/package/duckfn-docs-kit), mounted through callback refs and fed with the imperative `translate()` API; every string still has an entry in `i18n/zh-Hans/code.json` under `homepage.*`. The code showcase stays here because it needs the theme's `CodeBlock`. |
| `src/pages/index.module.css` | The code showcase's styles. The hero, feature grid and cards carry their own styles inside the kit's shadow DOM, so they are not here. |
| `src/css/custom.css` | Palette and theme overrides. It pulls the kit's global CSS in with `@import 'duckfn-docs-kit/src/kit.css'` (the `--duckfn-*` brand tokens and the TOC-toggle styles); this file itself only owns the Infima ramp. |
| `static/` | Files copied to the site root (images, `.nojekyll`). |
| `sidebars.ts` | Sidebar definition. Categories come from `_category_.json`; order from `sidebar_position`. |
| `docusaurus.config.ts` | Site configuration: `REPO_URL`, `EXTENSION_NAME`, locales, navbar, footer, and the docs-kit plugins. |
| `extension-version.ts` | The version shown in the docs. The only place it is written; `{{EXTENSION_VERSION}}` in the markdown is replaced from here at build time. |
| `package.json` / `package-lock.json` | Dependencies. `package-lock.json` is committed: CI installs with `npm ci`. |

A new content directory has to end up in git. The repository's `.gitignore` used to hold a bare `build`,
which matches a directory of that name at **any** depth — that silently kept `docs/docs/development-guide/build/`
and its translation out of the commit, so the page did not exist on CI and `onBrokenLinks: throw` failed
the build over the footer's link to it. `git status` never shows ignored files; `git check-ignore -v <path>`
does. The rule is anchored now (`/build`); keep it that way.

## What to change first

1. `REPO_URL` and `EXTENSION_NAME` in `docusaurus.config.ts` — the navbar, the footer, the home page
   and the runnable blocks' extension preload all read them from there.
2. `tagline` in `docusaurus.config.ts`, plus the copyright line in the footer and in
   `i18n/zh-Hans/docusaurus-theme-classic/footer.json`.
3. The logo and palette: `static/img/logo.svg` is a placeholder, and the brand ramp in
   `src/css/custom.css` was picked to match it — replace both together, or neither.
4. `static/img/docusaurus-social-card.jpg` (the preview image) and `static/img/logo.svg`.
5. The pages under `docs/docs/` and their translations under
   `i18n/zh-Hans/docusaurus-plugin-content-docs/current/`.
6. Optional: search. See the commented `algolia` block in `docusaurus.config.ts`; DocSearch is free but
   needs an index (https://docsearch.algolia.com/apply).
7. One-time setup in the repository: Settings → Pages → Build and deployment → Source:
   **GitHub Actions**.

## What duckfn-docs-kit provides

The site is wired to [`duckfn-docs-kit`](https://www.npmjs.com/package/duckfn-docs-kit) instead of
carrying its own copies of the same glue. `docusaurus.config.ts` registers four entry points and
`src/css/custom.css` imports the fifth:

| Entry point | What it does here |
| --- | --- |
| `duckfn-docs-kit/remark` | `remarkVersionPlaceholder` — replaces `{{EXTENSION_VERSION}}` at build time. |
| `duckfn-docs-kit/sql/remark` | `remarkRunnableSql` — turns a runnable `sql` fence into a `<dfk-sql>` element. |
| `duckfn-docs-kit/sql/extensions` | `dfkExtensions` — preloads the extension and injects the `dfk-*` element registration. |
| `duckfn-docs-kit/toc-toggle/plugin` | `dfkTocToggle` — the TOC collapse control. |
| `duckfn-docs-kit/src/kit.css` | The `--duckfn-*` brand tokens plus the styles a shadow boundary cannot host. |
| `duckfn-docs-kit` (barrel) | The `<dfk-hero>` / `<dfk-features>` / `<dfk-next-steps>` home-page elements. |

Because the two plugins inject their own browser glue, the site keeps **no `src/clientModules/` files
of its own**, and there is no local `plugins/` directory either.

## Runnable SQL blocks

A fenced `sql` block whose info string is a JSON config turns into a live example: a CodeMirror
editor with a Run button, running in DuckDB-Wasm in the reader's browser.

````md
```sql {"type":"duckfn","show":"table"}
SELECT name, qs_html_reports_by_prices(symbol, date, price, NULL) FROM prices;
```
````

`"type":"duckfn"` is required; `show` is `table` (default), `text`, `html`, `iframe` or `svg`
(`iframe` and `html` take a `field` and a `tab_name`, and `option.height` is what keeps a full report
inside its own box), and a block that demonstrates a failure declares `"expect":"error"`. A plain
`sql` block without that info string stays an ordinary code block — use it for anything a reader is
meant to copy. The full config reference is in the kit's own guide at the path recorded in
`AGENTS.md` (`<duckfn crate>/docs/docs/docs-kit/runnable-sql.md`).

**Examples use the real snapshot, not generated data**: a runnable block reads
`https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv` (served from `static/demo/`), which
works in the browser — DuckDB-Wasm in the browser can fetch over HTTPS.

What `npm test` does, and what it cannot see:

- It runs each block in DuckDB-Wasm **in a real browser** (kit 0.3.0 and up): `duckfn-sql-verify` drives
  your system Chrome/Edge through `playwright-core` (`executablePath`, so no browser is downloaded) and
  serves the engine and the extension from local files over a loopback http server — the suite itself
  is offline, while a block reading `https://…` needs the network exactly as it does on the site. So
  the snapshot-reading blocks really run, and no manual click-through is needed. If no browser is
  detected, point `--browser <exe>` (or `DFK_BROWSER`) at one.
- **A wasm build writes no files** — `output_dir` is accepted and then ignored there: no error, no file,
  `file_path` NULL, the report still in the `html` column. The file operation is skipped rather than
  attempted, because that file system is not a faithful one: a path that does not exist comes back as a
  phantom one-byte entry, DuckDB's own `glob` / `read_text` / `file_size` report it as present, and a raw
  write offset is off by a byte — so "is this name free" has no trustworthy answer. The blocks that set
  `output_dir` are therefore ordinary `sql` blocks, with a note saying that only a native DuckDB writes
  the files.

## Preloaded extensions

The blocks call the extension, so the extension has to be there before the first Run. That is the
`dfkExtensions` plugin, configured in `docusaurus.config.ts`: on `npm start` / `npm run build` it
fetches the repository's **latest release** asset
`duckfn_quantstats-wasm_eh.duckdb_extension.wasm` into
`static/duckdb-extensions/duckfn_quantstats.duckdb_extension.wasm`. The file name must keep
`duckfn_quantstats` before the first dot — that base is the entry symbol DuckDB looks up, hence the
rename from the release asset, which carries the wasm suffix.

Two consequences worth knowing:

- **The release has to exist, and `REPO_URL` has to point at the right repository.** The plugin has
  nothing to fetch before this repository has cut at least one `v*.*.*` tag, and `npm run build`
  fails until then. Note that on a brand-new repository the **first** tag's Deploy Docs run can fail
  for the same reason: the release is created by the *other* workflow the tag triggers, in parallel.
  Re-run Deploy Docs once the release is up.
- Downloads are cached under `.cache/duckfn-docs-kit/` and only re-fetched when the release asset's
  sha256 changes; with a warm cache the build works offline. Both `.cache/` and
  `static/duckdb-extensions/` are gitignored — a file placed there by hand needs `git add -f`.

The extension is built by CI for DuckDB v1.5.5; the kit pins `@duckdb/duckdb-wasm` to the exact dev
build whose engine matches. When either side moves, re-check that the live blocks still run.

## Testing the examples

`npm test` runs every runnable block in `docs/` and in each locale through DuckDB-Wasm — in a headless
browser, on the very runtime the page uses — and fails if one breaks, so an example cannot rot
unnoticed:

```shell
npm test             # = duckfn-sql-verify --site . (needs the extension under static/duckdb-extensions/)
```

It preloads the same extension the page does and, like the page, runs one DuckDB instance per page
with the page's blocks sharing a connection. It is **not** wired into CI: the test needs the wasm
build of the extension, which only exists after a release, and the Deploy Docs workflow would race
the release it depends on. Run it locally after a release, or from a checkout that already has the
file under `static/duckdb-extensions/`.

## Commands

```shell
npm install          # once
npm start            # dev server at http://localhost:3000
npm start -- --locale zh-Hans   # dev server, Chinese
npm run build        # static site into build/
npm run serve        # preview the build
npm run typecheck    # tsc
npm test             # run the runnable SQL blocks (see above)
```

The `Justfile` wraps the first two (`just docs_build`, `just docs_start`) so the commands live next to
the extension's own.

`npm run build` is the check that matters: `onBrokenLinks` is set to `throw`, so a link to a page
that does not exist fails the build for both locales. The Deploy Docs workflow runs `npm run
typecheck` as well, which catches mistyped React props and type mismatches the build alone accepts.

## Markdown conventions

**Admonitions.** The opening directive goes on a line of its own and takes an optional title in
square brackets — a bare `:::note Title` does not render. The content always starts on the next line:

```md
::::note[Limitations]

- the first point
::::
```

Nesting works by using more colons for each level: `:::::info[Parent]` → `::::danger[Child]` →
`:::tip[Deep Child]`.

Two more things worth knowing: `onBrokenLinks` is `throw`, so every internal link and anchor has to
resolve (in both locales), and code fences should use one of the languages enabled for Prism in
`docusaurus.config.ts` — `bash`, `rust`, `sql` or `toml`.

**Version numbers.** Do not write a version by hand: write `{{EXTENSION_VERSION}}` (inside a code
fence or inline code, where the braces stay literal) and it is replaced at build time from
`extension-version.ts`. The value there is updated by `just release_bump`, so a release does not have
to touch the markdown at all.

## Translations

The site ships in English (`en`, default) and Simplified Chinese (`zh-Hans`). Routes are prefixed per
locale: `/docs/...` and `/zh-Hans/docs/...`.

A translated page is a full copy of its English source, placed under
`i18n/zh-Hans/docusaurus-plugin-content-docs/current/` with the same relative path:

- Translate the body and the reader-facing front matter (`title`, `description`).
- Keep `id`, `slug` and `sidebar_position` identical so both languages share routes and order.
- Link to other pages with **relative file paths** (`./project-structure.md`, `../guide/functions.md`).
  A hard-coded `/docs/...` link would send a Chinese page to the English one.
- Runnable blocks are copied verbatim except for the SQL comments: the `{"type":"duckfn",…}` info
  string and the query itself are code, and `npm test` runs the Chinese page's blocks too.

UI strings live in `i18n/zh-Hans/*.json`. After changing text in `docusaurus.config.ts`, in `src/`, or
a `_category_.json`, regenerate the stubs and fill in the new entries:

```shell
npx docusaurus write-translations --locale zh-Hans
```

`write-translations` keeps existing messages, so it only adds what is missing. Check afterwards that
the new entries it appends are translated, and that the manually written `homepage.*` entries in
`code.json` are still present — it warns about `homepage.tagline` because that one cannot be extracted
statically, which is expected.

Add another language by listing it in `i18n.locales` in `docusaurus.config.ts` and repeating the
steps above.

## Deployment

Pushing a version tag (`v*.*.*`) builds the site and publishes it to GitHub Pages; the same workflow
can be started by hand from the Actions tab.

`url` and `baseUrl` are not hard-coded — the workflow reads them from `actions/configure-pages` and
passes them to the build as `DOCS_URL` and `DOCS_BASE_URL`, which `docusaurus.config.ts` picks up.
Outside CI they fall back to `http://localhost:3000` and `/`.

One-time setup: in the repository settings, set **Pages → Build and deployment → Source** to
**GitHub Actions**.
