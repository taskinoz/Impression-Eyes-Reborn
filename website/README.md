# ime-reborn website

Static Astro website for Impression Eyes Reborn.

```powershell
bun install
bun run dev
bun run build
```

The production site is configured for `https://ime-reborn.com/`. Tagged
application releases are downloaded from the repository's `releases/latest`
URL.

Pushes to `main` or `master` deploy the static build over SSH. Configure
these GitHub Actions repository secrets:

- `SSH_HOST`: hosting SSH hostname
- `SSH_PORT`: SSH port (optional; defaults to `22`)
- `SSH_USER`: restricted deployment user
- `SSH_KEY`: private half of a dedicated SSH deployment key
- `SSH_KNOWN_HOSTS`: pinned host-key line produced by `ssh-keyscan` and
  verified against the fingerprint supplied by the host
- `SSH_DEPLOY_PATH`: optional override for the add-on domain's document root;
  defaults to `public_html/ime-reborn.com` and must contain
  `ime-reborn.com`

Point the add-on domain at that same document root. The workflow synchronizes
the generated `website/dist/` contents and removes obsolete files, so do not
use a shared `public_html` directory.

Before publishing, enable GitHub Sponsors for `taskinoz` or replace the
`support` URL in `src/pages/index.astro` with the preferred contribution page.
