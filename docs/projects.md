# Projects

Feature introduces a concept called "projects". These are similar to git submodules, except that their exact commit isn't tracked by the parent repo.

Git submodules are a great way to depend on a git repository and ensure that it points to a particular commit. Git requires you to update it explicitly by checking out to a new commit in the submodule, then committing that change in the parent.

However, it's less useful when you have a single logical project split into several repos. You want those subprojects to remain up-to-date all the time, so having to commit in the parent repo any time new changes are introduced in a submodule is redundant.

Feature solves this by adding projects, which are entirely separate repos that aren't tracked by git in the parent repo. Feature looks at your list of projects (found in your project config file) in commands like `sync` to automatically keep them up to date in a single command.

Like git submodules, you interact with each subproject like a regular repo. `cd` into it and make changes, switch branches, push, pull, etc.

This allows you to turn several individual repos (that are part of the same overall project) into something like a monorepo.
You can even push this repo so other people (with feature) can use it as a monorepo by cloning and running `feature sync`.

The main downside is that people who don't use feature won't be able to use the monorepo without cloning each project manually.

While submodules kind of solve this, they have a confusing api and can't easily be brought "up-to-date".
`git submodule update` checks them out to the tracked commit, so they'll be in detached head and won't even have the latest changes.
In fact, when they initially setup the submodules with `git submodule init`, they might expect each submodule to be up to date, but this depends on when the parent repo last committed.

## Commands

### Add

```bash
feature project add --repo https://github.com/user/app-frontend frontend
feature project add --path ./modules/frontend frontend
feature project add --repo https://github.com/user/app-frontend --path ./modules/frontend frontend
```

Add a new project.

Feature will add the `[projects]` section to your project config if it doesn't exist yet (and create the project config if it doesn't exist yet), then add an entry with the project metadata. It will also add the project path to `.gitignore`.

The last argument is the name of the project, which is only used by feature to target specific projects in other commands. It must be unique across other projects in the repo.

If you omit `--path`, feature will crete a dir with the same name as the project (directly in the repo root).

If you omit `--repo`, feature will assume a repo already exists in the dir. If you include it, feature will clone the repo into the dir.

### Remove

```bash
feature project rm frontend
```

Remove a project.

This will delete the entry in your project config and remove the path from `.gitignore`.

Feature won't delete the repo itself.

### List

```bash
feature project ls
```

List all projects.

### Each

```bash
feature project each git config user.name "User Name"
feature project each feature start new branch
feature project each feature npm run format
```

Run a command in each project.

This is especially useful when working on features that span multiple projects. You can create a branch with the same name in each and even commit/push.
