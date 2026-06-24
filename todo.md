# chat-rs Web Client Todo

Updated: 2026-06-24

## Current Objective

Implement the third web-client step from
`docs/web-client-initial-implementation-plan.md`: add session loading, login
surface, and logout. Also correct the visual direction toward a technical,
text-first, light IRC-like client instead of a world-themed UI.

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

## Notes

- Keep runtime dependencies unchanged.
- Step 3 may call only `/api/v1/session` and `/api/v1/session` DELETE from the
  app shell. Conversation/message API calls begin in the next implementation
  step.
- Keep all IDs as strings across the browser boundary.
- Do not automatically retry POST requests.
- Design direction: technical, text-first, readable for long sessions,
  off-white light theme, restrained contrast, IRC-like message rows. Avoid
  building a decorative game world.
