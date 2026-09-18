# Invariant audits recon — capability gating + blocking

Raw fact matrices for the invariant audit. Recon only: every row states what
the code does today, with `file:line` citations. No recommendations.

Basis: working tree of `develop` at 2026-09-18. async-lsp citations are from
the compiled source of `async-lsp 0.2.4`
(`~/.cargo/registry/src/index.crates.io-*/async-lsp-0.2.4/`), cited as
`async-lsp:<file>:<line>`. The word "gate" below means: the condition that
must hold for a surface to be reachable or active.

## Matrix 1 — capability-gated activation

### Structural fact that frames the whole matrix

There is **no dispatch-time capability check** for the 48 dispatch-table
rows. The wrapper answers every method the router knows about, whatever the
capabilities say. The "gate" is advertise-time only:

1. The implementor returns `ServerCapabilities` from
   `Server::server_capabilities` (`src/server/server_trait.rs:46-48`); the
   wrapper merges them into the `InitializeResult` verbatim
   (`src/server/with_state/initialize.rs:36`).
2. The final advertisement is recorded via `set_advertised_methods`
   (`src/server/with_state/initialize.rs:76` →
   `src/server/state/mod.rs:237-239`), which derives one boolean predicate
   per non-resolve method in `MethodInventory::from_capabilities`
   (`src/server/inventory.rs:103-115`; per-method predicates at
   `inventory.rs:179-311`).
3. The only runtime consequence is advisory: when a trait default runs and
   errors with `MethodNotImplemented`, the dispatch engines call
   `warn_once_default` — once per method, only if advertised
   (`macros/src/dispatch.rs:147,190`; `inventory.rs:123-141`).
   Unimplemented-but-unadvertised methods answer `-32601` silently
   (`src/server/server_trait.rs:706-708`).

The resolve family is absent from the inventory on purpose — its defaults
resolve the item unchanged and never error
(`src/server/inventory.rs:13-18`), so no resolve row has any gate check.

### 1A. The 48 `lsp_dispatch!` rows (`src/server/with_state/mod.rs:229-276`)

Capability = the field the trait method's `///` doc names (default-doc line
cited). Gate = where a check exists today. Pins: `wired_methods_dispatch`
(`src/server/tests/dispatch.rs:43`) exercises every non-resolve row on the
wire, and `inventory_covers_exactly_the_non_resolve_dispatch_rows`
(`dispatch.rs:142`) pins the table/inventory alignment; row-specific pins
listed per row.

| # | Method (row, mod.rs) | Default doc (server_trait.rs) | Capability that must gate it | Gate check today | Pin(s) beyond the shared ones |
|---|---|---|---|---|---|
| 1 | `hover` (229) | 61 | `hover_provider` | predicate `inventory.rs:181-184`; warn-only | `explicit_bool_false_never_advertises` (`inventory.rs:397`); `a_predicate_reads_its_own_capability_not_a_neighbors` (`inventory.rs:731`) |
| 2 | `declaration` (230) | 72 | `declaration_provider` | `inventory.rs:185-188`; warn-only | `inventory.rs:397` |
| 3 | `definition` (231) | 83 | `definition_provider` | `inventory.rs:189`; warn-only | `inventory.rs:397` |
| 4 | `references` (232) | 94 | `references_provider` | `inventory.rs:190`; warn-only | `inventory.rs:397` |
| 5 | `link` (233) | 105 | `document_link_provider` | `inventory.rs:191`; warn-only | `inventory.rs:604` (`all_advertised_shapes_advertise_their_methods`) |
| 6 | `rename` (234) | 116 | `rename_provider` | `inventory.rs:192`; warn-only | `inventory.rs:397` (explicit `Left(false)`) |
| 7 | `rename_prepare` (235) | 127 | `rename_provider` with `prepare_provider` | `inventory.rs:193-198`; warn-only | `advertised_defaults_warn_exactly_once` (`inventory.rs:364`, asserts the prepare shape at 379-381) |
| 8 | `document_format` (236) | 138 | `document_formatting_provider` | `inventory.rs:199`; warn-only | `inventory.rs:731` (positive control) |
| 9 | `document_range_format` (237) | 149 | `document_range_formatting_provider` | `inventory.rs:200-202`; warn-only | `inventory.rs:397` |
| 10 | `implementation` (238) | 160 | `implementation_provider` | `inventory.rs:203-208`; warn-only | `inventory.rs:397` |
| 11 | `type_definition` (239) | 171 | `type_definition_provider` | `inventory.rs:209-214`; warn-only | `inventory.rs:397` |
| 12 | `document_highlight` (240) | 182 | `document_highlight_provider` | `inventory.rs:215`; warn-only | `inventory.rs:397` |
| 13 | `on_type_formatting` (241) | 193 | `document_on_type_formatting_provider` | `inventory.rs:216` (presence); warn-only | `inventory.rs:604` |
| 14 | `folding_range` (242) | 204 | `folding_range_provider` | `inventory.rs:217-222`; warn-only | `inventory.rs:397` |
| 15 | `linked_editing_range` (243) | 215 | `linked_editing_range_provider` | `inventory.rs:223-232`; warn-only | `inventory.rs:397` |
| 16 | `code_lens` (244) | 226 | `code_lens_provider` | `inventory.rs:233` (presence); warn-only | `inventory.rs:604` |
| 17 | `will_save_wait_until` (245) | 237 | `text_document_sync` options `will_save_wait_until` | `inventory.rs:234-238`; warn-only | `inventory.rs:397` (`Some(false)` cell); `inventory.rs:604` |
| 18 | `document_color` (246) | 248 | `color_provider` | `inventory.rs:241-250`; warn-only | `inventory.rs:397` (`Simple(false)` cell) |
| 19 | `color_presentation` (247) | 259 | `color_provider` (same capability advertises both — comment `inventory.rs:239-240`) | `inventory.rs:241-250`; warn-only | same as row 18 |
| 20 | `prepare_call_hierarchy` (248) | 270 | `call_hierarchy_provider` | `inventory.rs:261-266`; warn-only | `inventory.rs:397` |
| 21 | `prepare_type_hierarchy` (249) | 281 | none exists — lsp-types 0.95.1 carries no type-hierarchy capability (`inventory.rs:17-19,267-269`) | predicate hard `false` (`inventory.rs:269`): never advertised, never warns | `inventory.rs:610-626` (asserts the trio is never advertisable) |
| 22 | `moniker` (250) | 292 | `moniker_provider` | `inventory.rs:270`; warn-only | `inventory.rs:397` |
| 23 | `will_create_files` (251) | 303 | `workspace.fileOperations.willCreate` | `inventory.rs:271` via `file_operation` (`inventory.rs:165-173`); warn-only | `file_operations_advertise_only_their_own_operation` (`inventory.rs:706`) |
| 24 | `will_rename_files` (252) | 314 | `workspace.fileOperations.willRename` | `inventory.rs:272`; warn-only | `inventory.rs:706` |
| 25 | `will_delete_files` (253) | 325 | `workspace.fileOperations.willDelete` | `inventory.rs:273`; warn-only | `inventory.rs:706` |
| 26 | `inlay_hint` (254) | 336 | `inlay_hint_provider` | `inventory.rs:274`; warn-only | `inventory.rs:397` |
| 27 | `document_symbol` (255) | 347 | `document_symbol_provider` | `inventory.rs:275`; warn-only | `inventory.rs:397` |
| 28 | `execute_command` (256) | 358 | `execute_command_provider` | `inventory.rs:276` (presence); warn-only | `inventory.rs:604` |
| 29 | `semantic_tokens_full` (257) | 369 | `semantic_tokens_provider` full leg | `inventory.rs:277-284`; warn-only | `semantic_tokens_advertise_full_range_and_delta_by_shape` (`inventory.rs:649`) |
| 30 | `semantic_tokens_range` (258) | 380 | `semantic_tokens_provider` range leg | `inventory.rs:285-287`; warn-only | `inventory.rs:649` |
| 31 | `semantic_tokens_full_delta` (259) | 391 | `semantic_tokens_provider` full delta leg | `inventory.rs:288-293`; warn-only | `inventory.rs:649` |
| 32 | `completion` (260) | 402 | `completion_provider` | `inventory.rs:294` (presence); warn-only | `inventory.rs:604` |
| 33 | `code_action` (261) | 413 | `code_action_provider` | `inventory.rs:295-297`; warn-only | `inventory.rs:397` |
| 34 | `document_diagnostics` (262) | 424 | `diagnostic_provider` (provider presence; the `workspace_diagnostics` flag does not affect method-level advertisement — `inventory.rs:382-385`) | `inventory.rs:298`; warn-only | `advertised_defaults_warn_exactly_once` (`inventory.rs:383-385`) |
| 35 | `selection_range` (263) | 435 | `selection_range_provider` | `inventory.rs:299-304`; warn-only | `inventory.rs:397` |
| 36 | `inline_value` (264) | 446 | `inline_value_provider` | `inventory.rs:305`; warn-only | `inventory.rs:397` |
| 37 | `incoming_calls` (265) | 457 | `call_hierarchy_provider` (doc: "Only issued when the server registered a call hierarchy provider" — client-side condition, no server check) | `inventory.rs:261-266`; warn-only | `inventory.rs:397` |
| 38 | `outgoing_calls` (266) | 468 | `call_hierarchy_provider` (same client-side condition) | `inventory.rs:261-266`; warn-only | `inventory.rs:397` |
| 39 | `supertypes` (267) | 479 | none exists (same lsp-types gap as row 21) | predicate hard `false` (`inventory.rs:269`) | `inventory.rs:610-626` |
| 40 | `subtypes` (268) | 490 | none exists (same gap) | predicate hard `false` (`inventory.rs:269`) | `inventory.rs:610-626` |
| 41 | `symbol` (269) | 501 | `workspace_symbol_provider` | `inventory.rs:306`; warn-only | `inventory.rs:397` |
| 42 | `signature_help` (270) | 512 | `signature_help_provider` | `inventory.rs:307` (presence); warn-only | `inventory.rs:604` |
| 43 | `completion_resolve` (271) | 523 | `completion_provider` with `resolve_provider` (doc only) | **none** — resolve family never enters the inventory (`inventory.rs:13-18`); default resolves unchanged | `resolve_converts_with_sole_document_and_passes_through_with_two` (`with_state/tests.rs:1113`); `code_lens_resolve_round_trips_through_the_sole_document` pattern family |
| 44 | `code_action_resolve` (272) | 534 | `code_action_provider` with `resolve_provider` (doc only) | none (as row 43) | no dedicated test found |
| 45 | `link_resolve` (273) | 545 | `document_link_provider` with `resolve_provider` (doc only) | none (as row 43) | `link_resolve_conversion_keys_on_the_sole_tracked_document` (`with_state/tests.rs:1163`) |
| 46 | `code_lens_resolve` (274) | 556 | `code_lens_provider` with `resolve_provider` (doc only) | none (as row 43) | `code_lens_resolve_round_trips_through_the_sole_document` (`with_state/tests.rs:1185`) |
| 47 | `inlay_hint_resolve` (275) | 567 | `inlay_hint_provider` with `resolve_provider` (doc only) | none (as row 43) | `inlay_hint_resolve_round_trips_through_the_sole_document` (`with_state/tests.rs:1208`) |
| 48 | `workspace_symbol_resolve` (276) | 578 | `workspace_symbol_provider` with `resolve_provider` (doc only) | none (as row 43) | `workspace_symbol_resolve_converts_per_url_and_passes_right_through` (`with_state/tests.rs:1253`); `workspace_symbol_resolve_converts_in_multi_document_states` (`with_state/tests.rs:1323`) |

Rows 43-48 carry no gate anywhere in the wrapper: `resolve_provider` is
named in docs only. No test pins a resolve-provider advertisement check,
because no check exists.

### 1B. Wrapper-handled surfaces

| Surface | Handler | Capability / gate | Where the check lives today | Pin(s) |
|---|---|---|---|---|
| `initialize` | `with_state/mod.rs:108-113` → `initialize.rs:17-115` | None — spec-mandatory. The wrapper force-inserts `text_document_sync` (incremental, openClose, save-with-text, `initialize.rs:62-71`), negotiates position encoding (`initialize.rs:26-58`, advertises at 61), and applies the workspace-diagnostics advertisement logic (Matrix 1C) | No gate; ordering gates (before initialize / no double initialize) live in async-lsp's `LifecycleLayer` (`serve.rs:75`) | `initialize_negotiates_position_encoding_end_to_end` (`src/server/tests/lifecycle.rs:10`); `requests_before_initialize_are_rejected` (`lifecycle.rs:24`); `double_initialize_is_rejected` (`lifecycle.rs:45`); `initialize_ignores_unknown_client_encodings` (`with_state/tests.rs:686`); `initialize_advertises_incremental_sync_with_open_close_and_save` (`with_state/tests.rs:732`); `initialize_prefers_encodings_by_the_preference_order` (`with_state/tests.rs:789`) |
| `initialized` | `with_state/mod.rs:117-120` → `workspace/diagnostics.rs:239-243` | None of its own; it fires the three registrations, each of which carries its own conjunction (rows below) | `diagnostics.rs:239-243` | `initialized_registers_did_change_configuration_when_supported` (`src/server/tests/workspace_diagnostics.rs:153`); `initialized_registers_file_watchers_when_supported` (`workspace_diagnostics.rs:185`); `initialized_skips_watcher_registration_without_support` (`workspace_diagnostics.rs:229`) |
| `shutdown` | **No wrapper handler** — async-lsp's `LanguageServer` default runs: `ready(Ok(()))` (`async-lsp:omni_trait.rs:101-106`) | None (spec) | No check anywhere in this crate | `requests_after_shutdown_are_rejected` (`lifecycle.rs:64`); `shutdown_exit_terminates_the_server_loop_cleanly` (`src/server/tests/termination.rs:41`) |
| `workspace/diagnostic` | `with_state/mod.rs:215-224` → `workspace/diagnostics.rs:259-284` | Three-layer: (a) advertisement `diagnostic_provider.workspace_diagnostics` (`diagnostics.rs:178-189`); (b) wrapper kill-switch forces it off under `WorkspaceDiagnostics::Disabled` (`diagnostics.rs:153-158`); (c) runtime verdicts `supported()` → `METHOD_NOT_FOUND` (`diagnostics.rs:267-272`) then `enabled()` → empty report (`diagnostics.rs:274-278`) | `diagnostics.rs:267-278`; verdict set in `configure_capabilities` (`diagnostics.rs:160-161`) | `resolution_matrix_advertises_verbatim_and_gates_support` (`diagnostics.rs` tests, line 1148); `registration_options_arm_follows_the_same_matrix` (1187); `provider_none_advertises_nothing_and_stays_unsupported` (1216); `disabled_options_force_workspace_diagnostics_capability_off` (1052); `supported_starts_false_before_configure_capabilities` (1136); `initialize_enables_workspace_diagnostics` (`with_state/tests.rs:635`); `initialize_respects_disabled_workspace_diagnostics` (`with_state/tests.rs:662`) |

### 1C. Dynamic registrations and refresh (full gate conjunctions)

| Mechanism | Location | Full conjunction | Conjunction check | Pin(s) |
|---|---|---|---|---|
| `register_watchers` (didChangeWatchedFiles registration) | `workspace/diagnostics.rs:324-386` | client `workspace.didChangeWatchedFiles.dynamicRegistration` (captured at `diagnostics.rs:167-173`) **and** workspace diagnostics enabled (= `supported && enabled`, `diagnostics.rs:63-65,81-83`) **and** not already registered (`watchers_registered`, `diagnostics.rs:333`) **and** non-empty watcher globs (`diagnostics.rs:337-340`). The registered flag is set only on client acceptance (`diagnostics.rs:377-384`); disable never unregisters (`diagnostics.rs:326-330`); fixed id `WATCHED_FILES_REGISTRATION_ID` (`diagnostics.rs:316`) | `diagnostics.rs:331-335` | wire: `initialized_registers_file_watchers_when_supported` (`workspace_diagnostics.rs:185`, asserts id/glob/kind 7); `initialized_skips_watcher_registration_without_support` (`workspace_diagnostics.rs:229`, both the capability conjunct and the enabled conjunct); unit: `file_watching_follows_the_client_capability` (`diagnostics.rs` tests, 1261); walk-cache gating: `walk_cache_serves_between_invalidations_and_refreshes_on_them` (`state/tests.rs:1246`), `without_watchers_every_poll_walks_and_sees_new_files` (`state/tests.rs:1325`), `invalidation_flips_serving_to_walking_and_back` (`state/tests.rs:1364`) |
| `register_configuration` (didChangeConfiguration section registration) | `workspace/diagnostics.rs:286-312` | `supported` **and** client `workspace.didChangeConfiguration.dynamicRegistration` (`diagnostics.rs:114-119`) **and** a `Configurable` setting exists (`configuration_gate`, `diagnostics.rs:86-89`) | `can_register_configuration`, `diagnostics.rs:95-97`, checked at 288 | wire: `initialized_registers_did_change_configuration_when_supported` (`workspace_diagnostics.rs:153`, asserts method + section); unit: `register_configuration_requires_dynamic_registration_support` (`diagnostics.rs` tests, 999); `machinery_gates_on_supported` (1234) |
| `request_configuration` (`workspace/configuration` pull) | `workspace/diagnostics.rs:388-420` | `supported` **and** client `workspace.configuration` (`diagnostics.rs:109-113`) **and** `Configurable` setting (`diagnostics.rs:87-89`); replies apply only when the generation is still current (`diagnostics.rs:396,408-410`) | `can_request_configuration`, `diagnostics.rs:91-93`, checked at 390 | `request_configuration_requires_client_capability_and_setting` (`diagnostics.rs` tests, 982); `stale_generation_drops_the_response` (1032); wire: `configuration_reply_applies_only_if_generation_current` (`src/server/tests/workspace_diagnostics.rs:258`) |
| `refresh_diagnostics` (`workspace/diagnostic/refresh` request) | `workspace/diagnostics.rs:439-453` | `supported` **and** client `workspace.diagnostic.refreshSupport` (`diagnostics.rs:121-127`) — `can_refresh`, `diagnostics.rs:99-101`; fired only on an actual enabled-value change (`apply_enabled`, `diagnostics.rs:434-436`) | `diagnostics.rs:440` | `refresh_gate_tracks_client_refresh_support` (`diagnostics.rs` tests, 1010); `machinery_gates_on_supported` (1234); wire: `refresh_fires_only_on_change_and_only_when_supported` (`src/server/tests/workspace_diagnostics.rs:306`) |
| `configure_capabilities` (the initialize-time merger) | `workspace/diagnostics.rs:143-174` | Disabled kill-switch (`153-158`); supported verdict from the post-merge advertisement (`160-161`); workspace-folder tracking flipped on only when supported (`163-165`, `204-222`); file-watching flag from the client capability (`167-173`) | `diagnostics.rs:143-174` | `resolution_matrix_advertises_verbatim_and_gates_support` (1148); `machinery_gates_on_supported` (1234); `file_watching_follows_the_client_capability` (1261); `disabled_options_force_workspace_diagnostics_capability_off` (1052) |
| Settings-driven enable (initializationOptions / didChangeConfiguration) | `diagnostics.rs:224-237` (`apply_initialization_options`), `diagnostics.rs:245-257` (`did_change_configuration`) | Only under a `Configurable` setting (`diagnostics.rs:229-231,247-249`); `did_change_configuration` falls back to a configuration pull when the section carries no value (`diagnostics.rs:254-256`) | `diagnostics.rs:229-236,247-256` | `configurable_workspace_diagnostics_can_be_toggled` (`with_state/tests.rs:806`); `configurable_workspace_diagnostics_read_initialization_options` (`with_state/tests.rs:893`) |

### 1D. Notification and event handlers — spec-unconditional

Marked per the brief so the audit does not treat them as gaps: the LSP spec
attaches no server capability to receiving these, and the wrapper gates
none of them.

| Notification | Internal handler | Gate |
|---|---|---|
| `initialized` | `with_state/mod.rs:117` | no capability gate required (spec) |
| `textDocument/didOpen` | `with_state/mod.rs:141` → `state/documents.rs:81` | no capability gate required (spec) |
| `textDocument/didChange` | `with_state/mod.rs:155` → `state/documents.rs:129` | no capability gate required (spec) |
| `textDocument/didClose` | `with_state/mod.rs:148` → `state/documents.rs:96` | no capability gate required (spec) |
| `textDocument/didSave` | `with_state/mod.rs:161` → `state/documents.rs:287` | no capability gate required (spec) |
| `textDocument/willSave` | `with_state/mod.rs:168` (trait hook only, no internal state change) | no capability gate required (spec) |
| `workspace/didChangeWatchedFiles` | `with_state/mod.rs:174` → `state/documents.rs:351` | no capability gate required (spec) — the *registration* that asks the client to send it is gated (Matrix 1C) |
| `workspace/didChangeWorkspaceFolders` | `with_state/mod.rs:131` → `state/workspace.rs:25` | no capability gate required (spec) |
| `workspace/didChangeConfiguration` | `with_state/mod.rs:122` → `workspace/diagnostics.rs:245` | no capability gate required (spec) — the settings *section* handling is `Configurable`-gated (Matrix 1C) |
| `workspace/didCreateFiles` | `with_state/mod.rs:186` | no capability gate required (spec); the wrapper never advertises the `did*` file-operation fields anywhere (only `will*` predicates exist, `inventory.rs:271-273`) |
| `workspace/didRenameFiles` | `with_state/mod.rs:192` | no capability gate required (spec); same note |
| `workspace/didDeleteFiles` | `with_state/mod.rs:199` | no capability gate required (spec); same note |
| `window/workDoneProgress/cancel` | `with_state/mod.rs:206` | no capability gate required (spec) |

Pins for the hook-always-runs contract: `notification_hooks_run_after_the_internal_handlers`
(`with_state/tests.rs:1471`).

## Matrix 2 — blocking sites

### 2A. Synchronous disk IO on the executor thread

| Site | Operation | Bounded? | Currently justified? | Served by |
|---|---|---|---|---|
| `with_state/mod.rs:57` (`read_document_from_disk`) | `std::fs::read_to_string`, one file per request, when the request URL is not tracked | No pool — runs on the executor; bounded only by "at most one file per request" (comment `with_state/mod.rs:49-50` "Blocking by design, matching the crate's other disk reads"; arch-lint allow at 56) | Comment + arch-lint allow (cited) | All 42 URL-anchored dispatch rows via `conversion_document` (`with_state/mod.rs:41-46`, engine step 2 at `macros/src/dispatch.rs:179`); also `symbol.rs:66` (per-request HashMap cache, comment `symbol.rs:50-53`) and `workspace_symbol_resolve.rs:49` |
| `state/documents.rs:119` (didClose) | `std::fs::read_to_string` — keeps a disk snapshot when workspace diagnostics are on | No pool — notification handler must stay synchronous (arch-lint allow at 118) | arch-lint reason cites the LSP synchronous-notification constraint | `didClose` (`with_state/mod.rs:148`) |
| `state/documents.rs:253` (didChange fallback) | `std::fs::read_to_string` — recovery when incremental application failed | No pool — same constraint; comment `documents.rs:245-252` states it and names the trade-off (unsaved editor changes discarded) | Comment `documents.rs:245-252` + arch-lint allow at 252 | `didChange` (`with_state/mod.rs:155`) |
| `state/documents.rs:302` (didSave fallback) | `std::fs::read_to_string` — when `params.text` is absent | No pool — same constraint (comment `documents.rs:296-298`, arch-lint allow at 301) | Comment + arch-lint allow (cited) | `didSave` (`with_state/mod.rs:161`) |
| `state/documents.rs:388` (watched-files refresh) | `std::fs::read_to_string` — replaces a tracked Workspace snapshot on Created/Changed events | No pool — comment `documents.rs:381-384` states the synchronous-handler constraint (arch-lint allow at 387) | Comment + arch-lint allow (cited) | `didChangeWatchedFiles` (`with_state/mod.rs:174`) |
| `state/workspace.rs:262` (`workspace_folder_path`) | `std::fs::canonicalize` per folder, `unwrap_or(path)` on failure | No pool — runs on the calling thread | arch-lint allow at 261 says "one-time path canonicalization during workspace-folder setup" — but the same function also runs per folder on every `didChangeWorkspaceFolders` notification (`state/workspace.rs:37,46`), not only during `initialize` (`state/workspace.rs:19`, via `set_workspace_folders` at `with_state/initialize.rs:81`) | `initialize` (`with_state/mod.rs:108`) and `didChangeWorkspaceFolders` (`with_state/mod.rs:131`) |

### 2B. Blocking work on the blocking pool (spawn_blocking)

| Site | Operation | Bounded? | Currently justified? | Served by |
|---|---|---|---|---|
| `state/workspace.rs:211-217` (`file_stamp` probe) | `std::fs::metadata` (`state/workspace.rs:194-198`) inside `spawn_blocking` | Yes — one blocking hop per candidate file | Comment `state/workspace.rs:200-204` ("The metadata probe and the read+parse+install composite run on the blocking pool"); rule doc in `.claude/rules/structure.md` ("the stamp probe and the read+parse composite … run on the blocking pool") | `workspace/diagnostic` — `refresh_workspace_documents` → `load_workspace_document` (`state/workspace.rs:82-154`, engine `workspace/diagnostics.rs:483-569`) |
| `state/workspace.rs:229-240` (read+parse+install composite) | `std::fs::read_to_string` + `insert_document` (incl. tree-sitter parse, `state/documents.rs:38-49`) + map install, one `spawn_blocking` hop; stamp written after (`state/workspace.rs:241-243`) | Yes — one hop per file, width-bounded by `for_each_bounded` (`state/workspace.rs:125-140`) | Comment `state/workspace.rs:200-204` + arch-lint allow at 233 | same as above |
| `state/workspace.rs:159-177` (`walk_blocking`) | `WorkspaceWalker::new` (canonicalizes every root, `src/workspace/walker.rs:43-47`) + `ignore`-crate parallel walk (`walker.rs:63-81`) + matcher lookup + `path_to_url`, all in one `spawn_blocking` | Yes — one hop per poll; the result feeds `WalkCache::store` (`state/workspace.rs:100-106`) | Comment `state/workspace.rs:156-158` ("The workspace walk, off the executor"); arch-lint allows at `walker.rs:67` and `state/workspace.rs:195` | `workspace/diagnostic` (both branches: cache-miss and no-watchers, `state/workspace.rs:96-106`) |
| `src/oneshot/workspace_diagnostics.rs:249-269` (open composite) | `fs::read_to_string` + `open_document` (drives `did_open` → `insert_document` → tree-sitter parse, `src/oneshot/server.rs:36-44`) | `spawn_blocking` when a tokio runtime is current (`oneshot:262-266`); **inline otherwise** — plain-executor oneshot runs have no runtime to offload to (comment `oneshot:244-248`, arch-lint allow at 250) | Comment `oneshot:244-248` (cited) | `oneshot::workspace_diagnostics` (`oneshot/workspace_diagnostics.rs:206-282`) |
| `src/oneshot/workspace_diagnostics.rs:214-225` (walker setup + walk) | `WorkspaceWalker::new` root canonicalization (`walker.rs:43-47`) and `walker.files()` (`walker.rs:56-88`) run **directly on the caller's thread** in the oneshot — no `spawn_blocking` here (the batch-CLI context) | Width-bounded later by `for_each_bounded` for the per-document work; the walk itself unbounded-sync | arch-lint allow at `walker.rs:67` ("the ignore-crate walk is a synchronous batch scan by design") | `oneshot::workspace_diagnostics` |

### 2C. Tree-sitter parse sites — on which thread

Answer to the brief's question: **Open-document parses run on the executor
thread; Workspace-load parses run on the blocking pool.**

| Site | Thread | Reached from |
|---|---|---|
| `insert_document` parse (`state/documents.rs:38-49`, `parse_rope` at 487-501) | Executor thread (synchronous notification handlers) | `didOpen` (`documents.rs:85`), `didClose` disk snapshot (`documents.rs:120`), `didChange` failed-incremental recovery (`documents.rs:254`), `didChangeWatchedFiles` re-read (`documents.rs:389`) — and, from the pool, `load_workspace_document` (`state/workspace.rs:235`) |
| Incremental `tree.edit()` (`state/documents.rs:180-191`) + finalize re-parse `finalize_edited_tree` (`documents.rs:207-209`, def 470-479) | Executor thread | `didChange` |
| `replace_full_text` full re-parse (`documents.rs:507-521`, called at 160-166) | Executor thread | `didChange` (a change with no range) |
| `didSave` full re-parse (`state/documents.rs:319-333`) | Executor thread | `didSave` |
| Recovery re-parse of kept text (`state/documents.rs:268-283`) | Executor thread | `didChange` (incremental failed AND disk re-read failed) |
| `Document::query` execution (`src/documents/document.rs:307-352`) — runs the compiled query over the tree via `QueryCursor` | Whichever thread calls it — executor for request handlers | Downstream `Server` implementations, inside their request handlers |
| Workspace-load parse | Blocking pool (via the composite above) | `load_workspace_document` (`state/workspace.rs:229-240`) |
| Oneshot open parse | Blocking pool when a runtime exists; caller thread otherwise | `oneshot/workspace_diagnostics.rs:249-269` |

Justification pattern for the executor-thread parses: every site cites the
synchronous-notification-handler constraint (`documents.rs:245-247`,
`documents.rs:296-298`, `documents.rs:381-384`; trait docs repeat it per
hook, e.g. `server_trait.rs:592-594`).

### 2D. Locks

| Lock | Location | Scope / cross-await? | Notes |
|---|---|---|---|
| `WalkCache.inner: Mutex<WalkCacheInner>` | `src/server/state/walk_cache.rs:13-15` | Guard never escapes a method (comment `walk_cache.rs:28-30`); all four mutators/one reader are sync — never held across an await | Poisoning recovered, reasoned at `walk_cache.rs:56-63`; callers: `refresh_workspace_documents` (`state/workspace.rs:96-106`), `handle_watched_files_change` (`state/documents.rs:360`), `apply_enabled` (`workspace/diagnostics.rs:427,432`), `set_workspace_folders` / `handle_workspace_folders_change` (`state/workspace.rs:16,31`) |
| `semantic_tokens_cache: DashMap<Url, CachedSemanticTokens>` | `src/server/state/mod.rs:33` | Reads `state/mod.rs:179-183` (guard dropped after clone), writes `state/mod.rs:187-189`; evictions at `state/documents.rs:78,114,123,307,346,377,434` and `state/mod.rs`-owned `retain_documents` sweep `state/documents.rs:441-453` | No await occurs under a guard; conversions consult it via `cached_semantic_tokens` (`src/lsp_requests/conversion.rs:887`) |
| `DocumentMatcher.compiled_queries: DashMap<String, Arc<Query>>` (feature-gated) | `src/documents/matcher.rs:47` | `compiled_query` get-or-insert, `matcher.rs:138-155`; sync, no await | Concurrent duplicate compiles benign — comment `matcher.rs:145-148`; entry is a pure function of (source, grammar), no invalidation path — comment `matcher.rs:42-46` |
| `DocumentInner.derived: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>` | `src/documents/document.rs:53` | `Document::derived` takes the lock twice (probe `document.rs:221-230`, insert `document.rs:233-237`); the lock is released before `compute` runs — re-entrant `derived` on the same `T` would recurse forever (documented `document.rs:211-214`) | Poisoning recovered via `PoisonError::into_inner` (`document.rs:225,236`); generation-scoped by construction (fresh inner per write) |
| `documents: DashMap<Url, DocumentEntry>` guards | `src/server/state/mod.rs:26` | `handle_document_change` holds a `get_mut` guard across the whole edit batch (`state/documents.rs:133-236`) — sync function, dropped before the re-entrant `insert_document` (explicit comment `documents.rs:227-229`: drop to prevent deadlock); `handle_document_save` holds a guard while parsing (`documents.rs:292-341`, sync); `handle_document_close` drops before insert (`documents.rs:110`); `load_workspace_document`'s probe guard is a statement-temporary (`state/workspace.rs:218-222`) | No DashMap guard is held across an `.await` anywhere: every guard-holding function is synchronous; async paths clone `Document` snapshots out (`state/mod.rs:83-118`) |
| `MethodInventory` | `src/server/inventory.rs:68-76` | Lock-free (`Arc` + `AtomicBool` warned flags, `inventory.rs:133`) | Not a blocking site; listed because the brief asks where the advertised-method state lives |

### 2E. `serve()` transport and middleware

| Site | Fact | Cite |
|---|---|---|
| Pipe locking | `serve()` locks stdin/stdout via async-lsp `PipeStdin::lock_tokio`/`PipeStdout::lock_tokio` — `lock()` then `AsyncFd` registration (READABLE/WRITABLE); one-time at startup; errors when the fd cannot be locked as a pipe or `AsyncFd` creation fails (doc `src/server/serve.rs:16-19,43-47`) | `src/server/serve.rs:53-56`; `async-lsp:stdio.rs:302-310,383-391` |
| ConcurrencyLayer width | `ConcurrencyLayer::default()` = `std::thread::available_parallelism()`, fallback 1 (`async-lsp:concurrency.rs:174-185`); the crate passes no custom bound | `src/server/serve.rs:79` |
| CatchUnwindLayer | Panic catching → structured internal-error replies; pin `panicking_handler_returns_structured_error` (`src/server/tests/robustness.rs:72`) | `src/server/serve.rs:80` |
| ClientProcessMonitorLayer | Reads `processId` from `initialize` params, opens a `waitpid_any` watch, and stops the main loop when the client process exits (`async-lsp:client_monitor.rs:1-18` and the `call` impl); `processId: null` leaves it inert | `src/server/serve.rs:81`; pin `at_most_limit_requests_run_concurrently` (`robustness.rs:98`, the documented async-lsp 0.2.4 deadlock tripwire) |
| LifecycleLayer | initialize-before-everything gating (lives upstream, not in this crate); pins `requests_before_initialize_are_rejected` (`lifecycle.rs:24`), `double_initialize_is_rejected` (`lifecycle.rs:45`) | `src/server/serve.rs:75` |
| Staleness gate (not serve(), adjacent engine fact) | Dispatch engines return `CONTENT_MODIFIED` when the document version moved during handling (`macros/src/dispatch.rs:196-203`); pin `stale_document_answers_content_modified_then_succeeds_on_retry` (`src/server/tests/staleness.rs:12`) | cited inline |

### 2F. globset / query compilation on matcher paths

| Site | Fact | Cite |
|---|---|---|
| `DocumentMatchers::new` | Compiles each matcher's `GlobSet` once, at `ServerState` construction (`state/mod.rs:129`) and per oneshot run (`oneshot/workspace_diagnostics.rs:215`); invalid globs warned and skipped, unbuildable globsets dropped (`matcher.rs:174-195`) | `src/documents/matcher.rs:167-208` |
| `find_path` / `find_url` / `find` | Run `is_match` over the **already-compiled** sets — lookup, not compilation; per walk entry (`walk_blocking`) and per request (`matchers.find` in `insert_document`, `handle_document_save`) | `src/documents/matcher.rs:210-230` |
| `watcher_globs` | Re-runs `Glob::new` per glob per call as a validity filter — compilation cost per invocation, but invoked only per enable transition (`register_watchers`, `workspace/diagnostics.rs:337`), not per request | `src/documents/matcher.rs:236-247`; `src/server/state/mod.rs:222-227` |
| Per-matcher query cache | First `Document::query(source)` compiles `Query::new` against the matcher's grammar; later calls hit the `DashMap` cache — compilation once per (matcher, source) | `src/documents/matcher.rs:138-155`; `src/documents/document.rs:320-325` |

## Row counts

- Matrix 1: 71 rows — 48 dispatch rows (1A) + 4 wrapper-handled surfaces (1B) + 6 dynamic-registration/config mechanisms (1C) + 13 notifications (1D, incl. `initialized` and `work_done_progress/cancel`).
- Matrix 2: 35 rows — 6 executor-thread disk-IO sites (2A) + 5 spawn_blocking sites (2B) + 8 tree-sitter parse/threading rows (2C) + 6 locks (2D) + 6 serve()/middleware facts (2E) + 4 compilation facts (2F).

Nothing in either matrix is [Unverified]: every cited line was read in the
working tree; async-lsp facts were read from the compiled 0.2.4 source.
