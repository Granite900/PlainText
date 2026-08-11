//! Locate and rewrite `tilemap(...)` + `<name>_tiles` dictionary text in a `.pt` file.
//!
//! Pure string surgery (no Raylib). Used by `plaintext edit_tilemap`.

use std::collections::BTreeMap;

/// Where `solid_tiles` lives in the source, so Save can update the same place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolidLoc {
    /// Inside `name = tilemap(..., solid_tiles: [...])`
    InTilemap,
    /// On `world.add_tilemap(name, solid_tiles: [...])`
    InAddTilemap,
    /// Not present yet — Save will add it to the tilemap call.
    Missing,
}

#[derive(Debug, Clone)]
pub struct TilemapDoc {
    pub map_name: String,
    pub rows: Vec<String>,
    pub solid_tiles: Vec<char>,
    pub solid_loc: SolidLoc,
    /// Character → image path (from `<name>_tiles` dictionary).
    pub tiles: BTreeMap<char, String>,
}

impl TilemapDoc {
    pub fn width(&self) -> usize {
        self.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0)
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    pub fn tile_at(&self, col: usize, row: usize) -> char {
        self.rows
            .get(row)
            .and_then(|r| r.chars().nth(col))
            .unwrap_or('.')
    }

    pub fn set_tile(&mut self, col: usize, row: usize, ch: char) {
        if row >= self.rows.len() {
            return;
        }
        let width = self.width().max(col + 1);
        let mut chars: Vec<char> = self.rows[row].chars().collect();
        while chars.len() < width {
            chars.push('.');
        }
        if col < chars.len() {
            chars[col] = ch;
        }
        self.rows[row] = chars.into_iter().collect();
    }

    pub fn set_solid(&mut self, ch: char, solid: bool) {
        if solid {
            if !self.solid_tiles.contains(&ch) {
                self.solid_tiles.push(ch);
            }
        } else {
            self.solid_tiles.retain(|c| *c != ch);
        }
    }

    pub fn is_solid(&self, ch: char) -> bool {
        self.solid_tiles.contains(&ch)
    }

    /// Pad every row with `.` so they all reach the current width.
    fn normalize_width(&mut self) {
        let w = self.width();
        for row in &mut self.rows {
            let len = row.chars().count();
            if len < w {
                row.extend(std::iter::repeat('.').take(w - len));
            }
        }
    }

    /// Append a column of `.` on the right.
    pub fn add_column(&mut self) -> bool {
        self.normalize_width();
        for row in &mut self.rows {
            row.push('.');
        }
        true
    }

    /// Drop the rightmost column, keeping at least one.
    pub fn remove_column(&mut self) -> bool {
        if self.width() <= 1 {
            return false;
        }
        self.normalize_width();
        for row in &mut self.rows {
            row.pop();
        }
        true
    }

    /// Append an empty row at the bottom.
    pub fn add_row(&mut self) -> bool {
        let w = self.width().max(1);
        self.rows.push(std::iter::repeat('.').take(w).collect());
        true
    }

    /// Drop the bottom row, keeping at least one.
    pub fn remove_row(&mut self) -> bool {
        if self.rows.len() <= 1 {
            return false;
        }
        self.rows.pop();
        true
    }
}

/// Starter snippet inserted when the file has no tilemap yet.
pub fn starter_block() -> &'static str {
    "\nlevel = tilemap(cell_size: 32, rows: [\n    \"########\",\n    \"#......#\",\n    \"#..P...#\",\n    \"#......#\",\n    \"########\",\n], solid_tiles: [\"#\"])\n\nlevel_tiles = dictionary { }\n"
}

/// Insert a starter `level` tilemap after the last `import` line (or at the top).
pub fn insert_starter(src: &str) -> String {
    let mut offset_after_imports = 0usize;
    let mut pos = 0usize;
    for line in src.lines() {
        let line_len = line.len();
        let has_nl = pos + line_len < src.len();
        let next = pos + line_len + if has_nl { 1 } else { 0 };
        if line.trim_start().starts_with("import ") {
            offset_after_imports = next;
        }
        pos = next;
    }
    let mut out = String::new();
    out.push_str(&src[..offset_after_imports]);
    if offset_after_imports > 0 && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(starter_block());
    out.push_str(&src[offset_after_imports..]);
    out
}

pub fn load_from_source(src: &str) -> Result<TilemapDoc, String> {
    let (map_name, call_inner) = find_tilemap_assignment(src)?;
    let rows = parse_rows_arg(call_inner)?;
    if rows.is_empty() {
        return Err("tilemap rows can't be empty".into());
    }

    let (solid_tiles, solid_loc) = load_solids(src, &map_name, call_inner)?;
    let tiles = load_tiles_dict(src, &map_name)?;

    Ok(TilemapDoc {
        map_name,
        rows,
        solid_tiles,
        solid_loc,
        tiles,
    })
}

/// Rewrite `src` so it matches `doc`. Returns the new file text.
pub fn apply_to_source(src: &str, doc: &TilemapDoc) -> Result<String, String> {
    let mut out = src.to_string();

    // 1) Replace rows: [...] inside the tilemap call.
    out = replace_tilemap_rows(&out, &doc.map_name, &doc.rows)?;

    // 2) Replace / insert solid_tiles.
    out = rewrite_solids(&out, doc)?;

    // 3) Replace / insert <name>_tiles dictionary.
    out = rewrite_tiles_dict(&out, doc)?;

    Ok(out)
}

fn find_tilemap_assignment(src: &str) -> Result<(String, &str), String> {
    // Walk by char so we never slice mid–UTF-8 (comments may contain `—`, etc.).
    for (i, _) in src.char_indices() {
        if let Some((name, call_start)) = match_ident_eq_tilemap(src, i) {
            let open = call_start; // index of '('
            let close = find_matching(src, open, '(', ')')
                .ok_or_else(|| "tilemap(...) is missing a closing `)`".to_string())?;
            let inner = &src[open + 1..close];
            return Ok((name, inner));
        }
    }
    Err("no `name = tilemap(...)` found in this file".into())
}

fn match_ident_eq_tilemap(src: &str, start: usize) -> Option<(String, usize)> {
    if !src.is_char_boundary(start) {
        return None;
    }
    // Only try at a token boundary.
    if start > 0 {
        let prev = src[..start].chars().last().unwrap_or('\n');
        if prev.is_ascii_alphanumeric() || prev == '_' {
            return None;
        }
    }
    let rest = &src[start..];
    let ident = match_ident_at(rest)?;
    let mut cursor = start + ident.len();
    while cursor < src.len() && src.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if src.as_bytes().get(cursor) != Some(&b'=') {
        return None;
    }
    cursor += 1;
    while cursor < src.len() && src.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if !src[cursor..].starts_with("tilemap") {
        return None;
    }
    cursor += "tilemap".len();
    while cursor < src.len() && src.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if src.as_bytes().get(cursor) != Some(&b'(') {
        return None;
    }
    Some((ident.to_string(), cursor))
}

fn match_ident_at(s: &str) -> Option<&str> {
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut end = first.len_utf8();
    for (i, c) in chars {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    // Must be at a token boundary behind (start of string, or non-ident char before).
    Some(&s[..end])
}

fn find_matching(src: &str, open_idx: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, c) in src[open_idx..].char_indices() {
        let abs = open_idx + i;
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(abs);
            }
        }
    }
    None
}

fn parse_rows_arg(call_inner: &str) -> Result<Vec<String>, String> {
    let key = "rows:";
    let start = call_inner
        .find(key)
        .ok_or_else(|| "tilemap needs rows: [...]".to_string())?;
    let after = call_inner[start + key.len()..].trim_start();
    if !after.starts_with('[') {
        return Err("rows: must be a list of text".into());
    }
    // Index of '[' in call_inner
    let bracket = start + key.len() + (call_inner[start + key.len()..].len() - after.len());
    let close = find_matching(call_inner, bracket, '[', ']')
        .ok_or_else(|| "rows: list is missing `]`".to_string())?;
    let list_inner = &call_inner[bracket + 1..close];
    parse_string_list(list_inner)
}

fn parse_string_list(inner: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut in_str = false;
    let mut escape = false;
    let mut buf = String::new();
    for c in inner.chars() {
        if in_str {
            if escape {
                buf.push(c);
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
                continue;
            }
            if c == '"' {
                in_str = false;
                out.push(std::mem::take(&mut buf));
                continue;
            }
            buf.push(c);
            continue;
        }
        if c == '"' {
            in_str = true;
        }
    }
    if in_str {
        return Err("unterminated text in rows list".into());
    }
    Ok(out)
}

fn load_solids(src: &str, map_name: &str, call_inner: &str) -> Result<(Vec<char>, SolidLoc), String> {
    if let Some(chars) = parse_solid_tiles_arg(call_inner) {
        return Ok((chars?, SolidLoc::InTilemap));
    }
    // world.add_tilemap(level, solid_tiles: [...])
    if let Some(inner) = find_add_tilemap_options(src, map_name) {
        if let Some(chars) = parse_solid_tiles_arg(inner) {
            return Ok((chars?, SolidLoc::InAddTilemap));
        }
    }
    Ok((Vec::new(), SolidLoc::Missing))
}

fn parse_solid_tiles_arg(inner: &str) -> Option<Result<Vec<char>, String>> {
    let key = "solid_tiles:";
    let start = inner.find(key)?;
    let after = inner[start + key.len()..].trim_start();
    if after.starts_with('[') {
        let bracket = start + key.len() + (inner[start + key.len()..].len() - after.len());
        let close = match find_matching(inner, bracket, '[', ']') {
            Some(c) => c,
            None => return Some(Err("solid_tiles: list is missing `]`".into())),
        };
        let list_inner = &inner[bracket + 1..close];
        return Some(parse_string_list(list_inner).and_then(|strs| {
            let mut chars = Vec::new();
            for s in strs {
                let mut cs = s.chars();
                let Some(ch) = cs.next() else {
                    return Err("solid_tiles entries must be one character".into());
                };
                if cs.next().is_some() {
                    return Err(format!("solid_tiles entry \"{}\" should be one character", s));
                }
                chars.push(ch);
            }
            Ok(chars)
        }));
    }
    // solid_tiles: "#"
    if after.starts_with('"') {
        let s = parse_one_string(after)?;
        return Some(Ok(s.chars().collect()));
    }
    None
}

fn parse_one_string(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('"') {
        return None;
    }
    let mut buf = String::new();
    let mut escape = false;
    for c in s[1..].chars() {
        if escape {
            buf.push(c);
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            return Some(buf);
        }
        buf.push(c);
    }
    None
}

fn find_add_tilemap_options<'a>(src: &'a str, map_name: &str) -> Option<&'a str> {
    // add_tilemap(name
    let needle = format!("add_tilemap({}", map_name);
    let idx = src.find(&needle)?;
    let open = src[idx..].find('(')? + idx;
    let close = find_matching(src, open, '(', ')')?;
    Some(&src[open + 1..close])
}

fn load_tiles_dict(src: &str, map_name: &str) -> Result<BTreeMap<char, String>, String> {
    let dict_name = format!("{}_tiles", map_name);
    match find_dict_assignment(src, &dict_name) {
        Some(inner) => parse_char_path_dict(inner),
        None => Ok(BTreeMap::new()),
    }
}

fn find_dict_assignment<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    // name = dictionary {
    let mut search = 0;
    while let Some(rel) = src[search..].find(name) {
        let abs = search + rel;
        // token boundary before name
        if abs > 0 {
            let prev = src[..abs].chars().last().unwrap_or(' ');
            if prev.is_ascii_alphanumeric() || prev == '_' {
                search = abs + name.len();
                continue;
            }
        }
        let after = src[abs + name.len()..].trim_start();
        if !after.starts_with('=') {
            search = abs + name.len();
            continue;
        }
        let after_eq = after[1..].trim_start();
        if !after_eq.starts_with("dictionary") {
            search = abs + name.len();
            continue;
        }
        let after_dict = after_eq["dictionary".len()..].trim_start();
        // optional `of K to V`
        let after_dict = if after_dict.starts_with("of ") {
            // skip until `{`
            let brace = after_dict.find('{')?;
            &after_dict[brace..]
        } else {
            after_dict
        };
        if !after_dict.starts_with('{') {
            search = abs + name.len();
            continue;
        }
        // Find `{` absolute index
        let brace_rel = src[abs..].find('{')?;
        let brace = abs + brace_rel;
        let close = find_matching(src, brace, '{', '}')?;
        return Some(&src[brace + 1..close]);
    }
    None
}

fn parse_char_path_dict(inner: &str) -> Result<BTreeMap<char, String>, String> {
    let mut map = BTreeMap::new();
    // Scan "key": "value" pairs
    let mut i = 0;
    let chars: Vec<char> = inner.chars().collect();
    while i < chars.len() {
        // skip whitespace and commas
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        if chars[i] != '"' {
            // skip unknown token
            i += 1;
            continue;
        }
        let (key, next) = read_string_from(&chars, i)?;
        i = next;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || chars[i] != ':' {
            continue;
        }
        i += 1;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || chars[i] != '"' {
            return Err("tile image paths must be text".into());
        }
        let (val, next) = read_string_from(&chars, i)?;
        i = next;
        let mut kc = key.chars();
        let Some(ch) = kc.next() else {
            continue;
        };
        if kc.next().is_some() {
            return Err(format!("tile key \"{}\" should be one character", key));
        }
        map.insert(ch, val);
    }
    Ok(map)
}

fn read_string_from(chars: &[char], start: usize) -> Result<(String, usize), String> {
    if chars.get(start) != Some(&'"') {
        return Err("expected text".into());
    }
    let mut buf = String::new();
    let mut i = start + 1;
    let mut escape = false;
    while i < chars.len() {
        let c = chars[i];
        if escape {
            buf.push(c);
            escape = false;
            i += 1;
            continue;
        }
        if c == '\\' {
            escape = true;
            i += 1;
            continue;
        }
        if c == '"' {
            return Ok((buf, i + 1));
        }
        buf.push(c);
        i += 1;
    }
    Err("unterminated text".into())
}

fn replace_tilemap_rows(src: &str, map_name: &str, rows: &[String]) -> Result<String, String> {
    let (found_name, _) = find_tilemap_assignment(src)?;
    if found_name != map_name {
        return Err(format!(
            "expected tilemap named `{}`, found `{}`",
            map_name, found_name
        ));
    }
    // Re-find with indices into full src
    let (open, close) = tilemap_call_range(src, map_name)?;
    let inner = &src[open + 1..close];
    let key = "rows:";
    let start = inner
        .find(key)
        .ok_or_else(|| "tilemap needs rows: [...]".to_string())?;
    let after = inner[start + key.len()..].trim_start();
    let bracket = start + key.len() + (inner[start + key.len()..].len() - after.len());
    let list_close = find_matching(inner, bracket, '[', ']')
        .ok_or_else(|| "rows: list is missing `]`".to_string())?;

    let abs_bracket = open + 1 + bracket;
    let abs_list_close = open + 1 + list_close;
    let mut rendered = String::from("[\n");
    for row in rows {
        rendered.push_str("    \"");
        rendered.push_str(&escape_pt_string(row));
        rendered.push_str("\",\n");
    }
    rendered.push(']');

    let mut out = String::new();
    out.push_str(&src[..abs_bracket]);
    out.push_str(&rendered);
    out.push_str(&src[abs_list_close + 1..]);
    Ok(out)
}

fn tilemap_call_range(src: &str, map_name: &str) -> Result<(usize, usize), String> {
    for (i, _) in src.char_indices() {
        if let Some((name, open)) = match_ident_eq_tilemap(src, i) {
            if name == map_name {
                let close = find_matching(src, open, '(', ')')
                    .ok_or_else(|| "tilemap(...) is missing a closing `)`".to_string())?;
                return Ok((open, close));
            }
        }
    }
    Err(format!("no `{} = tilemap(...)` found", map_name))
}

fn rewrite_solids(src: &str, doc: &TilemapDoc) -> Result<String, String> {
    let rendered = render_solid_list(&doc.solid_tiles);
    match doc.solid_loc {
        SolidLoc::InTilemap | SolidLoc::Missing => {
            let (open, close) = tilemap_call_range(src, &doc.map_name)?;
            let inner = &src[open + 1..close];
            if let Some((a, b)) = solid_tiles_list_range(inner) {
                let abs_a = open + 1 + a;
                let abs_b = open + 1 + b;
                let mut out = String::new();
                out.push_str(&src[..abs_a]);
                out.push_str(&rendered);
                out.push_str(&src[abs_b + 1..]);
                Ok(out)
            } else {
                // Insert before closing paren: `, solid_tiles: [...]`
                let mut out = String::new();
                out.push_str(&src[..close]);
                let trim_inner = inner.trim_end();
                if !trim_inner.is_empty() && !trim_inner.ends_with(',') {
                    out.push(',');
                }
                out.push_str(" solid_tiles: ");
                out.push_str(&rendered);
                out.push_str(&src[close..]);
                Ok(out)
            }
        }
        SolidLoc::InAddTilemap => {
            let needle = format!("add_tilemap({}", doc.map_name);
            let idx = src
                .find(&needle)
                .ok_or_else(|| "add_tilemap call not found for solid_tiles".to_string())?;
            let open = src[idx..].find('(').map(|p| idx + p).unwrap();
            let close = find_matching(src, open, '(', ')')
                .ok_or_else(|| "add_tilemap(...) is missing `)`".to_string())?;
            let inner = &src[open + 1..close];
            if let Some((a, b)) = solid_tiles_list_range(inner) {
                let abs_a = open + 1 + a;
                let abs_b = open + 1 + b;
                let mut out = String::new();
                out.push_str(&src[..abs_a]);
                out.push_str(&rendered);
                out.push_str(&src[abs_b + 1..]);
                Ok(out)
            } else {
                Err("add_tilemap is missing solid_tiles: [...]".into())
            }
        }
    }
}

fn solid_tiles_list_range(inner: &str) -> Option<(usize, usize)> {
    let key = "solid_tiles:";
    let start = inner.find(key)?;
    let after = inner[start + key.len()..].trim_start();
    if !after.starts_with('[') {
        return None;
    }
    let bracket = start + key.len() + (inner[start + key.len()..].len() - after.len());
    let close = find_matching(inner, bracket, '[', ']')?;
    Some((bracket, close))
}

fn render_solid_list(chars: &[char]) -> String {
    let parts: Vec<String> = chars
        .iter()
        .map(|c| format!("\"{}\"", escape_pt_string(&c.to_string())))
        .collect();
    format!("[{}]", parts.join(", "))
}

fn rewrite_tiles_dict(src: &str, doc: &TilemapDoc) -> Result<String, String> {
    let dict_name = format!("{}_tiles", doc.map_name);
    let body = render_tiles_dict(&doc.tiles);
    if let Some((brace, close)) = dict_brace_range(src, &dict_name) {
        let mut out = String::new();
        out.push_str(&src[..brace + 1]);
        out.push_str(&body);
        out.push_str(&src[close..]);
        Ok(out)
    } else {
        // Insert after the tilemap assignment statement.
        let (_, close) = tilemap_call_range(src, &doc.map_name)?;
        // End of statement: after `)` maybe newline
        let insert_at = close + 1;
        let mut out = String::new();
        out.push_str(&src[..insert_at]);
        out.push_str("\n\n");
        out.push_str(&dict_name);
        out.push_str(" = dictionary {");
        out.push_str(&body);
        out.push_str("}\n");
        out.push_str(&src[insert_at..]);
        Ok(out)
    }
}

fn dict_brace_range(src: &str, name: &str) -> Option<(usize, usize)> {
    let mut search = 0;
    while let Some(rel) = src[search..].find(name) {
        let abs = search + rel;
        if abs > 0 {
            let prev = src[..abs].chars().last().unwrap_or(' ');
            if prev.is_ascii_alphanumeric() || prev == '_' {
                search = abs + name.len();
                continue;
            }
        }
        let after = src[abs + name.len()..].trim_start();
        if !after.starts_with('=') {
            search = abs + name.len();
            continue;
        }
        let after_eq = after[1..].trim_start();
        if !after_eq.starts_with("dictionary") {
            search = abs + name.len();
            continue;
        }
        let brace = abs + src[abs..].find('{')?;
        let close = find_matching(src, brace, '{', '}')?;
        return Some((brace, close));
    }
    None
}

fn render_tiles_dict(tiles: &BTreeMap<char, String>) -> String {
    if tiles.is_empty() {
        return " ".to_string();
    }
    let mut s = String::from("\n");
    for (ch, path) in tiles {
        s.push_str("    \"");
        s.push_str(&escape_pt_string(&ch.to_string()));
        s.push_str("\": \"");
        s.push_str(&escape_pt_string(path));
        s.push_str("\",\n");
    }
    s
}

fn escape_pt_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Turn an absolute dropped path into a `/`-separated path relative to `base` when possible.
pub fn relativize_path(path: &std::path::Path, base: &std::path::Path) -> String {
    let abs = simplify(&path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
    let base_abs = simplify(&base.canonicalize().unwrap_or_else(|_| base.to_path_buf()));
    if let Ok(rel) = abs.strip_prefix(&base_abs) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    abs.to_string_lossy().replace('\\', "/")
}

/// Drop Windows' `\\?\` verbatim prefix that `canonicalize()` adds, so an
/// absolute fallback path reads as `C:/foo` rather than the unusable `//?/C:/foo`.
/// No-op for non-verbatim paths and on other platforms.
fn simplify(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{}", rest));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return std::path::PathBuf::from(rest);
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "import gamekit\n\nlevel = tilemap(cell_size: 40, rows: [\n    \"####\",\n    \"#.P#\",\n    \"####\",\n])\n\nworld = physics_world(gravity: 2200)\nworld.add_tilemap(level, solid_tiles: [\"#\"])\n";

    #[test]
    fn loads_rows_and_add_tilemap_solids() {
        let doc = load_from_source(SAMPLE).unwrap();
        assert_eq!(doc.map_name, "level");
        assert_eq!(doc.rows.len(), 3);
        assert_eq!(doc.rows[1], "#.P#");
        assert_eq!(doc.solid_tiles, vec!['#']);
        assert_eq!(doc.solid_loc, SolidLoc::InAddTilemap);
        assert!(doc.tiles.is_empty());
    }

    #[test]
    fn rewrite_rows_preserves_surroundings() {
        let mut doc = load_from_source(SAMPLE).unwrap();
        doc.set_tile(2, 1, 'X'); // "#.P#" → "#.X#"
        let out = apply_to_source(SAMPLE, &doc).unwrap();
        assert!(out.contains("import gamekit"));
        assert!(out.contains("world.add_tilemap(level, solid_tiles: [\"#\"])"));
        assert!(
            out.contains("\"#.X#\""),
            "rewritten source missing painted row:\n{out}"
        );
        let again = load_from_source(&out).unwrap();
        assert_eq!(again.tile_at(2, 1), 'X');
    }

    #[test]
    fn creates_tiles_dict_on_save() {
        let mut doc = load_from_source(SAMPLE).unwrap();
        doc.tiles.insert('#', "examples/assets/wall.png".into());
        let out = apply_to_source(SAMPLE, &doc).unwrap();
        assert!(out.contains("level_tiles = dictionary"));
        assert!(out.contains("\"#\": \"examples/assets/wall.png\""));
        let again = load_from_source(&out).unwrap();
        assert_eq!(
            again.tiles.get(&'#').map(String::as_str),
            Some("examples/assets/wall.png")
        );
    }

    #[test]
    fn insert_starter_then_load() {
        let src = "import gamekit\n\nprint(\"hi\")\n";
        let with = insert_starter(src);
        let doc = load_from_source(&with).unwrap();
        assert_eq!(doc.map_name, "level");
        assert!(doc.is_solid('#'));
        assert!(with.contains("print(\"hi\")"));
    }

    #[test]
    fn solid_tiles_on_tilemap() {
        let src = "\nm = tilemap(cell_size: 16, rows: [\"##\", \"##\"], solid_tiles: [\"#\"])\n";
        let doc = load_from_source(src).unwrap();
        assert_eq!(doc.solid_loc, SolidLoc::InTilemap);
        let mut doc = doc;
        doc.set_solid('.', true);
        let out = apply_to_source(src, &doc).unwrap();
        assert!(out.contains("solid_tiles: [\"#\", \".\"]") || out.contains("\".\""));
        let again = load_from_source(&out).unwrap();
        assert!(again.is_solid('.'));
    }

    #[test]
    fn resize_adds_and_removes_columns_and_rows() {
        let mut doc = load_from_source(SAMPLE).unwrap(); // 3 rows, width 4
        assert_eq!((doc.width(), doc.height()), (4, 3));
        assert!(doc.add_column());
        assert_eq!(doc.width(), 5);
        for r in &doc.rows {
            assert_eq!(r.chars().count(), 5);
        }
        assert!(doc.add_row());
        assert_eq!(doc.height(), 4);
        assert_eq!(doc.rows.last().unwrap(), ".....");
        assert!(doc.remove_column());
        assert_eq!(doc.width(), 4);
        assert!(doc.remove_row());
        assert_eq!(doc.height(), 3);
    }

    #[test]
    fn resize_keeps_a_minimum_of_one() {
        let src = "\nm = tilemap(cell_size: 8, rows: [\"#\"])\n";
        let mut doc = load_from_source(src).unwrap();
        assert_eq!((doc.width(), doc.height()), (1, 1));
        assert!(!doc.remove_column());
        assert!(!doc.remove_row());
        assert_eq!((doc.width(), doc.height()), (1, 1));
    }

    #[test]
    fn resized_grid_survives_round_trip() {
        let mut doc = load_from_source(SAMPLE).unwrap();
        doc.add_row();
        doc.add_column();
        let out = apply_to_source(SAMPLE, &doc).unwrap();
        let again = load_from_source(&out).unwrap();
        assert_eq!((again.width(), again.height()), (5, 4));
    }

    #[test]
    fn simplify_strips_windows_verbatim_prefix() {
        // The `\\?\` prefix canonicalize() adds on Windows must not survive into
        // a saved tile path (it used to render as the unusable `//?/C:/...`).
        assert_eq!(
            simplify(std::path::Path::new(r"\\?\C:\a\b.png")).to_string_lossy(),
            r"C:\a\b.png"
        );
        assert_eq!(
            simplify(std::path::Path::new(r"\\?\UNC\srv\share\x.png")).to_string_lossy(),
            r"\\srv\share\x.png"
        );
        assert_eq!(
            simplify(std::path::Path::new("plain/x.png")).to_string_lossy(),
            "plain/x.png"
        );
    }

    #[test]
    fn loads_file_with_unicode_dashes_in_comments() {
        // Em dash in a comment used to panic (byte-scan mid-character).
        let src = "// fill level_tiles — then play\n\nlevel = tilemap(cell_size: 8, rows: [\"#P\"])\n";
        let doc = load_from_source(src).unwrap();
        assert_eq!(doc.map_name, "level");
        assert_eq!(doc.rows, vec!["#P".to_string()]);
    }
}
