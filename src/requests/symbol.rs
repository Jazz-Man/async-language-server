use std::collections::HashMap;

use async_lsp::lsp_types::{
    Location as LspLocation, OneOf, Url, WorkspaceSymbolParams as LspWorkspaceSymbolParams,
    WorkspaceSymbolResponse as LspWorkspaceSymbolResponse,
};

use crate::server::{Document, ServerState, read_document_from_disk};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub(crate) struct Symbol;

impl Request for Symbol {
    type Params = LspWorkspaceSymbolParams;
    type Response = Option<LspWorkspaceSymbolResponse>;

    // extract_url stays the default (`None`): the params carry only the
    // query (plus work-done/partial tokens), so there is no request URL and
    // no staleness tracking. modify_params stays the default: nothing in the
    // params is conversion-relevant.

    // State-driven shape: no single engine-resolved document plays a role,
    // because each location resolves against its own document below. The
    // standalone hook is overridden INSTEAD of `modify_response`; the trait
    // default of the latter delegates here, so dispatch runs it in every
    // state, including the sole-tracked-document fallback.
    fn modify_response_standalone(state: &ServerState, response: &mut Self::Response) {
        let Some(response) = response else { return };
        let mut disk: HashMap<Url, Option<Document>> = HashMap::new();
        match response {
            LspWorkspaceSymbolResponse::Flat(symbols) => {
                for symbol in symbols {
                    convert_symbol_location(state, &mut disk, &mut symbol.location);
                }
            }
            LspWorkspaceSymbolResponse::Nested(symbols) => {
                for symbol in symbols {
                    if let OneOf::Left(location) = &mut symbol.location {
                        convert_symbol_location(state, &mut disk, location);
                    }
                    // `Right(WorkspaceLocation)` carries no range — nothing
                    // to convert.
                }
            }
        }
    }
}

/// Converts one symbol location's range: against the tracked document for
/// its URL (store-first), else against a per-request disk snapshot (cached
/// in `disk`, so N symbols in one file cost a single read), else left
/// unconverted.
fn convert_symbol_location(
    state: &ServerState,
    disk: &mut HashMap<Url, Option<Document>>,
    location: &mut LspLocation,
) {
    let uri = location.uri.clone();
    if let Some(document) = state.document(&uri) {
        convert_range(state, &document, &mut location.range, Direction::Outgoing);
        return;
    }
    let document = disk
        .entry(uri.clone())
        .or_insert_with(|| read_document_from_disk(&uri));
    if let Some(document) = document {
        convert_range(state, document, &mut location.range, Direction::Outgoing);
    }
}

#[cfg(test)]
#[allow(
    deprecated,
    reason = "fixtures construct `SymbolInformation`, whose upstream `deprecated` \
              field is marked `#[deprecated]` yet still required in struct literals"
)]
mod tests {
    use std::fs;

    use async_lsp::lsp_types::{
        Location as LspLocation, OneOf, SymbolInformation, SymbolKind, Url, WorkspaceLocation,
        WorkspaceSymbol, WorkspaceSymbolResponse,
    };

    use crate::requests::Request;
    use crate::testing::{same_line, state_with_documents, temp_workspace};

    use super::Symbol;

    #[test]
    fn symbol_flat_locations_convert_tracked_from_disk_or_pass_through() {
        let (state, _plain, emoji) = state_with_documents();

        // Tracked: store-first converts against the tracked snapshot.
        let mut tracked = Some(WorkspaceSymbolResponse::Flat(vec![SymbolInformation {
            name: "t".into(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: LspLocation {
                uri: emoji.clone(),
                range: same_line(0, 4, 5),
            },
            container_name: None,
        }]));
        <Symbol as Request>::modify_response_standalone(&state, &mut tracked);
        let WorkspaceSymbolResponse::Flat(symbols) = tracked.expect("response present") else {
            panic!("expected flat symbols");
        };
        assert_eq!(symbols[0].location.range, same_line(0, 2, 3));

        // Untracked but on disk: the per-request fallback reads the file.
        // "x🙂🙂" maps byte 1 to UTF-16 1, where the tracked document maps
        // byte 1 to UTF-16 0 (floored into its emoji), so a fallback
        // against the wrong text fails here.
        let root = temp_workspace("requests", "symbol");
        let on_disk = root.join("sym.txt");
        fs::write(&on_disk, "x🙂🙂").expect("temp file can be written");
        let disk_uri = Url::from_file_path(&on_disk).expect("path converts to a URL");
        let mut disk = Some(WorkspaceSymbolResponse::Flat(vec![SymbolInformation {
            name: "d".into(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: LspLocation {
                uri: disk_uri,
                range: same_line(0, 1, 5),
            },
            container_name: None,
        }]));
        <Symbol as Request>::modify_response_standalone(&state, &mut disk);
        let WorkspaceSymbolResponse::Flat(symbols) = disk.expect("response present") else {
            panic!("expected flat symbols");
        };
        assert_eq!(symbols[0].location.range, same_line(0, 1, 3));

        // Nonexistent file: passes through unchanged.
        let missing_uri =
            Url::from_file_path(root.join("missing.txt")).expect("path converts to a URL");
        let mut missing = Some(WorkspaceSymbolResponse::Flat(vec![SymbolInformation {
            name: "m".into(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: LspLocation {
                uri: missing_uri,
                range: same_line(0, 4, 5),
            },
            container_name: None,
        }]));
        <Symbol as Request>::modify_response_standalone(&state, &mut missing);
        let WorkspaceSymbolResponse::Flat(symbols) = missing.expect("response present") else {
            panic!("expected flat symbols");
        };
        assert_eq!(symbols[0].location.range, same_line(0, 4, 5));

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    #[test]
    fn symbol_nested_left_converts_and_right_passes_through() {
        let (state, _plain, emoji) = state_with_documents();
        let mut response = Some(WorkspaceSymbolResponse::Nested(vec![
            WorkspaceSymbol {
                name: "l".into(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                container_name: None,
                location: OneOf::Left(LspLocation {
                    uri: emoji.clone(),
                    range: same_line(0, 4, 5),
                }),
                data: None,
            },
            WorkspaceSymbol {
                name: "r".into(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                container_name: None,
                location: OneOf::Right(WorkspaceLocation { uri: emoji.clone() }),
                data: None,
            },
        ]));

        <Symbol as Request>::modify_response_standalone(&state, &mut response);

        let WorkspaceSymbolResponse::Nested(symbols) = response.expect("response present") else {
            panic!("expected nested symbols");
        };
        let OneOf::Left(location) = &symbols[0].location else {
            panic!("expected a left location");
        };
        assert_eq!(location.range, same_line(0, 2, 3));
        assert_eq!(
            symbols[1].location,
            OneOf::Right(WorkspaceLocation { uri: emoji })
        );
    }
}
