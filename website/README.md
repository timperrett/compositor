# Compositor marketing site

A framework-free, single-page static site for Compositor. It uses Tailwind's
standalone CLI and builds to `dist/`, ready for GitHub Pages.

## Local development

No Node.js, npm, package manager, or JavaScript build tool is required. You
need `make`, `curl`, and a supported macOS or Linux architecture.

```bash
make site-bootstrap
make site-build
```

`site-bootstrap` fetches the pinned Tailwind standalone binary into
`website/.tools/`, which is ignored by Git. `site-build` uses that local binary,
copies the HTML and authored progressive-enhancement JavaScript, and compiles
the CSS. The authored `src/main.js` has no third-party dependency and is copied
to `dist/` unchanged.

## Build

```bash
make site-build
```

The build copies the page and progressive-enhancement JavaScript to `dist/` and
compiles `src/input.css` into `dist/assets/styles.css`.

## GitHub Pages

The repository workflow at `../.github/workflows/deploy-pages.yml` runs
`make site-build` and publishes `website/dist/` when changes reach `main`. In
the repository's GitHub settings, open **Pages**, set **Source** to **GitHub
Actions**, then push the workflow and this directory to `main`.
