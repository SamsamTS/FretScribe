//! Hammer-ons, pull-offs and grace notes: the note editor demo loaded with every case.
//! Select a note and press H (hammer-on / pull-off from the previous note) or G (grace note entry).

#[path = "note_editor_demo.rs"]
mod demo;

fn main() -> eframe::Result {
    demo::run(true)
}
