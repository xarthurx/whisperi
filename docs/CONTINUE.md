# Continue

- Verify the `WINGET_CREATE_GITHUB_TOKEN` repository secret is configured as a GitHub classic PAT with `public_repo` scope before the next tagged release so Winget submissions can succeed.
- After the secret is configured, create and push the `v0.5.13` tag to exercise the full release -> GitHub Release -> Winget submission flow.
