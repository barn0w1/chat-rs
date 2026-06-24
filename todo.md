# chat-rs Web Client Todo

Updated: 2026-06-24

## Current Objective

Implement the fourth web-client step from
`docs/web-client-initial-implementation-plan.md`: fix the Lit template build
error from Step 3, add the authenticated HTTP chat MVP, and revise the layout
so the application shell uses the full viewport width while message rows remain
comfortable to read.

## Step 1 Checklist

- [x] Read the current `web/` starter and implementation plan.
- [x] Replace starter entrypoint with `src/main.ts`.
- [x] Replace `my-element` with a `chat-app` production shell.
- [x] Add base, layout, and token CSS files.
- [x] Remove starter-only assets and imports.
- [x] Verify the changed file set and static references.
- [x] Archive the full codebase for download.

## Step 2 Checklist

- [x] Read server HTTP problem and route contracts.
- [x] Check design references and accessibility contrast guidance.
- [x] Add `web/src/api/types.ts`.
- [x] Add `web/src/api/problems.ts`.
- [x] Add `web/src/api/client.ts`.
- [x] Update static shell theme away from the green palette.
- [x] Verify static references and changed file set.
- [x] Archive the full codebase for download.

## Step 3 Checklist

- [x] Review form, live-region, contrast, and text-spacing references.
- [x] Add `web/src/state/session-store.ts`.
- [x] Add `web/src/components/chat-login.ts`.
- [x] Wire `chat-app` to session loading and logout.
- [x] Rework the shell toward readable IRC-like message rows.
- [x] Keep admission-code login as a same-origin form POST.
- [x] Verify static references and changed file set.
- [x] Archive the full codebase for download.

## Step 4 Checklist

- [x] Fix the raw-backtick Lit template build error in `app-shell`.
- [x] Add `web/src/state/conversation-store.ts`.
- [x] Add `web/src/state/message-store.ts`.
- [x] Wire authenticated conversation reads, creation, selection, and message
      reads/posts into `chat-app`.
- [x] Keep POST requests manual-only with no automatic retry.
- [x] Revise the shell to full viewport width while keeping IRC-like message
      rows readable.
- [x] Update the implementation plan status.
- [x] Verify static references and changed file set.
- [x] Archive the full codebase for download.

## Notes

- Keep runtime dependencies unchanged.
- Step 4 may call authenticated conversation and message HTTP APIs from the app
  shell, using the in-memory session CSRF token for unsafe requests.
- Keep all IDs as strings across the browser boundary.
- Do not automatically retry POST requests.
- Design direction: technical, text-first, readable for long sessions,
  off-white light theme, restrained contrast, IRC-like message rows. Avoid
  building a decorative game world.
- Step 4 build note: attempted to install and build in a temporary copy, but
  dependency fetches from npm registry returned 403 in this environment. The
  workspace itself was left without `node_modules` or `dist`.
