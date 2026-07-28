//! `git-memo`

use crate::{Args, Category, Kind, Tool};

pub(crate) const GIT_MEMO: &str = r#"## Configuration

Set the global config

```shell
git config --global user.name "[name]"
git config --global user.email "[email]"
```

## Get started

Create a git repository

```shell
git init
```

Clone an existing git repository

```shell
git clone [url]
```

## Branches

Create a branch and switch to it

```shell
git switch -c [branch-name]
```

Switch to an existing branch

```shell
git switch [branch-name]
```

List local branches, or all of them

```shell
git branch
git branch -a
```

Rename the current branch

```shell
git branch -m [new-name]
```

Delete a merged branch, or one with unmerged work

```shell
git branch -d [branch-name]
git branch -D [branch-name]
```

Push a branch and set its upstream

```shell
git push -u origin [branch-name]
```

Set the upstream of an existing branch

```shell
git branch --set-upstream-to=origin/[branch-name]
```

Delete a branch on the remote

```shell
git push origin --delete [branch-name]
```

## Commit

Commit all tracked changes

```shell
git commit -am "[commit message]"
```

Add new modifications to the last commit

```shell
git commit --amend --no-edit
```

## I've made a mistake

Change last commit message

```shell
git commit --amend
```

Undo most recent commit and keep changes

```shell
git reset HEAD~1
```

Undo the `N` most recent commit and keep changes

```shell
git reset HEAD~N
```

Undo most recent commit and get rid of changes

```shell
git reset HEAD~1 --hard
```

Reset branch to remote state

```shell
git fetch origin
git reset --hard origin/[branch-name]
```

## Set work aside

Stash the working tree, keeping untracked files too

```shell
git stash push -u -m "[message]"
```

List stashes and put one back

```shell
git stash list
git stash pop stash@{0}
```

Work on two branches at once, in two directories

```shell
git worktree add ../[directory] [branch-name]
git worktree list
git worktree remove ../[directory]
```

## Move commits around

Take one commit from another branch

```shell
git cherry-pick [commit]
```

Replay a branch onto another base, dropping the old one

```shell
git rebase --onto [new-base] [old-base] [branch-name]
```

Continue or abandon a rebase that stopped on a conflict

```shell
git rebase --continue
git rebase --abort
```

## Find out what happened

Every position HEAD has had, including the ones no branch points at any more

```shell
git reflog
```

Recover a commit that a reset threw away

```shell
git reset --hard [commit-from-reflog]
```

Find the commit that introduced a bug, by bisection

```shell
git bisect start
git bisect bad
git bisect good [commit]
git bisect reset
```

Find the commit that last touched a line

```shell
git log -S "[string]"
git blame [file]
```

## Miscellaneous

Renaming the local master branch to main

```shell
git branch -m master main
```
"#;

pub(crate) fn git_memo(_: &Args) -> anyhow::Result<String> {
    Ok(GIT_MEMO.into())
}

pub static TOOL: Tool = Tool {
    id: "git-memo",
    name: "Git cheatsheet",
    description: "Quick access to the most common git commands.",
    category: Category::Development,
    keywords: &["git", "memo", "cheatsheet", "reference", "commands", "vcs"],
    fields: &[],
    kind: Kind::Pure,
    run: git_memo,
};
