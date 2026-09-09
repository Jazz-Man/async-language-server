//! Helpers for working with tree-sitter syntax trees in an LSP context.
//!
//! All conversions between tree-sitter and LSP coordinates assume UTF-8
//! positions, matching the crate-wide invariant.

use std::collections::VecDeque;

// lsp_types::Position/Range — aliased against the crate's own Position (below) and tree-sitter's Range (next line).
use async_lsp::lsp_types::{Position as LspPosition, Range as LspRange};
// tree_sitter::Point/Range — aliased against lsp_types' Range; Point follows the same Ts... convention.
use tree_sitter::{Node, Point as TsPoint, Range as TsRange};

pub use crate::error::QueryError;

use crate::text_utils::Position;

/// Converts a tree sitter `Point` to an LSP [`Position`](LspPosition).
///
/// # LSP Compatibility
///
/// This function assumes that the returned LSP [`Position`](LspPosition) is one that will
/// be used with this language server, using UTF-8 encoding specifically.
#[must_use]
pub fn ts_point_to_lsp_position(pos: TsPoint) -> LspPosition {
    LspPosition {
        line: u32::try_from(pos.row).unwrap_or(u32::MAX),
        character: u32::try_from(pos.column).unwrap_or(u32::MAX),
    }
}

/// Converts a tree sitter `Range` to an LSP `Range`.
///
/// # LSP Compatibility
///
/// This function assumes that the returned LSP `Range` is one that will
/// be used with this language server, using UTF-8 encoding specifically.
#[must_use]
pub fn ts_range_to_lsp_range(range: TsRange) -> LspRange {
    LspRange {
        start: ts_point_to_lsp_position(range.start_point),
        end: ts_point_to_lsp_position(range.end_point),
    }
}

/// Returns `true` if the given tree sitter `Range`
/// contains the given LSP [`Position`](LspPosition), otherwise `false`.
///
/// This is an **inclusive** bounds check, meaning the position is
/// considered *inside* even if it lies on a line or column boundary
///
/// # LSP Compatibility
///
/// This function assumes that the given LSP [`Position`](LspPosition) is one returned
/// by this language server, using UTF-8 encoding specifically. Using
/// any other encoding **will** return an invalid result here.
#[must_use]
pub const fn ts_range_contains_lsp_position(range: TsRange, pos: LspPosition) -> bool {
    let point = lsp_position_to_ts_point(pos);
    ts_range_contains_ts_point(range, point)
}

/// Returns `true` if the given tree sitter `Range`
/// contains the given tree sitter `Point`, otherwise `false`.
///
/// This is an **inclusive** bounds check, meaning the point is
/// considered *inside* even if it lies on a line or column boundary
#[must_use]
pub const fn ts_range_contains_ts_point(range: TsRange, point: TsPoint) -> bool {
    (point.row > range.start_point.row
        || point.row == range.start_point.row && point.column >= range.start_point.column)
        && (point.row < range.end_point.row
            || point.row == range.end_point.row && point.column <= range.end_point.column)
}

/// Converts an LSP [`Position`](LspPosition) to a tree sitter `Point`.
///
/// # LSP Compatibility
///
/// This function assumes that the given LSP [`Position`](LspPosition) is one returned
/// by this language server, using UTF-8 encoding specifically. Using
/// any other encoding **will** return an invalid result here.
#[must_use]
pub const fn lsp_position_to_ts_point(pos: LspPosition) -> TsPoint {
    TsPoint {
        row: pos.line as usize,
        column: pos.character as usize,
    }
}

/// Finds the first child node that matches the given predicate.
#[must_use]
pub fn find_child<'a, F>(node: Node<'a>, predicate: F) -> Option<Node<'a>>
where
    F: Fn(Node<'a>) -> bool,
{
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|child| predicate(*child))
}

/// Finds the first ancestor node that matches the given predicate.
#[must_use]
pub fn find_ancestor<'a, F>(node: Node<'a>, predicate: F) -> Option<Node<'a>>
where
    F: Fn(Node<'a>) -> bool,
{
    let mut current = node.parent();

    while let Some(node) = current {
        if predicate(node) {
            return Some(node);
        }
        current = node.parent();
    }

    None
}

/// Finds the first descendant node that matches the given predicate.
///
/// This will search descendants in a breadth-first manner.
#[must_use]
pub fn find_descendant<'a, F>(node: Node<'a>, predicate: F) -> Option<Node<'a>>
where
    F: Fn(Node<'a>) -> bool,
{
    let mut cursor = node.walk();
    let mut stack = VecDeque::from([node]);

    while let Some(current) = stack.pop_front() {
        if predicate(current) {
            return Some(current);
        }
        for child in current.children(&mut cursor) {
            stack.push_back(child);
        }
    }

    None
}

/// Finds the nearest node at `pos` that also matches the given predicate.
///
/// 1. If the given node itself matches the predicate, returns the node
/// 2. If the given node has a child that matches the predicate, returns the child
/// 3. If the given node has a descendant that matches the predicate, returns the descendant
/// 4. If the given node has an ancestor that matches the predicate, returns the ancestor
///
/// Note that this uses **inclusive** bounds checks, meaning that points
/// are considered *inside* even if they lie on a line or column boundary
pub fn find_nearest<'a, F>(
    node: Node<'a>,
    pos: impl Into<Position>,
    predicate: F,
) -> Option<Node<'a>>
where
    F: Fn(Node<'a>) -> bool,
{
    find_nearest_inner(node, pos.into().into_lsp(), predicate)
}

// NOTE: We split this into an "inner" function to get slightly
// better compile times with the `impl Into<Position>` generic

fn find_nearest_inner<'a, F>(node: Node<'a>, pos: LspPosition, predicate: F) -> Option<Node<'a>>
where
    F: Fn(Node<'a>) -> bool,
{
    // Make sure that we are actually inside this node, first of all ...
    if ts_range_contains_lsp_position(node.range(), pos) {
        // We are inside the node, check it
        if predicate(node) {
            return Some(node);
        }
        // Node is not of kind, check children + descendants + ancestors
        // This may do some redundant work for descendants, but unfortunately
        // there is no easy way to find node depth and skip the direct children
        find_child(node, |child| {
            ts_range_contains_lsp_position(child.range(), pos) && predicate(child)
        })
        .or_else(|| {
            find_descendant(node, |descendant| {
                ts_range_contains_lsp_position(descendant.range(), pos) && predicate(descendant)
            })
        })
        .or_else(|| find_ancestor(node, &predicate))
    } else {
        // We are not inside the node, but an ancestor may still match the position
        find_ancestor(node, |ancestor| {
            ts_range_contains_lsp_position(ancestor.range(), pos) && predicate(ancestor)
        })
    }
}

#[cfg(test)]
mod tests {
    use tree_sitter::{Point, Range};

    use super::ts_range_contains_ts_point;

    const fn p(row: usize, column: usize) -> Point {
        Point { row, column }
    }

    const fn r(start_point: Point, end_point: Point) -> Range {
        Range {
            start_byte: 0,
            end_byte: 0,
            start_point,
            end_point,
        }
    }

    #[test]
    fn range_contains_multiline_points_lexicographically() {
        let range = r(p(1, 5), p(3, 2));

        assert!(ts_range_contains_ts_point(range, p(1, 5)));
        assert!(ts_range_contains_ts_point(range, p(2, 0)));
        assert!(ts_range_contains_ts_point(range, p(3, 2)));
        assert!(!ts_range_contains_ts_point(range, p(1, 4)));
        assert!(!ts_range_contains_ts_point(range, p(3, 3)));
    }

    use async_lsp::lsp_types::{Position as LspPosition, Range as LspRange};
    use tree_sitter::{Node, Parser, Tree};

    use super::{
        find_ancestor, find_child, find_descendant, find_nearest, lsp_position_to_ts_point,
        ts_point_to_lsp_position, ts_range_contains_lsp_position, ts_range_to_lsp_range,
    };
    use crate::text_utils::Position;

    /// Parses `text` with the JSON grammar — the established dev-dependency
    /// fixture — for the navigation tests.
    fn json_tree(text: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_json::LANGUAGE.into())
            .expect("json grammar loads");
        parser.parse(text, None).expect("json source parses")
    }

    fn id(node: Option<Node<'_>>) -> Option<usize> {
        node.map(|found| found.id())
    }

    #[test]
    fn ts_point_and_range_convert_to_lsp_coordinates() {
        let point = LspPosition {
            line: 1,
            character: 5,
        };
        assert_eq!(ts_point_to_lsp_position(p(1, 5)), point);

        let expected = LspRange {
            start: LspPosition {
                line: 1,
                character: 5,
            },
            end: LspPosition {
                line: 3,
                character: 2,
            },
        };
        assert_eq!(ts_range_to_lsp_range(r(p(1, 5), p(3, 2))), expected);
    }

    #[test]
    fn lsp_position_containment_matches_ts_point_containment() {
        let range = r(p(1, 5), p(3, 2));
        let inside = LspPosition {
            line: 2,
            character: 0,
        };
        let start = LspPosition {
            line: 1,
            character: 5,
        };
        let end = LspPosition {
            line: 3,
            character: 2,
        };
        let before = LspPosition {
            line: 1,
            character: 4,
        };
        let after = LspPosition {
            line: 3,
            character: 3,
        };

        for pos in [inside, start, before, after] {
            assert_eq!(
                ts_range_contains_lsp_position(range, pos),
                ts_range_contains_ts_point(range, lsp_position_to_ts_point(pos)),
                "LSP containment must match point containment at {pos:?}",
            );
        }

        // Inclusive bounds: the start and end corners are inside.
        assert!(ts_range_contains_lsp_position(range, start));
        assert!(ts_range_contains_lsp_position(range, end));
        assert!(!ts_range_contains_lsp_position(range, before));
        assert!(!ts_range_contains_lsp_position(range, after));
    }

    #[test]
    fn find_child_ancestor_descendant_traverse_as_documented() {
        let tree = json_tree(r#"{"aa": 1, "b": 2}"#);
        let root = tree.root_node();
        let object = root.child(0).expect("the document's object child");
        let pair = object.child(1).expect("the object's first pair");
        let string = pair.named_child(0).expect("the pair's key string");
        let number = pair.named_child(1).expect("the pair's value number");

        // find_child: direct children only — the object is found, the pairs
        // (grandchildren) are not.
        assert_eq!(
            id(find_child(root, |n| n.kind() == "object")),
            Some(object.id()),
        );
        assert_eq!(
            id(find_child(root, |n| n.kind() == "pair")),
            None,
            "pairs are grandchildren of the document node",
        );

        // find_ancestor: parents upward, never the node itself.
        assert_eq!(
            id(find_ancestor(string, |n| n.kind() == "pair")),
            Some(pair.id()),
        );
        assert_eq!(
            id(find_ancestor(pair, |n| n.kind() == "document")),
            Some(root.id()),
        );
        assert_eq!(
            id(find_ancestor(root, |_| true)),
            None,
            "the root has no ancestors",
        );

        // find_descendant: matches below the node.
        assert_eq!(
            id(find_descendant(root, |n| n.kind() == "pair")),
            Some(pair.id()),
        );
        assert_eq!(
            id(find_descendant(pair, |n| n.kind() == "number")),
            Some(number.id()),
        );
        assert_eq!(
            id(find_descendant(string, |n| n.kind() == "number")),
            None,
            "a string has no numeric descendants",
        );
    }

    #[test]
    fn find_nearest_prefers_node_then_child_then_descendant_then_ancestor() {
        let tree = json_tree(r#"{"aa": 1, "b": 2}"#);
        let root = tree.root_node();
        let object = root.child(0).expect("the document's object child");
        let pair = object.child(1).expect("the object's first pair");
        let string = pair.named_child(0).expect("the pair's key string");
        let second_pair = object.child(3).expect("the object's second pair");
        let other_string = second_pair
            .named_child(0)
            .expect("the second pair's key string");

        // ASCII source: byte columns on line 0 equal byte offsets.
        let at = |col: usize| Position { line: 0, col };

        // 1. The node itself wins when it matches, over any matching child.
        assert_eq!(
            id(find_nearest(object, at(2), |n| n.kind() == "object")),
            Some(object.id()),
        );
        assert_eq!(
            id(find_nearest(object, at(2), |_| true)),
            Some(object.id()),
            "the containing node beats a matching child",
        );

        // 2. Otherwise a position-containing direct child, over a matching
        //    deeper descendant.
        assert_eq!(
            id(find_nearest(object, at(2), |n| n.kind() != "object")),
            Some(pair.id()),
            "the containing child beats a matching descendant",
        );

        // 3. Otherwise a position-containing descendant: the pair holds the
        //    position but fails this predicate, the key string passes both.
        assert_eq!(
            id(find_nearest(object, at(2), |n| n.kind() == "string")),
            Some(string.id()),
        );

        // 4. Otherwise an ancestor: the second key string does not hold the
        //    value position; its pair holds it but fails the predicate, so
        //    the object answers.
        assert_eq!(
            id(find_nearest(other_string, at(15), |n| n.kind() == "object")),
            Some(object.id()),
        );
    }
}
