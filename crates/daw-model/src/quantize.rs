// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving notes onto a rhythmic grid.

use crate::song::Note;
use crate::time::Ticks;

/// Moves each note's start to the nearest point of `grid`, keeping its
/// length; a note that would end up shorter than a quarter of the grid is
/// lengthened to that, so it stays visible and audible.
pub fn quantize_starts(notes: &mut [Note], grid: Ticks) {
    if grid <= Ticks::ZERO {
        return;
    }
    let shortest = Ticks((grid.0 / 4).max(1));
    for note in notes {
        note.start = note.start.round_to(grid).max(Ticks::ZERO);
        note.length = note.length.max(shortest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_VELOCITY, NoteId};

    #[test]
    fn starts_move_to_the_nearest_grid_point() {
        let sixteenth = Ticks::note_value(1, 16);
        let make = |start: i64, length: i64| Note {
            id: NoteId(0),
            start: Ticks(start),
            length: Ticks(length),
            key: 60,
            velocity: DEFAULT_VELOCITY,
            pitch_offset: 0.0,
        };
        let mut notes = [
            make(sixteenth.0 + 10, 1000),
            make(sixteenth.0 * 2 - 10, sixteenth.0),
            make(-5, 1),
        ];
        quantize_starts(&mut notes, sixteenth);
        assert_eq!(notes[0].start, sixteenth);
        assert_eq!(notes[1].start, sixteenth * 2);
        assert_eq!(notes[1].length, sixteenth, "lengths are kept");
        assert_eq!(notes[2].start, Ticks::ZERO);
        assert_eq!(
            notes[0].length,
            Ticks(sixteenth.0 / 4),
            "very short notes are lengthened"
        );
    }
}
