# Projects: unified resource list

## Review of the task

The frontend can implement the requested list using existing profiles, discovery,
and inventory responses. No Rust, IPC, schema, or data-hook changes are needed.

The original task leaves several details unspecified:

- Existing profile editing and status details must remain accessible even though
  they are omitted from the proposed row actions.
- A registered project's children come from the existing `inventory.projects`
  projection, using the same Compose-name association as `useProjectStatus`.
  The UI must not invent a second association or classify raw Docker containers.
- Discovery candidates provide a count and metadata, not child container objects.
  Their expansion shows the existing path, count, and conflict evidence. It does
  not infer container ownership from a possibly conflicting project name.
- `already_registered` observations are omitted from the resource list because the
  registered profile is their UI owner. Other candidates are not deduplicated by
  name: different candidates can have the same name and different identities or paths.
- Loading, failed queries, stale observations, no registered profiles, and no
  search matches need distinct feedback. A failed source must not announce a
  successfully loaded empty resource list.
- Restore backup belongs to Diagnostics, so its entry point remains there.
- The Rust desktop crate is `src-tauri` (`colui-tauri`), not
  `crates/colui-tauri` as written in the task.

## UI decisions

`ResourceList` sorts all resource entries by name, with a stable identity tie-break.
Search trims whitespace and ignores case. It matches profile display/Compose names,
registered child container names, candidate names, and standalone container names.
A child-name match retains its parent group; the user expands it to inspect children.
Filtering keeps rows mounted to preserve expansion, pending actions, and feedback.

`ResourceRow` owns shared row presentation and group expansion. The existing feature
components retain profile, discovery, and container action ownership. Discovery's
existing controller renders its controls and supplies entries to the combined list;
its query hooks and per-candidate mutation bookkeeping remain in place.

Desktop actions appear on hover and keyboard focus; touch devices and narrow
layouts show actions.
Register stays visible. Icon actions have accessible names and native tooltips.
Project status still uses `projectStatusLabel`, represented by a labelled dot.
Ports expand separately and reuse `PortBindings` for exact backend Copy/Open values.
Profile confirmations and container log dialogs keep their existing content and
focus behavior. Add Project is always in the page header.

Definition details load only after the runtime reaches `ready`. Inventory status and
registered rows remain visible while the runtime connects; this prevents a transient
`runtime_unavailable` definition result from appearing as a persistent row error.

Automatically discovered profiles can lack the environment used by the original
Compose invocation because Docker labels expose working/config paths but not those
values. If definition loading fails for such a profile, its current containers remain
controllable through the existing container-ID/runtime-session command. The group row
provides Start/Stop/Restart across the current snapshot, and child rows provide the
same actions per container. Compose Apply and Tear down remain disabled until the
profile has a usable definition. The definition error is available in status details
instead of being repeated below every row.

Status chips, log side panels, auto-registration persistence, and Diagnostics
redesign remain outside this change.

## Verification

Test-first scenarios cover a mixed sorted list, search across resource types and
registered child names, keyboard expansion, child logs and focus restoration,
preserved expansion after filtering, and discovery failure versus empty state.
Existing tests cover action payloads, candidate eligibility and concurrent feedback,
confirmation content and focus, port copying/opening, and inventory polling ownership.

The browser smoke includes a mixed-list fixture, hover/focus action visibility,
search and expansion, logs, and a narrow-layout overflow check. It writes desktop
and mobile screenshots to a fresh temporary directory and prints its path.
Browser checks use mock IPC; they do not verify Tauri or a real Docker daemon.
