use std::fs::File;
use std::path::Path;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
}

impl Tags {
    fn absorb(&mut self, revision: &MetadataRevision) {
        for tag in revision.tags() {
            let value = tag.value.to_string().trim().to_owned();
            if value.is_empty() {
                continue;
            }
            let slot = match tag.std_key {
                Some(StandardTagKey::TrackTitle) => &mut self.title,
                Some(StandardTagKey::Album) => &mut self.album,
                Some(StandardTagKey::Artist) => &mut self.artist,
                _ => continue,
            };
            slot.get_or_insert(value);
        }
    }
}

/// Never fails: an unreadable or untagged file yields empty tags.
pub fn read_tags(path: &Path) -> Tags {
    let mut tags = Tags::default();
    let Ok(file) = File::open(path) else {
        return tags;
    };
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let Ok(mut probed) = symphonia::default::get_probe().format(
        &hint,
        stream,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return tags;
    };
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            tags.absorb(revision);
        }
    }
    if let Some(revision) = probed.format.metadata().current() {
        tags.absorb(revision);
    }
    tags
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/metadata.rs"]
mod tests;
