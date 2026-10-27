//! The tables a PDF's structure tree declares (Tagged PDF, ISO 32000-2
//! 14.8): the only evidence Edit PDF finds tables by.
//!
//! A page's marked content is tied to the tree through the page's
//! `/StructParents` and the tree's `/ParentTree`, which lists the element
//! each marked-content identifier belongs to. A table is a `Table` element;
//! its rows are `TR` elements, directly or inside `THead`, `TBody` and
//! `TFoot`; its cells are their `TH` and `TD` elements, spanning the rows
//! and columns their `/Table` attributes give.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};
use serde::{Deserialize, Serialize};

/// How many elements are read for one page's tables, and how far the tree
/// is followed: bounds for malformed or hostile files.
const MAX_ELEMENTS: usize = 100_000;
const MAX_DEPTH: usize = 64;
/// The most rows or columns a declared table is laid out with.
const MAX_GRID: usize = 10_000;

/// A table the structure tree declares, as far as it is on one page.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TaggedTable {
    /// Its rows on the page, in the tree's order, and its columns.
    pub row_count: usize,
    pub column_count: usize,
    /// By row, then column.
    pub cells: Vec<TaggedCell>,
    /// Its `Table` element and its rows' `TR` elements, as object numbers
    /// and generations.
    #[serde(default)]
    pub element: Option<(u32, u16)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_elements: Vec<(u32, u16)>,
    /// Some of it is on another page: rows, or content of its cells.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub continues: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TaggedCell {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    /// The page's marked content the cell holds, ascending.
    pub marked_content_ids: Vec<i64>,
    /// Its `TH` or `TD` element.
    #[serde(default)]
    pub element: Option<(u32, u16)>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub header: bool,
}

fn resolve<'a>(document: &'a Document, mut object: &'a Object) -> Option<&'a Object> {
    for _ in 0..MAX_DEPTH {
        match object {
            Object::Reference(id) => object = document.get_object(*id).ok()?,
            _ => return Some(object),
        }
    }
    None
}

fn dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(document, object)?.as_dict().ok()
}

fn integer(document: &Document, object: &Object) -> Option<i64> {
    match resolve(document, object)? {
        Object::Integer(value) => Some(*value),
        _ => None,
    }
}

fn kids<'a>(document: &'a Document, element: &'a Dictionary) -> Vec<&'a Object> {
    match element
        .get(b"K")
        .ok()
        .and_then(|kids| resolve(document, kids))
    {
        Some(Object::Array(kids)) => kids.iter().collect(),
        Some(_) => element.get(b"K").ok().into_iter().collect(),
        None => Vec::new(),
    }
}

/// The structure tree's elements a page's tables are read from.
struct Tree<'a> {
    document: &'a Document,
    role_map: Option<&'a Dictionary>,
    read: usize,
}

impl<'a> Tree<'a> {
    fn element(&mut self, id: ObjectId) -> Option<&'a Dictionary> {
        self.read += 1;
        if self.read > MAX_ELEMENTS {
            return None;
        }
        self.document.get_object(id).ok()?.as_dict().ok()
    }

    /// An element's standard type: its `/S`, through the tree's `/RoleMap`.
    fn kind(&self, element: &Dictionary) -> Option<Vec<u8>> {
        let mut name = element.get(b"S").ok()?.as_name().ok()?.to_vec();
        for _ in 0..MAX_DEPTH {
            let Some(mapped) = self
                .role_map
                .and_then(|map| map.get(&name).ok())
                .and_then(|object| object.as_name().ok())
            else {
                break;
            };
            if mapped == name.as_slice() {
                break;
            }
            name = mapped.to_vec();
        }
        Some(name)
    }

    fn parent(&mut self, id: ObjectId) -> Option<(ObjectId, &'a Dictionary)> {
        let element = self.element(id)?;
        let parent = element.get(b"P").ok()?.as_reference().ok()?;
        Some((parent, self.element(parent)?))
    }

    /// Whether cell `cell` holds any marked content, and whether any is on
    /// a page other than `page` (an `MCR`'s `/Pg`, else the nearest
    /// element's).
    fn content_pages(&mut self, cell: ObjectId, page: ObjectId) -> (bool, bool) {
        let mut any = false;
        let mut pending = vec![(cell, None::<ObjectId>)];
        while let Some((id, inherited)) = pending.pop() {
            let Some(element) = self.element(id) else {
                return (any, true);
            };
            let on = element
                .get(b"Pg")
                .ok()
                .and_then(|pg| pg.as_reference().ok())
                .or(inherited);
            for kid in kids(self.document, element) {
                let reference = match kid {
                    Object::Integer(_) => {
                        any = true;
                        if on.is_some_and(|on| on != page) {
                            return (true, true);
                        }
                        continue;
                    }
                    Object::Reference(child) => match self.document.get_object(*child) {
                        Ok(Object::Dictionary(child_dictionary)) if child_dictionary.has(b"S") => {
                            pending.push((*child, on));
                            continue;
                        }
                        Ok(Object::Dictionary(reference)) => reference,
                        _ => continue,
                    },
                    Object::Dictionary(reference) => reference,
                    _ => continue,
                };
                if let Some(pg) = marked_reference_page(reference, on) {
                    any = true;
                    if pg != page {
                        return (true, true);
                    }
                } else if reference.has(b"MCID") {
                    any = true;
                }
            }
        }
        (any, false)
    }

    /// The element kids of `element`, with their types.
    fn child_elements(&mut self, element: &'a Dictionary) -> Vec<(ObjectId, Vec<u8>)> {
        let mut children = Vec::new();
        for kid in kids(self.document, element) {
            let Ok(id) = kid.as_reference() else {
                continue;
            };
            let Some(child) = self.element(id) else {
                continue;
            };
            if let Some(kind) = self.kind(child) {
                children.push((id, kind));
            }
        }
        children
    }

    /// A cell's row and column spans, from its `/Table` attributes.
    fn spans(&self, cell: &Dictionary) -> (usize, usize) {
        let mut spans = (1, 1);
        let attributes = match cell.get(b"A").ok().and_then(|a| resolve(self.document, a)) {
            Some(Object::Array(list)) => list.iter().collect::<Vec<_>>(),
            Some(object) => vec![object],
            None => Vec::new(),
        };
        for attribute in attributes {
            let Some(attribute) = dictionary(self.document, attribute) else {
                continue;
            };
            if attribute.get(b"O").ok().and_then(|o| o.as_name().ok()) != Some(b"Table") {
                continue;
            }
            let span = |key: &[u8]| {
                attribute
                    .get(key)
                    .ok()
                    .and_then(|value| integer(self.document, value))
                    .filter(|value| *value >= 1)
                    .map(|value| (value as usize).min(MAX_GRID))
            };
            if let Some(rows) = span(b"RowSpan") {
                spans.0 = rows;
            }
            if let Some(columns) = span(b"ColSpan") {
                spans.1 = columns;
            }
        }
        spans
    }

    /// A table's rows, each its `TR` and its cells in order, with their
    /// spans and whether each is a header cell.
    fn rows(&mut self, table: &'a Dictionary) -> Vec<(ObjectId, Vec<Cell>)> {
        let mut rows = Vec::new();
        let mut row_ids = Vec::new();
        for (id, kind) in self.child_elements(table) {
            match kind.as_slice() {
                b"TR" => row_ids.push(id),
                b"THead" | b"TBody" | b"TFoot" => {
                    if let Some(section) = self.element(id) {
                        row_ids.extend(
                            self.child_elements(section)
                                .into_iter()
                                .filter(|(_, kind)| kind == b"TR")
                                .map(|(id, _)| id),
                        );
                    }
                }
                _ => {}
            }
        }
        for row in row_ids.into_iter().take(MAX_GRID) {
            let Some(element) = self.element(row) else {
                continue;
            };
            let cells = self
                .child_elements(element)
                .into_iter()
                .filter(|(_, kind)| kind == b"TH" || kind == b"TD")
                .filter_map(|(id, kind)| {
                    let cell = self.element(id)?;
                    let (row_span, column_span) = self.spans(cell);
                    Some(Cell {
                        id,
                        row_span,
                        column_span,
                        header: kind == b"TH",
                    })
                })
                .collect();
            rows.push((row, cells));
        }
        rows
    }
}

/// A table cell as the tree lays it out.
#[derive(Clone, Copy)]
struct Cell {
    id: ObjectId,
    row_span: usize,
    column_span: usize,
    header: bool,
}

/// The page a marked-content reference (`MCR`) is on: its `/Pg`, else the
/// page its element is on. Object references (`OBJR`) are not content.
fn marked_reference_page(reference: &Dictionary, inherited: Option<ObjectId>) -> Option<ObjectId> {
    if !reference.has(b"MCID") {
        return None;
    }
    reference
        .get(b"Pg")
        .ok()
        .and_then(|pg| pg.as_reference().ok())
        .or(inherited)
}

/// The element of each of a page's marked-content identifiers, from the
/// structure tree's `/ParentTree`.
fn marked_content_elements(
    document: &Document,
    root: &Dictionary,
    page: &Dictionary,
) -> Vec<(i64, ObjectId)> {
    let Some(key) = page
        .get(b"StructParents")
        .ok()
        .and_then(|key| integer(document, key))
    else {
        return Vec::new();
    };
    let Some(tree) = root
        .get(b"ParentTree")
        .ok()
        .and_then(|tree| dictionary(document, tree))
    else {
        return Vec::new();
    };
    let Some(Object::Array(parents)) =
        number_tree_value(document, tree, key, 0).and_then(|value| resolve(document, value))
    else {
        return Vec::new();
    };
    parents
        .iter()
        .enumerate()
        .filter_map(|(id, parent)| Some((id as i64, parent.as_reference().ok()?)))
        .collect()
}

/// The value number tree `node` keeps for `key`.
fn number_tree_value<'a>(
    document: &'a Document,
    node: &'a Dictionary,
    key: i64,
    depth: usize,
) -> Option<&'a Object> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Some(Object::Array(entries)) = node.get(b"Nums").ok().and_then(|n| resolve(document, n))
    {
        for pair in entries.chunks(2) {
            if let [entry, value] = pair
                && integer(document, entry) == Some(key)
            {
                return Some(value);
            }
        }
    }
    let Some(Object::Array(children)) = node.get(b"Kids").ok().and_then(|k| resolve(document, k))
    else {
        return None;
    };
    for child in children {
        let Some(child) = dictionary(document, child) else {
            continue;
        };
        if let Some(Object::Array(limits)) =
            child.get(b"Limits").ok().and_then(|l| resolve(document, l))
            && let [low, high] = limits.as_slice()
            && let (Some(low), Some(high)) = (integer(document, low), integer(document, high))
            && !(low..=high).contains(&key)
        {
            continue;
        }
        if let Some(value) = number_tree_value(document, child, key, depth + 1) {
            return Some(value);
        }
    }
    None
}

/// The tables page `page_id`'s structure declares, with each cell's marked
/// content on the page. A page without a structure tree has none.
pub fn page_tagged_tables(document: &Document, page_id: ObjectId) -> Vec<TaggedTable> {
    let Some(root) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"StructTreeRoot").ok())
        .and_then(|root| dictionary(document, root))
    else {
        return Vec::new();
    };
    let Some(page) = document
        .get_object(page_id)
        .ok()
        .and_then(|page| page.as_dict().ok())
    else {
        return Vec::new();
    };
    let mut tree = Tree {
        document,
        role_map: root
            .get(b"RoleMap")
            .ok()
            .and_then(|map| dictionary(document, map)),
        read: 0,
    };
    // Each identifier's innermost cell, and each cell's table.
    let mut cell_content: BTreeMap<ObjectId, Vec<i64>> = BTreeMap::new();
    let mut tables: Vec<ObjectId> = Vec::new();
    for (id, element) in marked_content_elements(document, root, page) {
        let mut current = element;
        let mut cell = None;
        for _ in 0..MAX_DEPTH {
            let Some(dictionary) = tree.element(current) else {
                break;
            };
            match tree.kind(dictionary).as_deref() {
                Some(b"TD" | b"TH") if cell.is_none() => cell = Some(current),
                Some(b"Table") => {
                    if let Some(cell) = cell {
                        cell_content.entry(cell).or_default().push(id);
                        if !tables.contains(&current) {
                            tables.push(current);
                        }
                    }
                    break;
                }
                _ => {}
            }
            let Some((parent, _)) = tree.parent(current) else {
                break;
            };
            current = parent;
        }
    }
    tables
        .into_iter()
        .filter_map(|table| {
            let element = tree.element(table)?;
            let rows = tree.rows(element);
            let mut empty = HashSet::new();
            let mut elsewhere = HashSet::new();
            for cell in rows.iter().flat_map(|(_, cells)| cells) {
                match tree.content_pages(cell.id, page_id) {
                    (false, _) => {
                        empty.insert(cell.id);
                    }
                    (true, true) => {
                        elsewhere.insert(cell.id);
                    }
                    (true, false) => {}
                }
            }
            let mut laid = laid_out(&rows, &cell_content, &empty)?;
            laid.element = Some(table);
            laid.continues = laid.continues
                || laid
                    .cells
                    .iter()
                    .filter_map(|cell| cell.element)
                    .any(|cell| elsewhere.contains(&cell));
            Some(laid)
        })
        .collect()
}

/// A table's cells placed on its grid (a cell takes the first free column
/// in its row), cut to the rows holding the page's content and the rows
/// around them with no content anywhere (`empty` cells only).
fn laid_out(
    rows: &[(ObjectId, Vec<Cell>)],
    content: &BTreeMap<ObjectId, Vec<i64>>,
    empty: &HashSet<ObjectId>,
) -> Option<TaggedTable> {
    let mut occupied: HashSet<(usize, usize)> = HashSet::new();
    let mut placed = Vec::new();
    let mut column_count = 0;
    for (row, (_, cells)) in rows.iter().enumerate() {
        let mut column = 0;
        for cell in cells {
            while occupied.contains(&(row, column)) {
                column += 1;
            }
            let row_span = cell.row_span.min(rows.len() - row);
            if column + cell.column_span > MAX_GRID {
                return None;
            }
            for r in row..row + row_span {
                for c in column..column + cell.column_span {
                    occupied.insert((r, c));
                }
            }
            placed.push((*cell, row, column, row_span));
            column += cell.column_span;
            column_count = column_count.max(column);
        }
    }
    let with_content = placed
        .iter()
        .filter(|(cell, ..)| content.contains_key(&cell.id))
        .flat_map(|(_, row, _, row_span)| *row..*row + *row_span)
        .collect::<BTreeSet<_>>();
    let (Some(&first), Some(&last)) = (with_content.first(), with_content.last()) else {
        return None;
    };
    let row_empty = |row: usize| {
        placed
            .iter()
            .filter(|(_, start, _, span)| (*start..*start + *span).contains(&row))
            .all(|(cell, ..)| empty.contains(&cell.id))
    };
    let (mut first, mut last) = (first, last);
    while first > 0 && row_empty(first - 1) {
        first -= 1;
    }
    while last + 1 < rows.len() && row_empty(last + 1) {
        last += 1;
    }
    let on_page = (first..=last).collect::<BTreeSet<_>>();
    let index = on_page.iter().copied().collect::<Vec<_>>();
    let mut cells = placed
        .into_iter()
        .filter_map(|(cell, row, column, row_span)| {
            let rows_here = (row..row + row_span)
                .filter(|row| on_page.contains(row))
                .collect::<Vec<_>>();
            let first = *rows_here.first()?;
            Some(TaggedCell {
                row: index.binary_search(&first).ok()?,
                column,
                row_span: rows_here.len(),
                column_span: cell.column_span,
                marked_content_ids: content.get(&cell.id).cloned().unwrap_or_default(),
                element: Some(cell.id),
                header: cell.header,
            })
        })
        .collect::<Vec<_>>();
    cells.sort_by_key(|cell| (cell.row, cell.column));
    Some(TaggedTable {
        row_count: index.len(),
        column_count,
        cells,
        element: None,
        row_elements: index.iter().map(|row| rows[*row].0).collect(),
        continues: first > 0 || last + 1 < rows.len(),
    })
}

/// Where a page's entry in the structure tree's `/ParentTree` is kept, to
/// change it.
enum ParentSlot {
    /// An array object of its own.
    Object(ObjectId),
    /// Inline in a number tree leaf's `/Nums`, at this index: the leaf, or
    /// its `/Nums` array, is the object `holder`; `in_root` when the leaf is
    /// the tree root inline in the structure tree root.
    Inline {
        holder: ObjectId,
        nums_object: bool,
        in_root: bool,
        index: usize,
    },
}

fn parent_slot(document: &Document, key: i64) -> Option<ParentSlot> {
    let root_id = document
        .catalog()
        .ok()?
        .get(b"StructTreeRoot")
        .ok()?
        .as_reference()
        .ok()?;
    let root = document.get_object(root_id).ok()?.as_dict().ok()?;
    let tree = root.get(b"ParentTree").ok()?;
    let (mut node_id, mut in_root) = match tree {
        Object::Reference(id) => (*id, false),
        _ => (root_id, true),
    };
    for _ in 0..MAX_DEPTH {
        let node = if in_root {
            root.get(b"ParentTree").ok()?.as_dict().ok()?
        } else {
            document.get_object(node_id).ok()?.as_dict().ok()?
        };
        if let Ok(nums) = node.get(b"Nums") {
            let (entries, holder, nums_object) = match nums {
                Object::Reference(id) => {
                    (document.get_object(*id).ok()?.as_array().ok()?, *id, true)
                }
                Object::Array(entries) => (entries, node_id, false),
                _ => return None,
            };
            for (pair, chunk) in entries.chunks(2).enumerate() {
                if let [entry, value] = chunk
                    && integer(document, entry) == Some(key)
                {
                    return Some(match value {
                        Object::Reference(id) => ParentSlot::Object(*id),
                        _ => ParentSlot::Inline {
                            holder,
                            nums_object,
                            in_root: in_root && !nums_object,
                            index: pair * 2 + 1,
                        },
                    });
                }
            }
        }
        let Some(Object::Array(children)) =
            node.get(b"Kids").ok().and_then(|k| resolve(document, k))
        else {
            return None;
        };
        let next = children.iter().find_map(|child| {
            let id = child.as_reference().ok()?;
            let child = document.get_object(id).ok()?.as_dict().ok()?;
            match child.get(b"Limits").ok().and_then(|l| resolve(document, l)) {
                Some(Object::Array(limits)) => match limits.as_slice() {
                    [low, high]
                        if integer(document, low)? <= key && key <= integer(document, high)? =>
                    {
                        Some(id)
                    }
                    _ => None,
                },
                _ => Some(id),
            }
        })?;
        node_id = next;
        in_root = false;
    }
    None
}

/// Changes page `page_id`'s `/ParentTree` array, which maps its
/// marked-content identifiers to their elements.
fn with_page_parents<T>(
    document: &mut Document,
    page_id: ObjectId,
    change: impl FnOnce(&mut Vec<Object>) -> T,
) -> Result<T, String> {
    let key = document
        .get_object(page_id)
        .ok()
        .and_then(|page| page.as_dict().ok())
        .and_then(|page| page.get(b"StructParents").ok())
        .and_then(|key| integer(document, key))
        .ok_or("the page has no structure parents")?;
    let slot = parent_slot(document, key).ok_or("the structure tree has no entry for the page")?;
    let array = match slot {
        ParentSlot::Object(id) => document
            .get_object_mut(id)
            .ok()
            .and_then(|o| o.as_array_mut().ok()),
        ParentSlot::Inline {
            holder,
            nums_object,
            in_root,
            index,
        } => {
            let object = document.get_object_mut(holder).ok();
            let nums = if nums_object {
                object.and_then(|o| o.as_array_mut().ok())
            } else if in_root {
                object
                    .and_then(|o| o.as_dict_mut().ok())
                    .and_then(|root| root.get_mut(b"ParentTree").ok())
                    .and_then(|tree| tree.as_dict_mut().ok())
                    .and_then(|tree| tree.get_mut(b"Nums").ok())
                    .and_then(|nums| nums.as_array_mut().ok())
            } else {
                object
                    .and_then(|o| o.as_dict_mut().ok())
                    .and_then(|node| node.get_mut(b"Nums").ok())
                    .and_then(|nums| nums.as_array_mut().ok())
            };
            nums.and_then(|nums| nums.get_mut(index))
                .and_then(|value| value.as_array_mut().ok())
        }
    };
    let array = array.ok_or("the page's structure parents are not an array")?;
    Ok(change(array))
}

/// The page's next free marked-content identifier.
pub fn next_marked_content_id(document: &mut Document, page_id: ObjectId) -> Option<i64> {
    with_page_parents(document, page_id, |parents| parents.len() as i64).ok()
}

/// Element `id`'s kids as an array to change: a single kid becomes an
/// array of one, and kids kept in an array object are changed there.
fn kids_mut(document: &mut Document, id: ObjectId) -> Result<&mut Vec<Object>, String> {
    let kids_object = document
        .get_object(id)
        .ok()
        .and_then(|element| element.as_dict().ok())
        .and_then(|element| element.get(b"K").ok())
        .and_then(|kids| kids.as_reference().ok())
        .filter(|kids| matches!(document.get_object(*kids), Ok(Object::Array(_))));
    if let Some(kids) = kids_object {
        return document
            .get_object_mut(kids)
            .and_then(Object::as_array_mut)
            .map_err(|error| error.to_string());
    }
    let element = document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|error| error.to_string())?;
    let kids = match element.remove(b"K") {
        Some(Object::Array(kids)) => kids,
        Some(kid) => vec![kid],
        None => Vec::new(),
    };
    element.set("K", Object::Array(kids));
    element
        .get_mut(b"K")
        .and_then(Object::as_array_mut)
        .map_err(|error| error.to_string())
}

fn new_element(document: &mut Document, kind: &str, parent: ObjectId, page: ObjectId) -> ObjectId {
    document.add_object(dictionary! {
        "Type" => "StructElem",
        "S" => kind,
        "P" => parent,
        "Pg" => page,
        "K" => Object::Array(Vec::new()),
    })
}

/// Where each row or column of an edited table comes from: a base row or
/// column, or a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Base(usize),
    New,
}

/// Makes table `tagged`'s structure match its edit on page `page_id`: its
/// rows and columns as `rows` and `columns` list them (base bands left out
/// are deleted, with their content), and new cell text `cell_texts` (row,
/// column, marked-content identifier, in the edited table) in its cells.
/// A table with merged cells, or that continues on another page, keeps its
/// rows and columns.
pub fn retag_table(
    document: &mut Document,
    page_id: ObjectId,
    tagged: &TaggedTable,
    rows: &[Band],
    columns: &[Band],
    cell_texts: &[(usize, usize, i64)],
) -> Result<(), String> {
    let unchanged = |bands: &[Band], count: usize| {
        bands.len() == count
            && bands
                .iter()
                .enumerate()
                .all(|(i, band)| *band == Band::Base(i))
    };
    let restructured =
        !(unchanged(rows, tagged.row_count) && unchanged(columns, tagged.column_count));
    if !restructured && cell_texts.is_empty() {
        return Ok(());
    }
    tagged.element.ok_or("the table has no structure element")?;
    let cell = |row: usize, column: usize| {
        tagged
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
    };
    // New cells, by their place in the edited table.
    let mut created: BTreeMap<(usize, usize), ObjectId> = BTreeMap::new();
    if restructured {
        if tagged.continues {
            return Err("the table continues on another page".into());
        }
        if tagged
            .cells
            .iter()
            .any(|cell| cell.row_span != 1 || cell.column_span != 1)
        {
            return Err("the table has merged cells".into());
        }
        if tagged.row_elements.len() != tagged.row_count
            || tagged.cells.len() != tagged.row_count * tagged.column_count
            || tagged.cells.iter().any(|cell| cell.element.is_none())
        {
            return Err("the table's structure does not match its rows and columns".into());
        }
        // The rows' container: the table, or a section all of them are in.
        let parents = tagged
            .row_elements
            .iter()
            .map(|row| {
                document
                    .get_object(*row)
                    .ok()
                    .and_then(|row| row.as_dict().ok())
                    .and_then(|row| row.get(b"P").ok())
                    .and_then(|parent| parent.as_reference().ok())
            })
            .collect::<BTreeSet<_>>();
        let container = match parents.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(container)] => *container,
            _ => return Err("the table's rows are in different sections".into()),
        };
        let base_rows = tagged.row_elements.clone();
        let mut row_ids = Vec::new();
        let mut removed_ids = Vec::new();
        for (output_row, band) in rows.iter().enumerate() {
            let row_id = match band {
                Band::Base(row) => *base_rows.get(*row).ok_or("a row is out of range")?,
                Band::New => {
                    let id = new_element(document, "TR", container, page_id);
                    created.insert((output_row, usize::MAX), id);
                    id
                }
            };
            let mut cell_ids = Vec::new();
            for (output_column, column) in columns.iter().enumerate() {
                let id = match (band, column) {
                    (Band::Base(row), Band::Base(column)) => cell(*row, *column)
                        .and_then(|cell| cell.element)
                        .ok_or("a cell is out of range")?,
                    _ => {
                        // Drawn like its row's or column's model: a header
                        // cell when the base cell it copies is one.
                        let header = match (band, column) {
                            (Band::Base(row), _) => {
                                tagged.cells.iter().any(|c| c.row == *row && c.header)
                                    && tagged
                                        .cells
                                        .iter()
                                        .filter(|c| c.row == *row)
                                        .all(|c| c.header)
                            }
                            (_, Band::Base(column)) => tagged
                                .cells
                                .iter()
                                .filter(|c| c.column == *column)
                                .all(|c| c.header),
                            _ => false,
                        };
                        let kind = if header { "TH" } else { "TD" };
                        let id = new_element(document, kind, row_id, page_id);
                        created.insert((output_row, output_column), id);
                        id
                    }
                };
                cell_ids.push(id);
            }
            // The row's other kids stay after its cells.
            let cell_set = tagged
                .cells
                .iter()
                .filter_map(|cell| cell.element)
                .collect::<HashSet<_>>();
            let kids = kids_mut(document, row_id)?;
            let others = kids
                .iter()
                .filter(|kid| {
                    kid.as_reference()
                        .map_or(true, |id| !cell_set.contains(&id))
                })
                .cloned()
                .collect::<Vec<_>>();
            *kids = cell_ids
                .iter()
                .copied()
                .map(Object::Reference)
                .chain(others)
                .collect();
            for id in &cell_ids {
                if let Ok(element) = document.get_object_mut(*id).and_then(Object::as_dict_mut) {
                    element.set("P", Object::Reference(row_id));
                }
            }
            row_ids.push(row_id);
        }
        // Deleted rows and columns take their cells' content with them.
        for cell in &tagged.cells {
            let kept =
                rows.contains(&Band::Base(cell.row)) && columns.contains(&Band::Base(cell.column));
            if !kept {
                removed_ids.extend(cell.marked_content_ids.iter().copied());
            }
        }
        // The container's rows on this page, in their new order, where its
        // first was.
        let base_set = base_rows.iter().copied().collect::<HashSet<_>>();
        let kids = kids_mut(document, container)?;
        let first = kids
            .iter()
            .position(|kid| kid.as_reference().is_ok_and(|id| base_set.contains(&id)))
            .ok_or("the table's rows are not in its structure")?;
        kids.retain(|kid| {
            kid.as_reference()
                .map_or(true, |id| !base_set.contains(&id))
        });
        let first = first.min(kids.len());
        kids.splice(first..first, row_ids.iter().copied().map(Object::Reference));
        with_page_parents(document, page_id, |parents| {
            for id in removed_ids {
                if let Some(slot) = usize::try_from(id).ok().and_then(|id| parents.get_mut(id)) {
                    *slot = Object::Null;
                }
            }
        })?;
    }
    // New text belongs to its cell.
    for &(row, column, id) in cell_texts {
        let element = match created.get(&(row, column)) {
            Some(element) => *element,
            None => {
                let (Some(Band::Base(row)), Some(Band::Base(column))) =
                    (rows.get(row), columns.get(column))
                else {
                    return Err("new text is outside the table".into());
                };
                cell(*row, *column)
                    .and_then(|cell| cell.element)
                    .ok_or("new text's cell has no structure element")?
            }
        };
        kids_mut(document, element)?.push(Object::Dictionary(dictionary! {
            "Type" => "MCR",
            "Pg" => page_id,
            "MCID" => id,
        }));
        with_page_parents(document, page_id, |parents| {
            let index = id as usize;
            if parents.len() <= index {
                parents.resize(index + 1, Object::Null);
            }
            parents[index] = Object::Reference(element);
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(id: u32, row_span: usize, column_span: usize) -> Cell {
        Cell {
            id: (id, 0),
            row_span,
            column_span,
            header: false,
        }
    }

    fn row(id: u32, cells: Vec<Cell>) -> (ObjectId, Vec<Cell>) {
        ((id, 0), cells)
    }

    #[test]
    fn cells_take_the_first_free_column_past_spans_from_rows_above() {
        let rows = vec![
            row(11, vec![cell(1, 2, 1), cell(2, 1, 2)]),
            row(12, vec![cell(3, 1, 1), cell(4, 1, 1)]),
        ];
        let content = [1, 2, 3, 4]
            .map(|id| ((id, 0), vec![i64::from(id)]))
            .into_iter()
            .collect();
        let table = laid_out(&rows, &content, &HashSet::new()).unwrap();
        assert_eq!((table.row_count, table.column_count), (2, 3));
        let places = table
            .cells
            .iter()
            .map(|cell| (cell.row, cell.column, cell.row_span, cell.column_span))
            .collect::<Vec<_>>();
        assert_eq!(
            places,
            [(0, 0, 2, 1), (0, 1, 1, 2), (1, 1, 1, 1), (1, 2, 1, 1)]
        );
    }

    #[test]
    fn rows_and_columns_of_a_table_that_continues_or_merges_cells_do_not_change() {
        let mut document = Document::with_version("1.7");
        let mut table = TaggedTable {
            row_count: 1,
            column_count: 1,
            cells: vec![TaggedCell {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 1,
                marked_content_ids: vec![0],
                element: Some((2, 0)),
                header: false,
            }],
            element: Some((1, 0)),
            row_elements: vec![(3, 0)],
            continues: true,
        };
        let added = [Band::Base(0), Band::New];
        let error = retag_table(&mut document, (4, 0), &table, &added, &[Band::Base(0)], &[]);
        assert_eq!(error.unwrap_err(), "the table continues on another page");
        table.continues = false;
        table.cells[0].row_span = 2;
        let error = retag_table(&mut document, (4, 0), &table, &added, &[Band::Base(0)], &[]);
        assert_eq!(error.unwrap_err(), "the table has merged cells");
        // Unchanged rows and columns with no new text change nothing.
        let same = retag_table(
            &mut document,
            (4, 0),
            &table,
            &[Band::Base(0)],
            &[Band::Base(0)],
            &[],
        );
        assert!(same.is_ok());
    }

    #[test]
    fn a_page_holds_only_the_rows_with_its_content() {
        let rows = vec![
            row(11, vec![cell(1, 1, 1), cell(2, 1, 1)]),
            row(12, vec![cell(3, 1, 1), cell(4, 1, 1)]),
            row(13, vec![cell(5, 1, 1), cell(6, 1, 1)]),
        ];
        // Only the last two rows' content is on this page; cell 4 is empty.
        let content = [(3, vec![7]), (5, vec![8]), (6, vec![9])]
            .map(|(id, ids)| ((id, 0), ids))
            .into_iter()
            .collect();
        let table = laid_out(&rows, &content, &HashSet::new()).unwrap();
        assert_eq!(table.row_count, 2);
        assert_eq!(table.row_elements, [(12, 0), (13, 0)]);
        assert!(table.continues);
        assert_eq!(table.cells.len(), 4);
        assert_eq!(table.cells[1].marked_content_ids, Vec::<i64>::new());
        assert_eq!(table.cells[3].marked_content_ids, [9]);
        assert!(laid_out(&rows, &BTreeMap::new(), &HashSet::new()).is_none());
    }
}
