//! Standalone SVG snapshots of the actual peeling trace.

use std::fmt::Write;

use crate::hash::{checksum, locations};
use crate::{Cell, Config, DecodeTrace, PeelStep};

const TILE_WIDTH: usize = 148;
const TILE_HEIGHT: usize = 78;
const TILE_GAP: usize = 8;
const REGION_COLUMNS: usize = 3;
const REGION_WIDTH: usize = REGION_COLUMNS * (TILE_WIDTH + TILE_GAP) + 24;
const MARGIN: usize = 28;
const WIDTH: usize = REGION_WIDTH * 3 + MARGIN * 2;

/// Render a self-contained SVG. Small traces show every step; larger traces
/// sample snapshots, explicitly label gaps, and always include initial/final state.
/// Every cell value comes from the trace, rather than reconstructing key membership.
pub fn render_svg(trace: &DecodeTrace) -> String {
    let snapshots = snapshot_indices(trace.steps.len(), trace.config.cells);
    let cells_per_region = trace.config.cells / 3;
    let rows = cells_per_region.div_ceil(REGION_COLUMNS);
    let snapshot_height = rows * (TILE_HEIGHT + TILE_GAP) + 138;
    let height = 188 + snapshots.len() * snapshot_height + 70;
    let residual = trace
        .final_cells
        .iter()
        .filter(|cell| !cell.is_zero())
        .count();
    let complete = residual == 0;
    let mut svg = String::with_capacity(snapshots.len() * trace.config.cells * 600);

    writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="title description">"#).unwrap();
    writeln!(
        svg,
        "<title id=\"title\">IBLT set reconciliation: {} peeling steps</title>",
        trace.steps.len()
    )
    .unwrap();
    writeln!(svg, "<desc id=\"description\">Three hash regions show signed recoverable keys, the cells updated by each selected peeling step, and the final residual. {}. Checksums make recovery probabilistic.</desc>", if complete { "Complete recovery: every residual field is zero" } else { "Incomplete recovery: nonzero residual cells remain" }).unwrap();
    svg.push_str(
        r#"<style>
text { font-family: ui-monospace, Consolas, monospace; fill: #172b4d; }
.heading { font-family: system-ui, sans-serif; font-size: 25px; font-weight: 700; }
.subheading { font-family: system-ui, sans-serif; font-size: 15px; }
.small { font-size: 11px; }
.celltext { font-size: 10px; }
.cellhead { font-size: 11px; font-weight: 700; }
.step { font-size: 15px; font-weight: 700; }
</style>
"#,
    );
    writeln!(
        svg,
        r##"<rect width="{WIDTH}" height="{height}" fill="#f5f8fc"/>"##
    )
    .unwrap();
    writeln!(svg, r#"<text x="{MARGIN}" y="40" class="heading">IBLT: peel the unordered set difference</text>"#).unwrap();
    writeln!(svg, r#"<text x="{MARGIN}" y="68" class="subheading">{} cells | seed {} | {} peeling steps | {} nonzero residual cells</text>"#, trace.config.cells, trace.config.seed, trace.steps.len(), residual).unwrap();
    writeln!(svg, r##"<text x="{MARGIN}" y="94" class="subheading" style="fill:{};font-weight:700">{}</text>"##,
        if complete { "#047857" } else { "#b91c1c" },
        if complete { "COMPLETE: every residual field is zero" } else { "INCOMPLETE: the decoder stalled with unresolved data" }).unwrap();

    let legend = [
        ("#ffffff", "#c3cfdd", "Empty"),
        ("#e6edf5", "#a8b7c9", "Mixed"),
        ("#d1fae5", "#059669", "A only (+)"),
        ("#dbeafe", "#2563eb", "B only (-)"),
        ("#ffffff", "#7c3aed", "Updated"),
        ("#ffffff", "#d97706", "Newly recoverable"),
    ];
    let mut legend_x = MARGIN;
    for (fill, stroke, label) in legend {
        writeln!(svg, r#"<rect x="{legend_x}" y="113" width="15" height="15" rx="3" fill="{fill}" stroke="{stroke}" stroke-width="2"/>"#).unwrap();
        writeln!(
            svg,
            r#"<text x="{}" y="125" class="small">{label}</text>"#,
            legend_x + 23
        )
        .unwrap();
        legend_x += label.len() * 7 + 52;
    }
    writeln!(svg, r#"<text x="{MARGIN}" y="155" class="small">A dark outline marks the source cell. Key/check fields are hexadecimal. A count of zero alone does not mean empty.</text>"#).unwrap();

    let mut current = trace.initial_cells.clone();
    let mut applied_steps = 0;
    let mut previous_snapshot = 0;
    for (snapshot_number, step_number) in snapshots.into_iter().enumerate() {
        while applied_steps < step_number {
            let step = &trace.steps[applied_steps];
            for (&index, &cell) in step.affected_cells.iter().zip(&step.cells_after) {
                current[index] = cell;
            }
            applied_steps += 1;
        }
        // Use the decoder's exact final state for the final panel.
        let cells = if step_number == trace.steps.len() {
            &trace.final_cells
        } else {
            &current
        };
        let step = step_number.checked_sub(1).map(|index| &trace.steps[index]);
        let y = 188 + snapshot_number * snapshot_height;
        let skipped = step_number.saturating_sub(previous_snapshot + 1);
        render_snapshot(
            &mut svg,
            trace.config,
            cells,
            step,
            step_number,
            trace.steps.len(),
            skipped,
            y,
        );
        previous_snapshot = step_number;
    }
    writeln!(svg, r#"<text x="{MARGIN}" y="{}" class="small">Completion is the decoder's residual check; use the CLI HashSet oracle result to independently verify the recovered keys.</text>"#, height - 34).unwrap();
    svg.push_str("</svg>\n");
    svg
}

fn snapshot_indices(steps: usize, cells: usize) -> Vec<usize> {
    if steps <= 12 {
        return (0..=steps).collect();
    }
    let count = (1200 / cells.max(1)).clamp(2, 8);
    (0..count)
        .map(|index| index * steps / (count - 1))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn render_snapshot(
    svg: &mut String,
    config: Config,
    cells: &[Cell],
    step: Option<&PeelStep>,
    step_number: usize,
    total_steps: usize,
    skipped: usize,
    y: usize,
) {
    let title = match step {
        Some(step) => format!(
            "Step {step_number}/{total_steps}: peeled {}0x{:016x} from cell {} ({})",
            if step.sign > 0 { "+" } else { "-" },
            step.key,
            step.source_cell,
            if step.sign > 0 { "A only" } else { "B only" }
        ),
        None => "Initial difference table: A minus B".to_owned(),
    };
    writeln!(
        svg,
        r#"<g data-step="{step_number}"><text x="{MARGIN}" y="{y}" class="step">{title}</text>"#
    )
    .unwrap();
    let nonzero = cells.iter().filter(|cell| !cell.is_zero()).count();
    let pure = cells
        .iter()
        .enumerate()
        .filter(|&(index, cell)| is_recoverable(config, index, cell))
        .count();
    let gap = if skipped == 0 {
        String::new()
    } else {
        format!(" | {skipped} intermediate steps omitted")
    };
    writeln!(svg, r#"<text x="{MARGIN}" y="{}" class="small">Nonzero cells: {nonzero} | Recoverable cells: {pure}{gap}</text>"#, y + 23).unwrap();
    if let Some(step) = step {
        let newly_pure = step
            .newly_recoverable
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(svg, r#"<text x="{MARGIN}" y="{}" class="small">Updated cells: {}, {}, {} | Newly recoverable cells: {}</text>"#,
            y + 43, step.affected_cells[0], step.affected_cells[1], step.affected_cells[2],
            if newly_pure.is_empty() { "none" } else { &newly_pure }).unwrap();
    }
    let cells_per_region = config.cells / 3;
    for region in 0..3 {
        let region_x = MARGIN + region * REGION_WIDTH;
        writeln!(
            svg,
            r#"<text x="{region_x}" y="{}" class="cellhead">HASH REGION {} (cells {}..{})</text>"#,
            y + 66,
            region + 1,
            region * cells_per_region,
            (region + 1) * cells_per_region - 1
        )
        .unwrap();
        for offset in 0..cells_per_region {
            let index = region * cells_per_region + offset;
            let x = region_x + offset % REGION_COLUMNS * (TILE_WIDTH + TILE_GAP);
            let tile_y = y + 78 + offset / REGION_COLUMNS * (TILE_HEIGHT + TILE_GAP);
            render_cell(
                svg,
                config,
                index,
                &cells[index],
                step,
                step_number == total_steps,
                x,
                tile_y,
            );
        }
    }
    svg.push_str("</g>\n");
}

#[allow(clippy::too_many_arguments)]
fn render_cell(
    svg: &mut String,
    config: Config,
    index: usize,
    cell: &Cell,
    step: Option<&PeelStep>,
    final_snapshot: bool,
    x: usize,
    y: usize,
) {
    let recoverable = is_recoverable(config, index, cell);
    let (fill, mut stroke) = if cell.is_zero() {
        ("#ffffff", "#c3cfdd")
    } else if recoverable && cell.count > 0 {
        ("#d1fae5", "#059669")
    } else if recoverable {
        ("#dbeafe", "#2563eb")
    } else if final_snapshot {
        ("#fee2e2", "#dc2626")
    } else {
        ("#e6edf5", "#a8b7c9")
    };
    let mut stroke_width = 1;
    let mut marker = "";
    if let Some(step) = step {
        if step.affected_cells.contains(&index) {
            stroke = "#7c3aed";
            stroke_width = 2;
        }
        if step.newly_recoverable.contains(&index) {
            stroke = "#d97706";
            stroke_width = 3;
            marker = " NEW";
        }
        if step.source_cell == index {
            stroke = "#172b4d";
            stroke_width = 3;
            marker = " SOURCE";
        }
    }
    writeln!(svg, r#"<g><title>Cell {index}: count {}, key XOR {:016x}, checksum XOR {:016x}{marker}</title><rect x="{x}" y="{y}" width="{TILE_WIDTH}" height="{TILE_HEIGHT}" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}"/>"#,
        cell.count, cell.key_xor, cell.checksum_xor).unwrap();
    writeln!(
        svg,
        r#"<text x="{}" y="{}" class="cellhead">#{index} c:{:+}</text>"#,
        x + 7,
        y + 15,
        cell.count
    )
    .unwrap();
    writeln!(
        svg,
        r#"<text x="{}" y="{}" class="celltext">key {:016x}</text>"#,
        x + 7,
        y + 33,
        cell.key_xor
    )
    .unwrap();
    writeln!(
        svg,
        r#"<text x="{}" y="{}" class="celltext">chk {:016x}</text>"#,
        x + 7,
        y + 49,
        cell.checksum_xor
    )
    .unwrap();
    let label = if recoverable {
        format!(
            "{}0x{:016x}",
            if cell.count > 0 { "+" } else { "-" },
            cell.key_xor
        )
    } else if cell.is_zero() {
        "empty".to_owned()
    } else {
        "unresolved".to_owned()
    };
    writeln!(
        svg,
        r#"<text x="{}" y="{}" class="celltext">{label}</text></g>"#,
        x + 7,
        y + 67
    )
    .unwrap();
}

fn is_recoverable(config: Config, index: usize, cell: &Cell) -> bool {
    matches!(cell.count, -1 | 1)
        && checksum(config.seed, cell.key_xor) == cell.checksum_xor
        && locations(config, cell.key_xor).contains(&index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Iblt;
    use std::collections::HashSet;

    #[test]
    fn signed_keys_and_complete_state_are_visible() {
        let config = Config::new(30, 42).unwrap();
        let a = Iblt::from_set(config, &HashSet::from([0])).unwrap();
        let b = Iblt::from_set(config, &HashSet::from([u64::MAX])).unwrap();
        let (_, trace) = a.subtract(&b).unwrap().decode_with_trace();
        let svg = render_svg(&trace);
        assert!(svg.contains("COMPLETE: every residual field is zero"));
        assert!(svg.contains("+0x0000000000000000"));
        assert!(svg.contains("-0xffffffffffffffff"));
        assert!(svg.contains("Newly recoverable cells:"));
        assert!(svg.ends_with("</svg>\n"));
        assert_eq!(svg, render_svg(&trace));
    }

    #[test]
    fn zero_count_with_xor_data_is_visibly_incomplete() {
        let config = Config::new(3, 1).unwrap();
        let cells = vec![
            Cell {
                count: 0,
                key_xor: 5,
                checksum_xor: 7
            };
            3
        ];
        let trace = DecodeTrace {
            config,
            initial_cells: cells.clone(),
            steps: vec![],
            final_cells: cells,
        };
        let svg = render_svg(&trace);
        assert!(svg.contains("INCOMPLETE: the decoder stalled with unresolved data"));
        assert!(svg.contains("3 nonzero residual cells"));
        assert!(svg.contains("unresolved"));
    }

    #[test]
    fn large_traces_keep_initial_and_final_snapshots() {
        let indices = snapshot_indices(100, 300);
        assert_eq!(indices.first(), Some(&0));
        assert_eq!(indices.last(), Some(&100));
        assert!(indices.len() <= 8);
        assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(snapshot_indices(3, 300), vec![0, 1, 2, 3]);
    }
}
