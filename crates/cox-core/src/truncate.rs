//! Lossless, line-safe tool-result truncation. Kept in core because tools
//! return complete output and only the loop decides what a model may see.

use cox_protocol::ArchiveId;

/// Shortens `text` at line boundaries, retaining leading and trailing lines.
pub(crate) fn visible(
    text: &str,
    id: ArchiveId,
    max_bytes: usize,
    head_lines: usize,
    tail_lines: usize,
) -> String {
    if text.len() <= max_bytes {
        return text.into();
    }
    let lines: Vec<&str> = text.lines().collect();
    let head = head_lines.min(lines.len());
    let tail_start = lines.len().saturating_sub(tail_lines).max(head);
    let trailer = format!(
        "[… {} KiB archived; expand #{id} lines {}–{}]",
        text.len().div_ceil(1024),
        head + 1,
        tail_start
    );
    let mut out = lines[..head].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&trailer);
    if tail_start < lines.len() {
        out.push('\n');
        out.push_str(&lines[tail_start..].join("\n"));
    }
    while out.len() > max_bytes && out.contains('\n') {
        let cut = out.rfind('\n').unwrap_or(0);
        out.truncate(cut);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_head_tail_and_archive_handle() {
        let text = (1..=100)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let id = ArchiveId::new();
        let result = visible(&text, id, 200, 2, 2);
        assert!(result.contains("line 1"));
        assert!(result.contains("line 100"));
        assert!(result.contains(&id.to_string()));
    }
}
