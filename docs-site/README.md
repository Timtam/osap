# Documentation site

A [Docusaurus](https://docusaurus.io) site for the OS Automation Platform. The
content lives in the repo's top-level [`../docs`](../docs) (single source of
truth, also referenced by the code); this project renders + **versions** it and
deploys to GitHub Pages.

## Develop

```bash
cd docs-site
npm install
npm start          # local dev server with live reload
```

## Build

```bash
npm run build      # static site → docs-site/build/
npm run serve      # preview the production build
```

## Versioning

The live `../docs` is the **`current`** version (the default shown). To freeze a
release as an archived, immutable snapshot:

```bash
npm run docusaurus docs:version 0.2.0
```

This copies `../docs` into `versioned_docs/version-0.2.0/`. Archived versions are
reachable via the version dropdown (top-right); `current` stays the default.

## Deploy (GitHub Pages)

`.github/workflows/deploy-docs.yml` builds + deploys on push to `main`/`master`.
Before the first deploy:

1. Push the repo to GitHub.
2. Set `organizationName` / `projectName` / `url` / `baseUrl` in
   `docusaurus.config.js` to the real repo (they are placeholders).
3. Enable **Settings → Pages → Source: GitHub Actions**.
