# LSP surface verification — lsp_types 0.95.1

Verified 2026-09-01 against pinned sources. All facts below come from files read in full or in targeted ranges; every type carries a `file:line` anchor into those files.

Sources:

- `lsp_types` 0.95.1 — `/Users/vasilsokolik/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lsp-types-0.95.1/src/…` (referred to below as `LT:<file>:<line>`; the file list was confirmed by `ls`)
- `async-lsp` 0.2.4 — `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/async-lsp-0.2.4/src/omni_trait_generated.rs` (trait-method table), `omni_trait.rs` (`define!` / `define_server!` macros)

Wire-method → trait-method binding: `omni_trait_generated.rs:4–56` (requests), `58–74` (client→server notifications). The `define_server!` macro resolves each wire method through `lsp_request!($method)` (`omni_trait.rs:60–86`, `7`), so the `Params`/`Result` types are exactly the `lsp_types::request::Request` impls in `LT:request.rs`.

Shared leaf types used throughout (anchors, `LT:lib.rs`):

- `LSPAny = serde_json::Value` — lib.rs:227
- `Position { line: u32, character: u32 }` (both non-Option) — lib.rs:242–251. Doc on `character`: "The meaning of this offset is determined by the negotiated `PositionEncodingKind`. If the character value is greater than the line length it defaults back to the line length."
- `Range { start: Position, end: Position }` — lib.rs:262–267
- `Location { uri: Url, range: Range }` — lib.rs:277–280
- `LocationLink { origin_selection_range: Option<Range>, target_uri: Url, target_range: Range, target_selection_range: Range }` — lib.rs:291–307
- `Command { title: String, command: String, arguments: Option<Vec<Value>> }` — lib.rs:498–507
- `TextEdit { range: Range, new_text: String }` — lib.rs:526–533
- `AnnotatedTextEdit { #[serde(flatten)] text_edit: TextEdit, annotation_id: ChangeAnnotationIdentifier }` — lib.rs:552–558; `ChangeAnnotationIdentifier = String` — lib.rs:545
- `TextDocumentEdit { text_document: OptionalVersionedTextDocumentIdentifier, edits: Vec<OneOf<TextEdit, AnnotatedTextEdit>> }` — lib.rs:567–576
- `ChangeAnnotation { label: String, needs_confirmation: Option<bool>, description: Option<String> }` — lib.rs:583–597
- `DocumentChanges` (untagged): `Edits(Vec<TextDocumentEdit>)` | `Operations(Vec<DocumentChangeOperation>)` — lib.rs:738–741; `DocumentChangeOperation` (untagged, `rename_all = "lowercase"`): `Op(ResourceOp)` | `Edit(TextDocumentEdit)` — lib.rs:760–763; `ResourceOp` (tagged `kind`, lowercase): `Create(CreateFile)` | `Rename(RenameFile)` | `Delete(DeleteFile)` — lib.rs:767–771
- `CreateFile { uri: Url, options: Option<CreateFileOptions>, annotation_id: Option<ChangeAnnotationIdentifier> }` — lib.rs:624–636; `RenameFile { old_uri: Url, new_uri: Url, options: Option<RenameFileOptions>, annotation_id: … }` — lib.rs:653–667; `DeleteFile { uri: Url, options: Option<DeleteFileOptions> }` — lib.rs:690–696
- `TextDocumentIdentifier { uri: Url }` — lib.rs:922–929
- `TextDocumentPositionParams { text_document: TextDocumentIdentifier, position: Position }` (both non-Option) — lib.rs:1017–1027
- `OneOf<A, B>` (untagged): `Left(A)` | `Right(B)` — lib.rs:1848–1851
- `MarkupContent { kind: MarkupKind, value: String }` — lib.rs:2730–2733
- `WorkDoneProgressParams { work_done_token: Option<ProgressToken> }` — LT:progress.rs:52–57; `ProgressToken = NumberOrString` — progress.rs:5
- `PartialResultParams { partial_result_token: Option<ProgressToken> }` — LT:lib.rs:2738–2741
- `Documentation` enum — lib.rs:2556 (string|MarkupContent; used in `ParameterInformation`/`SignatureInformation`)

`Url` in this crate is `url::Url` (re-exported, `LT:lib.rs` `use` chains, e.g. call_hierarchy.rs:3, type_hierarchy.rs:4 `Url` alias).

---

## Requests (one section per async-lsp trait method)

The trait-method names are quoted verbatim from `omni_trait_generated.rs`; the line given after each method name is the `impl Request for …` block in `LT:request.rs` unless stated otherwise.

### implementation (omni_trait_generated.rs:7)

- Request type: `request::GotoImplementation` (request.rs:378); wire `textDocument/implementation`
- Params: `GotoImplementationParams` = type alias for `GotoTypeDefinitionParams` = type alias for `GotoDefinitionParams` (request.rs:365, 380) = `GotoDefinitionParams { #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — lib.rs:2599–2608. One incoming position.
- Result: `Option<GotoImplementationResponse>` = `Option<GotoDefinitionResponse>` (request.rs:381, 385). `GotoDefinitionResponse` (untagged): `Scalar(Location)` | `Array(Vec<Location>)` | `Link(Vec<LocationLink>)` — lib.rs:2613–2617. Outgoing conversion must cover all three variants.

### type_definition (omni_trait_generated.rs:8)

- Request type: `request::GotoTypeDefinition` (request.rs:363); wire `textDocument/typeDefinition`
- Params: `GotoTypeDefinitionParams` = `GotoDefinitionParams` (request.rs:365) — lib.rs:2599–2608. One incoming position.
- Result: `Option<GotoTypeDefinitionResponse>` = `Option<GotoDefinitionResponse>` (request.rs:366, 370) — lib.rs:2613–2617.

### signature_help (omni_trait_generated.rs:37)

- Request type: `request::SignatureHelpRequest` (request.rs:316); wire `textDocument/signatureHelp`
- Params: `SignatureHelpParams { context: Option<SignatureHelpContext>, #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams }` — LT:signature_help.rs:100–113. One incoming position.
  - `SignatureHelpContext { trigger_kind: SignatureHelpTriggerKind, trigger_character: Option<String>, is_retrigger: bool, active_signature_help: Option<SignatureHelp> }` — signature_help.rs:115–136. `SignatureHelpTriggerKind` is `#[serde(transparent)] i32` with consts `INVOKED = 1`, `TRIGGER_CHARACTER = 2`, `CONTENT_CHANGE = 3` — signature_help.rs:86–98.
- Result: `Option<SignatureHelp>` (request.rs:320).
  - `SignatureHelp { signatures: Vec<SignatureInformation>, active_signature: Option<u32>, active_parameter: Option<u32> }` — signature_help.rs:141–154.
  - `SignatureInformation { label: String, documentation: Option<Documentation>, parameters: Option<Vec<ParameterInformation>>, active_parameter: Option<u32> }` — signature_help.rs:159–182.
  - `ParameterInformation { label: ParameterLabel, documentation: Option<Documentation> }` — signature_help.rs:186–200. See Special semantics for `ParameterLabel`.

### document_highlight (omni_trait_generated.rs:40)

- Request type: `request::DocumentHighlightRequest` (request.rs:397); wire `textDocument/documentHighlight`
- Params: `DocumentHighlightParams { #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:document_highlight.rs:10–21. One incoming position.
- Result: `Option<Vec<DocumentHighlight>>` (request.rs:401).
  - `DocumentHighlight { range: Range, kind: Option<DocumentHighlightKind> }` — document_highlight.rs:26–34. `DocumentHighlightKind` transparent i32: `TEXT = 1`, `READ = 2`, `WRITE = 3` — document_highlight.rs:37–51.

### selection_range (omni_trait_generated.rs:13)

- Request type: `request::SelectionRangeRequest` (request.rs:706); wire `textDocument/selectionRange`
- Params: `SelectionRangeParams { text_document: TextDocumentIdentifier, positions: Vec<Position>, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:selection_range.rs:60–74. **Multiple incoming positions.** Doc: "`positions[i]` must be contained in `result[i].range`." (request.rs:697–705)
- Result: `Option<Vec<SelectionRange>>` (request.rs:710). Same length/indices as `positions`.
  - `SelectionRange { range: Range, parent: Option<Box<SelectionRange>> }` — selection_range.rs:77–86. Linked list; every `Range` in the chain (including through `parent`) is conversion-relevant.

### folding_range (omni_trait_generated.rs:11)

- Request type: `request::FoldingRangeRequest` (request.rs:644); wire `textDocument/foldingRange`
- Params: `FoldingRangeParams { text_document: TextDocumentIdentifier, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:folding_range.rs:8–17. **No position.**
- Result: `Option<Vec<FoldingRange>>` (request.rs:648).
  - `FoldingRange { start_line: u32, start_character: Option<u32>, end_line: u32, end_character: Option<u32>, kind: Option<FoldingRangeKind>, collapsed_text: Option<String> }` — folding_range.rs:115–145. See Special semantics for the doc quotes. `FoldingRangeKind` (`rename_all = "lowercase"`): `Comment` | `Imports` | `Region` — folding_range.rs:103–112.

### moniker (omni_trait_generated.rs:24)

- Request type: `request::MonikerRequest` (request.rs:824); wire `textDocument/moniker`
- Params: `MonikerParams { #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:moniker.rs:62–73. One incoming position.
- Result: `Option<Vec<Moniker>>` (request.rs:828).
  - `Moniker { scheme: String, identifier: String, unique: UniquenessLevel, kind: Option<MonikerKind> }` — moniker.rs:76–92. `UniquenessLevel` (camelCase): `Document` | `Project` | `Group` | `Scheme` | `Global` — moniker.rs:34–47. `MonikerKind`: `Import` | `Export` | `Local` — moniker.rs:50–60. **No positions/ranges/URIs in the response.**

### code_lens (omni_trait_generated.rs:46)

- Request type: `request::CodeLensRequest` (request.rs:525); wire `textDocument/codeLens`
- Params: `CodeLensParams { text_document: TextDocumentIdentifier, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:code_lens.rs:20–31. No position.
- Result: `Option<Vec<CodeLens>>` (request.rs:529).
  - `CodeLens { range: Range, command: Option<Command>, data: Option<Value> }` — code_lens.rs:38–52. Doc: "The range in which this code lens is valid. Should only span a single line." Outgoing range conversion.

### code_lens_resolve (omni_trait_generated.rs:47)

- Request type: `request::CodeLensResolve` (request.rs:536); wire `codeLens/resolve`
- Params: `CodeLens` — code_lens.rs:38–52 (carries `range: Range` → **incoming** range conversion; `data: Option<Value>` round-trips through the resolve).
- Result: `CodeLens` (request.rs:540) → outgoing range conversion.

### linked_editing_range (omni_trait_generated.rs:20)

- Request type: `request::LinkedEditingRange` (request.rs:602); wire `textDocument/linkedEditingRange`
- Params: `LinkedEditingRangeParams { #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams }` — LT:linked_editing.rs:39–47. One incoming position.
- Result: `Option<LinkedEditingRanges>` (request.rs:606).
  - `LinkedEditingRanges { ranges: Vec<Range>, word_pattern: Option<String> }` — linked_editing.rs:49–61. Doc: "A list of ranges that can be renamed together. The ranges must have identical length and contain identical text content. The ranges cannot overlap."

### on_type_formatting (omni_trait_generated.rs:52)

- Request type: `request::OnTypeFormatting` (request.rs:588); wire `textDocument/onTypeFormatting`
- Params: `DocumentOnTypeFormattingParams { #[serde(flatten)] text_document_position: TextDocumentPositionParams, ch: String, options: FormattingOptions }` — LT:formatting.rs:90–102. One incoming position. **Note: no `work_done_progress_params` field at all** (deviation #8). `FormattingOptions { tab_size: u32, insert_spaces: bool, #[serde(flatten)] properties: HashMap<String, FormattingProperty>, trim_trailing_whitespace: Option<bool>, insert_final_newline: Option<bool>, trim_final_newlines: Option<bool> }` — formatting.rs:40–64; `FormattingProperty` (untagged): `Bool(bool)` | `Number(i32)` | `String(String)` — formatting.rs:66–72.
- Result: `Option<Vec<TextEdit>>` (request.rs:592). Outgoing range conversion on each edit.

### document_color (omni_trait_generated.rs:9)

- Request type: `request::DocumentColor` (request.rs:623); wire `textDocument/documentColor`
- Params: `DocumentColorParams { text_document: TextDocumentIdentifier, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:color.rs:50–61. No position.
- Result: `Vec<ColorInformation>` — **not `Option`** (request.rs:627; deviation #6).
  - `ColorInformation { range: Range, color: Color }` — color.rs:63–70. `Color { red: f32, green: f32, blue: f32, alpha: f32 }` — color.rs:72–83. Outgoing range conversion.

### color_presentation (omni_trait_generated.rs:10)

- Request type: `request::ColorPresentationRequest` (request.rs:634); wire `textDocument/colorPresentation`
- Params: `ColorPresentationParams { text_document: TextDocumentIdentifier, color: Color, range: Range, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:color.rs:85–102. **Confirmed: carries `range: Range` (incoming conversion needed)** plus `text_document` for fallback conversion context.
- Result: `Vec<ColorPresentation>` — not `Option` (request.rs:638).
  - `ColorPresentation { label: String, text_edit: Option<TextEdit>, additional_text_edits: Option<Vec<TextEdit>> }` — color.rs:104–122. Outgoing range conversion on the edits.

### prepare_call_hierarchy (omni_trait_generated.rs:14)

- Request type: `request::CallHierarchyPrepare` (request.rs:714); wire `textDocument/prepareCallHierarchy`
- Params: `CallHierarchyPrepareParams { #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams }` — LT:call_hierarchy.rs:38–46. One incoming position.
- Result: `Option<Vec<CallHierarchyItem>>` (request.rs:718).
  - `CallHierarchyItem { name: String, kind: SymbolKind, tags: Option<Vec<SymbolTag>>, detail: Option<String>, uri: Url, range: Range, selection_range: Range, data: Option<Value> }` — call_hierarchy.rs:48–78. Outgoing: uri + two ranges.

### incoming_calls (omni_trait_generated.rs:15)

- Request type: `request::CallHierarchyIncomingCalls` (request.rs:722); wire `callHierarchy/incomingCalls`
- Params: `CallHierarchyIncomingCallsParams { item: CallHierarchyItem, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — call_hierarchy.rs:80–90. **Incoming conversion**: the `item` carries `uri` + `range` + `selection_range`.
- Result: `Option<Vec<CallHierarchyIncomingCall>>` (request.rs:726).
  - `CallHierarchyIncomingCall { from: CallHierarchyItem, from_ranges: Vec<Range> }` — call_hierarchy.rs:93–102. Outgoing: whole `from` item + ranges.

### outgoing_calls (omni_trait_generated.rs:16)

- Request type: `request::CallHierarchyOutgoingCalls` (request.rs:730); wire `callHierarchy/outgoingCalls`
- Params: `CallHierarchyOutgoingCallsParams { item: CallHierarchyItem, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — call_hierarchy.rs:104–114. Incoming conversion as above.
- Result: `Option<Vec<CallHierarchyOutgoingCall>>` (request.rs:734).
  - `CallHierarchyOutgoingCall { to: CallHierarchyItem, from_ranges: Vec<Range> }` — call_hierarchy.rs:117–127. (Field is named `from_ranges` on the outgoing call too — per the doc comment the ranges are "relative to the caller".)

### prepare_type_hierarchy (omni_trait_generated.rs:25)

- Request type: `request::TypeHierarchyPrepare` (request.rs:938); wire `textDocument/prepareTypeHierarchy`
- Params: `TypeHierarchyPrepareParams { #[serde(flatten)] text_document_position_params: TextDocumentPositionParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams }` — LT:type_hierarchy.rs:27–33. One incoming position.
- Result: `Option<Vec<TypeHierarchyItem>>` (request.rs:942).
  - `TypeHierarchyItem { name: String, kind: SymbolKind, tags: Option<SymbolTag>, detail: Option<String>, uri: Url, range: Range, selection_range: Range, data: Option<LSPAny> }` — type_hierarchy.rs:55–90. **`tags` is a single `Option<SymbolTag>`, not `Option<Vec<SymbolTag>>`** (deviation #4).

### supertypes (omni_trait_generated.rs:26)

- Request type: `request::TypeHierarchySupertypes` (request.rs:951); wire `typeHierarchy/supertypes`
- Params: `TypeHierarchySupertypesParams { item: TypeHierarchyItem, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — type_hierarchy.rs:35–43. Incoming conversion on `item` (uri + 2 ranges).
- Result: `Option<Vec<TypeHierarchyItem>>` (request.rs:955). Outgoing conversion.

### subtypes (omni_trait_generated.rs:27)

- Request type: `request::TypeHierarchySubtypes` (request.rs:963); wire `typeHierarchy/subtypes`
- Params: `TypeHierarchySubtypesParams { item: TypeHierarchyItem, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — type_hierarchy.rs:45–53. Incoming conversion on `item`.
- Result: `Option<Vec<TypeHierarchyItem>>` (request.rs:967). Outgoing conversion.

### inline_value (omni_trait_generated.rs:28)

- Request type: `request::InlineValueRequest` (request.rs:870); wire `textDocument/inlineValue`
- Params: `InlineValueParams { #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, text_document: TextDocumentIdentifier, range: Range, context: InlineValueContext }` — LT:inline_value.rs:44–59. **Incoming range conversion** (`range` field + `context.stopped_location`).
  - `InlineValueContext { frame_id: i32, stopped_location: Range }` — inline_value.rs:62–72.
- Result: `Option<InlineValue>` — a **single enum value, not a Vec** (request.rs:874; deviation #7). `InlineValue` (untagged): `Text(InlineValueText)` | `VariableLookup(InlineValueVariableLookup)` | `EvaluatableExpression(InlineValueEvaluatableExpression)` — inline_value.rs:138–144. **All three variants carry `range: Range`; none carries a `Position`.**
  - `InlineValueText { range: Range, text: String }` — inline_value.rs:77–84.
  - `InlineValueVariableLookup { range: Range, variable_name: Option<String>, case_sensitive_lookup: bool }` — inline_value.rs:94–108.
  - `InlineValueEvaluatableExpression { range: Range, expression: Option<String> }` — inline_value.rs:118–129.

### inlay_hint (omni_trait_generated.rs:29)

- Request type: `request::InlayHintRequest` (request.rs:834); wire `textDocument/inlayHint`
- Params: `InlayHintParams { #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, text_document: TextDocumentIdentifier, range: Range }` — LT:inlay_hint.rs:66–77. Incoming range conversion.
- Result: `Option<Vec<InlayHint>>` (request.rs:838).
  - `InlayHint { position: Position, label: InlayHintLabel, kind: Option<InlayHintKind>, text_edits: Option<Vec<TextEdit>>, tooltip: Option<InlayHintTooltip>, padding_left: Option<bool>, padding_right: Option<bool>, data: Option<LSPAny> }` — inlay_hint.rs:84–137. Outgoing: `position` + every `TextEdit` range + `label` parts' `location`.
  - `InlayHintLabel` (untagged): `String(String)` | `LabelParts(Vec<InlayHintLabelPart>)` — inlay_hint.rs:139–144.
  - `InlayHintLabelPart { value: String, tooltip: Option<InlayHintLabelPartTooltip>, location: Option<Location>, command: Option<Command> }` — inlay_hint.rs:183–215. `location: Option<Location>` — uri+range conversion.
  - `InlayHintTooltip` (untagged): `String(String)` | `MarkupContent(MarkupContent)` — inlay_hint.rs:160–165. `InlayHintLabelPartTooltip` identical — inlay_hint.rs:217–222.
  - `InlayHintKind` transparent i32: `TYPE = 1`, `PARAMETER = 2` — inlay_hint.rs:241–252.

### inlay_hint_resolve (omni_trait_generated.rs:30)

- Request type: `request::InlayHintResolveRequest` (request.rs:846); wire `inlayHint/resolve`
- Params: `InlayHint` — inlay_hint.rs:84–137. **Incoming conversion** (position, text_edits, label parts' location) — this resolve's params are position-bearing, unlike codeLens/resolve where only a range rides along.
- Result: `InlayHint` (request.rs:850) — outgoing conversion, same fields.

### document_symbol (omni_trait_generated.rs:41)

- Request type: `request::DocumentSymbolRequest` — **not `DocumentSymbol`** (request.rs:408); wire `textDocument/documentSymbol`
- Params: `DocumentSymbolParams { text_document: TextDocumentIdentifier, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams }` — LT:document_symbols.rs:57–68. No position.
- Result: `Option<DocumentSymbolResponse>` (request.rs:412). `DocumentSymbolResponse` (untagged): `Flat(Vec<SymbolInformation>)` | `Nested(Vec<DocumentSymbol>)` — document_symbols.rs:38–43. **`Flat` is declared first**, which is the order serde tries when deserializing.
  - `DocumentSymbol { name: String, detail: Option<String>, kind: SymbolKind, tags: Option<Vec<SymbolTag>>, #[deprecated] deprecated: Option<bool>, range: Range, selection_range: Range, children: Option<Vec<DocumentSymbol>> }` — document_symbols.rs:74–104. Recursive tree; every level carries two ranges.
  - `SymbolInformation { name: String, kind: SymbolKind, tags: Option<Vec<SymbolTag>>, #[deprecated] deprecated: Option<bool>, location: Location, container_name: Option<String> }` — document_symbols.rs:108–134. `Location` = uri + range (lib.rs:277–280).

### symbol (workspace/symbol) (omni_trait_generated.rs:44)

- Request type: `request::WorkspaceSymbolRequest` — not `WorkspaceSymbol` (request.rs:419); wire `workspace/symbol`
- Params: `WorkspaceSymbolParams { #[serde(flatten)] partial_result_params: PartialResultParams, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, query: String }` — LT:workspace_symbols.rs:40–50. **No `text_document`, no position/range.**
- Result: `Option<WorkspaceSymbolResponse>` (request.rs:423). `WorkspaceSymbolResponse` (untagged): `Flat(Vec<SymbolInformation>)` | `Nested(Vec<WorkspaceSymbol>)` — workspace_symbols.rs:100–105 (`Flat` first again).
  - `WorkspaceSymbol { name: String, kind: SymbolKind, tags: Option<Vec<SymbolTag>>, container_name: Option<String>, location: OneOf<Location, WorkspaceLocation>, data: Option<LSPAny> }` — workspace_symbols.rs:62–93. `WorkspaceLocation { uri: Url }` (no range) — workspace_symbols.rs:95–98. Outgoing conversion must handle the `OneOf`: `Left(Location)` needs uri+range conversion; `Right(WorkspaceLocation)` only uri.

### workspace_symbol_resolve (omni_trait_generated.rs:45)

- Request type: `request::WorkspaceSymbolResolve` (request.rs:430); wire `workspaceSymbol/resolve`
- Params: `WorkspaceSymbol` — workspace_symbols.rs:62–93. **Incoming conversion** (the `OneOf<Location, WorkspaceLocation>` rides in the params).
- Result: `WorkspaceSymbol` (request.rs:434) — outgoing conversion.

### will_save_wait_until (omni_trait_generated.rs:33)

- Request type: `request::WillSaveWaitUntil` (request.rs:456); wire `textDocument/willSaveWaitUntil`
- Params: `WillSaveTextDocumentParams { text_document: TextDocumentIdentifier, reason: TextDocumentSaveReason }` — LT:lib.rs:2302–2310. **Confirmed: no position field.** `TextDocumentSaveReason` transparent i32: `MANUAL = 1`, `AFTER_DELAY = 2`, `FOCUS_OUT = 3` — lib.rs:2313–2328.
- Result: `Option<Vec<TextEdit>>` (request.rs:460). Outgoing range conversion. The request's `text_document` gives the conversion document context.

### will_create_files (omni_trait_generated.rs:21)

- Request type: `request::WillCreateFiles` (request.rs:789); wire `workspace/willCreateFiles`
- Params: `CreateFilesParams { files: Vec<FileCreate> }` — LT:file_operations.rs:153–158; `FileCreate { uri: String }` — file_operations.rs:162–167. **`uri` is a plain `String`, not `url::Url`** (deviation #5). No positions.
- Result: `Option<WorkspaceEdit>` (request.rs:793). Outgoing conversion of the whole edit.

### will_rename_files (omni_trait_generated.rs:22)

- Request type: `request::WillRenameFiles` (request.rs:798); wire `workspace/willRenameFiles`
- Params: `RenameFilesParams { files: Vec<FileRename> }` — file_operations.rs:173–179; `FileRename { old_uri: String, new_uri: String }` — file_operations.rs:184–192. Both `String`. No positions.
- Result: `Option<WorkspaceEdit>` (request.rs:802).

### will_delete_files (omni_trait_generated.rs:23)

- Request type: `request::WillDeleteFiles` (request.rs:807); wire `workspace/willDeleteFiles`
- Params: `DeleteFilesParams { files: Vec<FileDelete> }` — file_operations.rs:198–203; `FileDelete { uri: String }` — file_operations.rs:208–213. No positions.
- Result: `Option<WorkspaceEdit>` (request.rs:811).

### execute_command (omni_trait_generated.rs:55)

- Request type: `request::ExecuteCommand` (request.rs:442); wire `workspace/executeCommand`
- Params: `ExecuteCommandParams { command: String, #[serde(default)] arguments: Vec<Value>, #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams }` — LT:lib.rs:2638–2647. **Confirmed: no `TextDocumentIdentifier`, no position, no range — nothing conversion-relevant arrives or leaves.**
- Result: `Option<Value>` (`serde_json::Value`; request.rs:446). Opaque — no position conversion.

### semantic_tokens_full (omni_trait_generated.rs:17)

- Request type: `request::SemanticTokensFullRequest` (request.rs:738); wire `textDocument/semanticTokens/full`
- Params: `SemanticTokensParams { #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams, text_document: TextDocumentIdentifier }` — LT:semantic_tokens.rs:484–495. No position/range.
- Result: `Option<SemanticTokensResult>` (request.rs:742). `SemanticTokensResult` (untagged): `Tokens(SemanticTokens)` | `Partial(SemanticTokensPartialResult)` — semantic_tokens.rs:263–269.
  - `SemanticTokens { result_id: Option<String>, data: Vec<SemanticToken> }` — semantic_tokens.rs:232–250. See Special semantics.
  - `SemanticTokensPartialResult { data: Vec<SemanticToken> }` (no `result_id`) — semantic_tokens.rs:253–261.

### semantic_tokens_full_delta (omni_trait_generated.rs:18)

- Request type: `request::SemanticTokensFullDeltaRequest` (request.rs:746); wire `textDocument/semanticTokens/full/delta`
- Params: `SemanticTokensDeltaParams { #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams, text_document: TextDocumentIdentifier, previous_result_id: String }` — semantic_tokens.rs:497–512.
- Result: `Option<SemanticTokensFullDeltaResult>` (request.rs:750). `SemanticTokensFullDeltaResult` (untagged): `Tokens(SemanticTokens)` | `TokensDelta(SemanticTokensDelta)` | `PartialTokensDelta { edits: Vec<SemanticTokensEdit> }` — semantic_tokens.rs:299–306. **The third variant is an inline struct variant**, not a named type.
  - `SemanticTokensDelta { result_id: Option<String>, edits: Vec<SemanticTokensEdit> }` — semantic_tokens.rs:321–329.
  - `SemanticTokensEdit { start: u32, delete_count: u32, data: Option<Vec<SemanticToken>> }` — semantic_tokens.rs:284–297. See Special semantics — **no `end` field**.

### semantic_tokens_range (omni_trait_generated.rs:19)

- Request type: `request::SemanticTokensRangeRequest` (request.rs:754); wire `textDocument/semanticTokens/range`
- Params: `SemanticTokensRangeParams { #[serde(flatten)] work_done_progress_params: WorkDoneProgressParams, #[serde(flatten)] partial_result_params: PartialResultParams, text_document: TextDocumentIdentifier, range: Range }` — semantic_tokens.rs:514–528. **Incoming range conversion.**
- Result: `Option<SemanticTokensRangeResult>` (request.rs:758). `SemanticTokensRangeResult` (untagged): `Tokens(SemanticTokens)` | `Partial(SemanticTokensPartialResult)` — semantic_tokens.rs:530–536.

---

## Notification params (client→server, for `did_*` forwarding)

Trait-method bindings: `omni_trait_generated.rs:58–74`. Notification impls: `LT:notification.rs` (`DidChangeWatchedFiles` → notification.rs:239–244, `WillSaveTextDocument` → 207–212, `WorkDoneProgressCancel` → 278–283, `DidCreateFiles` → 287–292, `DidRenameFiles` → 296–301, `DidDeleteFiles` → 304–310).

- `DidChangeWatchedFilesParams { changes: Vec<FileEvent> }` — LT:lib.rs:2377–2381. `FileEvent { uri: Url, #[serde(rename = "type")] typ: FileChangeType }` — lib.rs:2401–2409. `FileChangeType` transparent i32: `CREATED = 1`, `CHANGED = 2`, `DELETED = 3` — lib.rs:2384–2398. Note: `FileEvent.uri` **is** `url::Url`, unlike the file-operation item types.
- `WillSaveTextDocumentParams { text_document: TextDocumentIdentifier, reason: TextDocumentSaveReason }` — lib.rs:2302–2310 (same type as `will_save_wait_until` params; also used by the `textDocument/willSave` notification).
- `CreateFilesParams { files: Vec<FileCreate> }` — file_operations.rs:153–158; `FileCreate { uri: String }` — 162–167.
- `RenameFilesParams { files: Vec<FileRename> }` — file_operations.rs:173–179; `FileRename { old_uri: String, new_uri: String }` — 184–192.
- `DeleteFilesParams { files: Vec<FileDelete> }` — file_operations.rs:198–203; `FileDelete { uri: String }` — 208–213.
- `WorkDoneProgressCancelParams { token: ProgressToken }` — LT:progress.rs:36–41; `ProgressToken = NumberOrString` — progress.rs:5.

---

## Special semantics (quoted from source)

### SemanticTokensEdit (semantic_tokens.rs:283–297)

The struct carries **`start` and `delete_count`** (`deleteCount` on the wire via `rename_all = "camelCase"`), not `start`/`end`:

```rust
/// @since 3.16.0
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokensEdit {
    pub start: u32,
    pub delete_count: u32,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "SemanticToken::deserialize_tokens_opt",
        serialize_with = "SemanticToken::serialize_tokens_opt"
    )]
    pub data: Option<Vec<SemanticToken>>,
}
```

**There is no doc comment on the struct or on `start`/`delete_count` in lsp-types 0.95.1.** Whether `start`/`delete_count` index token-stream positions or elements of the flattened `u32` array is **not documented in the source I read**. The only in-source pointer is the URL on `SemanticTokensDelta.edits` (semantic_tokens.rs:326–327) to the vscode `semantic-tokens-sample/vscode.proposed.d.ts#L131` sample, which I did not read. What the source does pin: on the wire `data` is a flat integer array — the custom serde (`deserialize_tokens_opt`/`serialize_tokens_opt`, semantic_tokens.rs:195–228) converts it through `Vec<SemanticToken>` 5-tuples and errors on a length not divisible by 5 (semantic_tokens.rs:161–165).

### SemanticTokens data layout (semantic_tokens.rs:145–250)

The 5-tuple layout is enforced by the serde impl, not by a doc comment: `SemanticToken { delta_line: u32, delta_start: u32, length: u32, token_type: u32, token_modifiers_bitset: u32 }` (semantic_tokens.rs:146–153), chunked via `data.chunks_exact(5)` with mapping `chunk[0]→delta_line, chunk[1]→delta_start, chunk[2]→length, chunk[3]→token_type, chunk[4]→token_modifiers_bitset` (semantic_tokens.rs:160–177); serialization writes the same order (semantic_tokens.rs:180–193). Round-trip test in-source: `{"data":[2,5,3,0,3]}` ⇄ one `SemanticToken` (semantic_tokens.rs:565–577).

The struct's doc comment says only:

```rust
    /// The actual tokens. For a detailed description about how the data is
    /// structured please see
    /// <https://github.com/microsoft/vscode-extension-samples/blob/5ae1f7787122812dcc84e37427ca90af5ee09f14/semantic-tokens-sample/vscode.proposed.d.ts#L71>
```

**The source does not state that `delta_start`/`length` are counted in the negotiated position encoding.** The negotiated-encoding language exists only on `Position::character` (lib.rs:245–250, quoted in the shared-types preamble). Encoding semantics of semantic-token offsets must come from the LSP spec itself, which is not vendored here; I cannot verify it from the files read.

### WorkspaceEdit (lib.rs:702–734)

```rust
/// A workspace edit represents changes to many resources managed in the workspace.
/// The edit should either provide `changes` or `documentChanges`.
/// If the client can handle versioned document edits and if `documentChanges` are present,
/// the latter are preferred over `changes`.
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    /// Holds changes to existing resources.
    #[serde(with = "url_map")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub changes: Option<HashMap<Url, Vec<TextEdit>>>, //    changes?: { [uri: string]: TextEdit[]; };

    /// Depending on the client capability `workspace.workspaceEdit.resourceOperations` document changes
    /// are either an array of `TextDocumentEdit`s … Or it can contain
    /// above `TextDocumentEdit`s mixed with create, rename and delete file / folder operations.
    /// …
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_changes: Option<DocumentChanges>,

    /// A map of change annotations that can be referenced in
    /// `AnnotatedTextEdit`s or create, rename and delete file / folder
    /// operations.
    /// …
    /// @since 3.16.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_annotations: Option<HashMap<ChangeAnnotationIdentifier, ChangeAnnotation>>,
}
```

- `changes: Option<HashMap<Url, Vec<TextEdit>>>` — keyed by `Url` through the `url_map` serde helper (`with = "url_map"`, lib.rs:706).
- `document_changes: Option<DocumentChanges>` — untagged `Edits(Vec<TextDocumentEdit>)` | `Operations(Vec<DocumentChangeOperation>)` (lib.rs:738–741); `DocumentChangeOperation` untagged `Op(ResourceOp)` | `Edit(TextDocumentEdit)` (lib.rs:760–763); `ResourceOp` internally-tagged (`kind`: create/rename/delete) wrapping `CreateFile`/`RenameFile`/`DeleteFile` (lib.rs:767–771; item types at lib.rs:624–636, 653–667, 690–696).
- `change_annotations: Option<HashMap<ChangeAnnotationIdentifier, ChangeAnnotation>>` with `ChangeAnnotationIdentifier = String` (lib.rs:545, 733).
- Positions live in every `TextEdit`/`AnnotatedTextEdit`/`TextDocumentEdit` — full recursive outgoing conversion (and incoming for `will_*` responses → these are outgoing-only; `WorkspaceEdit` appears in results, not params, for all four request types listed here).

### FoldingRange character semantics (folding_range.rs:114–145)

Field doc comments, verbatim:

```rust
    /// The zero-based line number from where the folded range starts.
    pub start_line: u32,

    /// The zero-based character offset from where the folded range starts. If not defined, defaults to the length of the start line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_character: Option<u32>,

    /// The zero-based line number where the folded range ends.
    pub end_line: u32,

    /// The zero-based character offset before the folded range ends. If not defined, defaults to the length of the end line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_character: Option<u32>,
```

So: `start_line`/`end_line` are **non-Option** `u32`; `start_character`/`end_character` are **`Option<u32>`**; `kind: Option<FoldingRangeKind>`; `collapsed_text: Option<String>` (3.17). The doc says "zero-based character offset" without naming an encoding; the client cap `line_folding_only` doc (folding_range.rs:84–87) mentions start/end character being ignored when set, nothing about encoding.

### ParameterInformation::label (signature_help.rs:186–207)

```rust
pub struct ParameterInformation {
    /// The label of this parameter information.
    ///
    /// Either a string or an inclusive start and exclusive end offsets within its containing
    /// signature label. (see SignatureInformation.label). *Note*: A label of type string must be
    /// a substring of its containing signature label.
    pub label: ParameterLabel,
    …
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ParameterLabel {
    Simple(String),
    LabelOffsets([u32; 2]),
}
```

Shape: `String` or `[u32; 2]` (inclusive start, exclusive end) offsets into the containing `SignatureInformation.label` **string** — i.e. offsets into a `String`, not into a document. **The source does not state what the offsets count (UTF-16 code units vs negotiated encoding)** — that detail is not present in lsp-types 0.95.1.

### DocumentSymbolResponse (document_symbols.rs:38–43) and WorkspaceSymbolResponse (workspace_symbols.rs:100–105)

```rust
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentSymbolResponse {
    Flat(Vec<SymbolInformation>),
    Nested(Vec<DocumentSymbol>),
}
```

```rust
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceSymbolResponse {
    Flat(Vec<SymbolInformation>),
    Nested(Vec<WorkspaceSymbol>),
}
```

`Flat` (the `SymbolInformation` shape with `Location`) precedes `Nested` in both; both are `#[serde(untagged)]`. The nested shapes carry ranges only (`DocumentSymbol.range`/`selection_range`, recursive `children`); `WorkspaceSymbol.location` is the `OneOf<Location, WorkspaceLocation>` described above.

### InlayHint position-bearing fields (inlay_hint.rs:84–215)

- `position: Position` (non-Option) — inlay_hint.rs:85–86.
- `text_edits: Option<Vec<TextEdit>>` — inlay_hint.rs:99–108 ("Optional text edits that are performed when accepting this inlay hint.").
- `label.location` — `InlayHintLabelPart.location: Option<Location>` — inlay_hint.rs:195–207 ("An optional source code location that represents this label part."). Reached only when `label` is `InlayHintLabel::LabelParts(Vec<InlayHintLabelPart>)` (inlay_hint.rs:139–144).
- `InlayHintParams.range: Range` — inlay_hint.rs:75–76 (incoming).
- Everything else (`tooltip`, `padding_*`, `data`, `kind`) carries no positions.

### CallHierarchyItem / TypeHierarchyItem (call_hierarchy.rs:48–78; type_hierarchy.rs:55–90)

Position/URI-bearing fields, identical in both items: `uri: Url` (non-Option), `range: Range`, `selection_range: Range` (both non-Option), plus `data` (opaque, preserved across prepare → calls/supertypes/subtypes). Differences: `CallHierarchyItem.tags: Option<Vec<SymbolTag>>` and `data: Option<Value>`; `TypeHierarchyItem.tags: Option<SymbolTag>` (single) and `data: Option<LSPAny>` (= `serde_json::Value`, so no runtime difference for `data`). Call wrappers additionally carry `CallHierarchyIncomingCall.from_ranges: Vec<Range>` / `CallHierarchyOutgoingCall.from_ranges: Vec<Range>`.

### ExecuteCommandParams / WillSaveTextDocumentParams — absence of positions

- `ExecuteCommandParams { command: String, arguments: Vec<Value>, work_done_token }` (lib.rs:2638–2647): no `TextDocumentIdentifier` at all, no position, no range, no URI. Only opaque `serde_json::Value` arguments.
- `WillSaveTextDocumentParams { text_document, reason }` (lib.rs:2302–2310): document identifier + save reason, **no position**.

### ColorPresentationParams carries a Range (color.rs:85–102)

Confirmed: `range: Range` (non-Option, doc: "The range where the color would be inserted. Serves as a context.") plus `color: Color` and `text_document: TextDocumentIdentifier`. Incoming conversion needed for `range`; `ColorPresentation.text_edit`/`additional_text_edits` need outgoing conversion.

### SelectionRange linked list (selection_range.rs:76–86)

```rust
/// Represents a selection range.
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRange {
    /// Range of the selection.
    pub range: Range,

    /// The parent selection range containing this range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Box<SelectionRange>>,
}
```

Confirmed: `range` + `parent: Option<Box<SelectionRange>>`, recursive; the root's `parent` is `None`.

### LinkedEditingRanges response (linked_editing.rs:49–61)

Confirmed: `LinkedEditingRanges { ranges: Vec<Range>, word_pattern: Option<String> }` — flat list of ranges plus an optional regex; no nesting.

---

## Deviations from expectations

1. **`SemanticTokensEdit` has no `end` field.** It is `{ start: u32, delete_count: u32 (wire `deleteCount`), data: Option<Vec<SemanticToken>> }` (semantic_tokens.rs:284–297). Any conversion spec written against `start`/`end` must be rewritten against `start`/`deleteCount`.
2. **No doc comments exist on `SemanticTokensEdit` or its fields** (semantic_tokens.rs:283–297), so the requested quote of "what start/end index" could not be produced: the source does not say. The only in-source pointer is a vscode-sample URL on `SemanticTokensDelta.edits` (semantic_tokens.rs:326–327), not read here.
3. **`SemanticTokens`' doc comment does not mention encoding semantics.** It only links the vscode sample (semantic_tokens.rs:241–244). The 5-tuple layout is enforced by the custom serde impl (semantic_tokens.rs:146–228), but "deltaStartChar/length are in the negotiated position encoding" is **not verifiable from lsp-types source**; the negotiated-encoding sentence exists only on `Position::character` (lib.rs:245–250).
4. **`TypeHierarchyItem.tags` is `Option<SymbolTag>` (single tag), not `Option<Vec<SymbolTag>>`** (type_hierarchy.rs:64–66) — differs from `CallHierarchyItem` (call_hierarchy.rs:57–59), `DocumentSymbol`, and `SymbolInformation`, which all use `Vec<SymbolTag>`.
5. **File-operation item URIs are plain `String`, not `url::Url`:** `FileCreate.uri` (file_operations.rs:164–166), `FileRename.old_uri`/`new_uri` (file_operations.rs:186–191), `FileDelete.uri` (file_operations.rs:210–212). Contrast `FileEvent.uri: Url` (lib.rs:2402–2404) and `TextDocumentIdentifier.uri: Url`.
6. **`document_color` and `color_presentation` results are bare `Vec`s, not `Option<Vec<…>>`:** `Vec<ColorInformation>` (request.rs:627), `Vec<ColorPresentation>` (request.rs:638). Response post-processing must handle the non-optional shape.
7. **`inline_value` result is `Option<InlineValue>` — a single enum value, not a Vec** (request.rs:874). All three variants carry a `Range`; none carries a bare `Position`.
8. **`DocumentOnTypeFormattingParams` has no `work_done_progress_params`** — only flattened `TextDocumentPositionParams`, `ch`, `options` (formatting.rs:90–102). Every other params type in this set carries `WorkDoneProgressParams`.
9. **`SemanticTokensFullDeltaResult`'s third variant is an inline struct variant** `PartialTokensDelta { edits: Vec<SemanticTokensEdit> }` (semantic_tokens.rs:305), not a named struct — pattern-matching code must handle it as a struct variant.
10. **`ParameterLabel` offsets' counting unit is not documented in lsp-types source** — the doc says only "inclusive start and exclusive end offsets within its containing signature label" (signature_help.rs:189–193). UTF-16-specific wording does not appear.
11. `documentSymbol` request type is `DocumentSymbolRequest` and `workspace/symbol` is `WorkspaceSymbolRequest` — as anticipated in the brief; listing for completeness since both were flagged as likely quirks.
12. Nothing else in the brief's assumptions was contradicted; every requested type above was found. Items I could not verify from the read sources are stated in place rather than guessed (semantic-token offset encoding, `SemanticTokensEdit.start` indexing target, `ParameterLabel` offset unit).
