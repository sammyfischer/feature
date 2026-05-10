# Release steps

1. Pull latest changes into main.
2. Branch from main, name the branch "release-vx.x.x" where "x.x.x" is the new version being created (not the one currently in `Cargo.toml`).
3. Bump version in `Cargo.toml`.
4. `just release`
   - if there are errors (including lints) resolve them and run again
   - if there are nontrivial errors, resolve those on a new feature branch and restart this process
5. Commit the following changes:
   - formatting
   - lint/error resolutions
   - the version in `Cargo.toml`
   - the version in `Cargo.lock`
   - files in `resources/` if they changed
6. Push and merge the release branch.
7. Switch to main and pull again.
8. `just tag`
9. `git push --tags origin`

Double check that CI ran properly. If not, fix the issue on a new feature branch, merge it, then do a brand new release. In general, you'll increment only the patch version, unless it causes a breaking user-facing change (in which case you increment the major version).
