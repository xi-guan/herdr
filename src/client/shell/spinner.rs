use super::*;

/// the same comet the stacked pair walks, on one cell's perimeter: what turns is the gap.
const SINGLE_CELL: [&str; 8] = ["⢹", "⠻", "⠟", "⡏", "⣇", "⣦", "⣴", "⣸"];

// two stacked braille cells form a 2x8 dot grid, whose perimeter the ring's comet walks
const RING_LEFT: [u8; 4] = [0x01, 0x02, 0x04, 0x40];
const RING_RIGHT: [u8; 4] = [0x08, 0x10, 0x20, 0x80];
pub(super) const RING_STEPS: u64 = 18;
// much past half the loop the unlit stretch closes and a turning agent reads as a blocked one
const RING_COMET: u64 = 8;

pub(super) const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

pub(super) fn single_cell(frame: u64) -> &'static str {
    SINGLE_CELL[(frame % SINGLE_CELL.len() as u64) as usize]
}

/// which columns the comet lights at a step, and how far down; both at the turns, or it stalls.
fn ring_step(step: u64) -> (bool, bool, usize) {
    match step {
        0..=7 => (true, false, step as usize),
        8 => (true, true, 7),
        9..=16 => (false, true, (16 - step) as usize),
        _ => (true, true, 0),
    }
}

pub(super) fn ring_cells(frame: u64) -> [char; 2] {
    // three steps every two ticks keep the longer loop turning at the one-cell spinner's pace
    let frame = frame * 3 / 2;
    let mut masks = [0u8; 2];
    for tail in 0..RING_COMET {
        let step = (frame + RING_STEPS - tail) % RING_STEPS;
        let (left, right, pos) = ring_step(step);
        if left {
            masks[pos / 4] |= RING_LEFT[pos % 4];
        }
        if right {
            masks[pos / 4] |= RING_RIGHT[pos % 4];
        }
    }
    masks.map(|mask| char::from_u32(0x2800 + u32::from(mask)).unwrap_or(' '))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpinnerGlyph {
    Single,
    RingTop,
    RingBottom,
}

impl SpinnerGlyph {
    pub(super) fn symbol(self, frame: u64) -> String {
        match self {
            Self::Single => single_cell(frame).to_owned(),
            Self::RingTop => ring_cells(frame)[0].to_string(),
            Self::RingBottom => ring_cells(frame)[1].to_string(),
        }
    }
}

/// a turning mark the last compose drew, kept so a tick can repaint that one cell alone.
#[derive(Clone, Debug)]
pub(super) struct SpinnerCell {
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) glyph: SpinnerGlyph,
    /// the cell as it reached the screen; empty until the compose that drew it finishes
    pub(super) cell: Option<crate::protocol::CellData>,
}

/// the cells a tick repaints, and what the screen must still show there for that to be safe.
pub(crate) struct SpinnerRepaint {
    pub(crate) shown: Vec<crate::protocol::PaneSurfacePatchRow>,
    pub(crate) next: Vec<crate::protocol::PaneSurfacePatchRow>,
}

/// keeps only the marks the finished frame still shows; an overlay may have covered some.
pub(super) fn settle(cells: &mut Vec<SpinnerCell>, frame: &FrameData, spinner_frame: u64) {
    cells.retain_mut(|spinner| {
        let index = usize::from(spinner.y) * usize::from(frame.width) + usize::from(spinner.x);
        let Some(cell) = frame.cells.get(index).filter(|_| spinner.x < frame.width) else {
            return false;
        };
        if cell.symbol != spinner.glyph.symbol(spinner_frame) {
            return false;
        }
        spinner.cell = Some(cell.clone());
        true
    });
}

impl ClientShellState {
    /// advances the working marks; nothing on screen turning means no wake-up and no repaint.
    pub(crate) fn tick_agent_spinner(&mut self, now: std::time::Instant) -> Option<SpinnerRepaint> {
        if self.hits.spinner_cells.is_empty() {
            self.agent_spinner_deadline = None;
            return None;
        }
        if self
            .agent_spinner_deadline
            .is_some_and(|deadline| now < deadline)
        {
            return None;
        }
        self.agent_spinner_deadline = Some(now + INTERVAL);
        self.agent_spinner_frame = self.agent_spinner_frame.wrapping_add(1);
        let frame = self.agent_spinner_frame;
        let mut repaint = SpinnerRepaint {
            shown: Vec::with_capacity(self.hits.spinner_cells.len()),
            next: Vec::with_capacity(self.hits.spinner_cells.len()),
        };
        for spinner in &mut self.hits.spinner_cells {
            let Some(cell) = spinner.cell.as_mut() else {
                continue;
            };
            repaint.shown.push(crate::protocol::PaneSurfacePatchRow {
                x: spinner.x,
                y: spinner.y,
                cells: vec![cell.clone()],
            });
            cell.symbol = spinner.glyph.symbol(frame);
            repaint.next.push(crate::protocol::PaneSurfacePatchRow {
                x: spinner.x,
                y: spinner.y,
                cells: vec![cell.clone()],
            });
        }
        (!repaint.next.is_empty()).then_some(repaint)
    }

    pub(super) fn spinner_wake_at(&self) -> Option<std::time::Instant> {
        self.agent_spinner_deadline
            .filter(|_| !self.hits.spinner_cells.is_empty())
    }
}
